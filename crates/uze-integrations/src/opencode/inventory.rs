//! What OpenCode carries at machine level, for the machine Overview.
//!
//! Read from its own global locations: the skills and agents directories,
//! the `mcp` section of `opencode.json`, the plugin directory (OpenCode's
//! hooks are plugins) and the global `AGENTS.md`. Nothing is written or run
//! to list them.
//!
//! Sessions are asked of `opencode stats`, the statistics the harness
//! publishes, never of its database: it counts sessions and subagents
//! apart, and is asked from a directory that is no project so the answer
//! is the machine's.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use uze_core::inventory::{Claim, Definition, FoundResource, ResourceKind, Runs, SessionCounts};

use crate::shared::process;

/// Long enough for the harness's cold start, short enough that the
/// Overview never waits on a harness that hangs.
const STATS_TIMEOUT: Duration = Duration::from_secs(10);

/// How UZE's rules plugin is named in its receipt.
const RULES_RECEIPT_KIND: &str = "rules-engine";
const RULES_RECEIPT_SELECTOR: &str = "plugin";

/// Everything OpenCode reads at machine level under `config_root`.
pub(super) fn inventory(config_root: &Path, config_path: &Path) -> Vec<FoundResource> {
    let mut found = Vec::new();
    found.extend(skills(&config_root.join("skills")));
    found.extend(agents(&config_root.join("agents")));
    let config = read_config(config_path);
    found.extend(mcp_servers(config.as_ref(), config_path));
    found.extend(plugin_files(&config_root.join("plugins")));
    found.extend(listed_plugins(config.as_ref(), config_path));
    let rules = config_root.join("AGENTS.md");
    if rules.is_file() {
        found.push(FoundResource::new(
            ResourceKind::Rule,
            "AGENTS.md",
            Definition::Whole(rules),
        ));
    }
    found
}

fn entries(directory: &Path) -> Vec<(String, PathBuf)> {
    let Ok(read) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut entries: Vec<(String, PathBuf)> = read
        .flatten()
        .filter_map(|entry| Some((entry.file_name().to_str()?.to_owned(), entry.path())))
        .filter(|(name, _)| !name.starts_with('.'))
        .collect();
    entries.sort();
    entries
}

fn skills(directory: &Path) -> Vec<FoundResource> {
    entries(directory)
        .into_iter()
        .filter(|(_, path)| {
            // A directory is a skill when it holds a `SKILL.md`; a link whose
            // target is gone still names one, and reads as broken.
            path.join("SKILL.md").is_file() || (path.is_symlink() && !path.exists())
        })
        .map(|(name, path)| {
            FoundResource::new(
                ResourceKind::Skill,
                name,
                Definition::Markdown(path.join("SKILL.md")),
            )
        })
        .collect()
}

fn agents(directory: &Path) -> Vec<FoundResource> {
    entries(directory)
        .into_iter()
        .filter_map(|(name, path)| {
            let stem = name.strip_suffix(".md")?.to_owned();
            Some(FoundResource::new(
                ResourceKind::Agent,
                stem,
                Definition::Markdown(path),
            ))
        })
        .collect()
}

fn read_config(config_path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&fs::read(config_path).ok()?).ok()
}

/// OpenCode V2 keeps servers under `mcp.servers`; an older file keeps them
/// under `mcp` itself.
fn mcp_servers(config: Option<&serde_json::Value>, config_path: &Path) -> Vec<FoundResource> {
    let Some(mcp) = config.and_then(|config| config.get("mcp")) else {
        return Vec::new();
    };
    let servers = mcp
        .get("servers")
        .and_then(serde_json::Value::as_object)
        .or_else(|| mcp.as_object());
    let Some(servers) = servers else {
        return Vec::new();
    };
    servers
        .iter()
        .filter(|(_, server)| server.is_object())
        .filter(|(_, server)| {
            server.get("type").is_some()
                || server.get("command").is_some()
                || server.get("url").is_some()
        })
        .map(|(name, server)| {
            let mut found = FoundResource::new(
                ResourceKind::Mcp,
                name.clone(),
                Definition::Entry(config_path.to_path_buf()),
            )
            .claimed(Claim::Entry(name.clone()));
            if let Some(runs) = server_runs(server) {
                found = found.running(runs);
            }
            if let Some(kind) = server.get("type").and_then(serde_json::Value::as_str) {
                found = found.noted(format!("{kind} server"));
            }
            found
        })
        .collect()
}

fn server_runs(server: &serde_json::Value) -> Option<Runs> {
    if let Some(url) = server.get("url").and_then(serde_json::Value::as_str) {
        return Some(Runs::Url(url.to_owned()));
    }
    match server.get("command")? {
        serde_json::Value::Array(parts) => parts
            .first()
            .and_then(serde_json::Value::as_str)
            .map(|program| Runs::Program(program.to_owned())),
        serde_json::Value::String(program) => Some(Runs::Program(program.clone())),
        _ => None,
    }
}

/// Every file in the plugin directory is a plugin, and OpenCode's hooks are
/// plugins. UZE's own is told apart by its receipt, never by its name.
fn plugin_files(directory: &Path) -> Vec<FoundResource> {
    entries(directory)
        .into_iter()
        .filter(|(_, path)| path.is_file())
        .filter(|(name, _)| {
            Path::new(name)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension, "ts" | "js" | "mjs" | "cjs"))
        })
        .map(|(name, path)| {
            let mut found =
                FoundResource::new(ResourceKind::Hook, name.clone(), Definition::Entry(path))
                    .noted("plugin");
            if name == super::runtime_plugin::FILE {
                found.name = uze_core::inventory::UZE_RULES.to_owned();
                found = found.claimed(Claim::Owned {
                    kind: RULES_RECEIPT_KIND.to_owned(),
                    selector: RULES_RECEIPT_SELECTOR.to_owned(),
                });
            }
            found
        })
        .collect()
}

/// Plugins `opencode.json` names by package, which OpenCode installs itself.
fn listed_plugins(config: Option<&serde_json::Value>, config_path: &Path) -> Vec<FoundResource> {
    let Some(listed) = config
        .and_then(|config| config.get("plugin"))
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    listed
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(|package| {
            FoundResource::new(
                ResourceKind::Hook,
                package,
                Definition::Entry(config_path.to_path_buf()),
            )
            .from_plugin(package)
            .noted("package plugin")
        })
        .collect()
}

/// Sessions per Overview period, as `opencode stats` counts them.
pub(super) fn session_counts(executable: &Path, home: &Path) -> Option<SessionCounts> {
    let outside_every_project = std::env::temp_dir();
    let ask = |days: u32| -> Option<u64> {
        let days = days.to_string();
        let output = process::capture_read_only_from(
            executable,
            home,
            &outside_every_project,
            &["stats", "--days", days.as_str(), "--json"],
            STATS_TIMEOUT,
        )
        .ok()?;
        if !output.status.success() {
            return None;
        }
        sessions_in(&output.stdout)
    };
    Some(SessionCounts {
        days_7: ask(7)?,
        days_30: ask(30)?,
        days_90: ask(90)?,
    })
}

/// The `sessions` count of a `stats --json` answer: subagents are counted
/// apart there, and are not sessions.
fn sessions_in(payload: &[u8]) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(payload)
        .ok()?
        .get("sessions")?
        .as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn names(found: &[FoundResource], kind: ResourceKind) -> Vec<String> {
        found
            .iter()
            .filter(|found| found.kind == kind)
            .map(|found| found.name.clone())
            .collect()
    }

    #[test]
    fn skills_are_the_directories_of_the_skills_root() {
        let root = uze_testkit::temp::TempDir::new("opencode-inventory-skills");
        write(
            &root.join("skills/review/SKILL.md"),
            "---\nname: review\n---\n",
        );
        write(&root.join("skills/.hidden/SKILL.md"), "");
        write(&root.join("skills/stray.txt"), "");
        let found = inventory(root.path(), &root.join("opencode.json"));
        assert_eq!(names(&found, ResourceKind::Skill), ["review"]);
        assert_eq!(
            found[0].definition,
            Definition::Markdown(root.join("skills/review/SKILL.md"))
        );
    }

    #[test]
    fn agents_are_the_markdown_files_of_the_agents_root() {
        let root = uze_testkit::temp::TempDir::new("opencode-inventory-agents");
        write(
            &root.join("agents/planner.md"),
            "---\ndescription: plans\n---\n",
        );
        write(&root.join("agents/notes.txt"), "");
        let found = inventory(root.path(), &root.join("opencode.json"));
        assert_eq!(names(&found, ResourceKind::Agent), ["planner"]);
    }

    #[test]
    fn mcp_servers_are_read_from_either_shape_of_the_config() {
        let root = uze_testkit::temp::TempDir::new("opencode-inventory-mcp");
        let config = root.join("opencode.json");
        write(
            &config,
            r#"{"mcp":{"servers":{
                "github":{"type":"local","command":["github-mcp","--stdio"]},
                "docs":{"type":"remote","url":"https://example.invalid/mcp"}
            }}}"#,
        );
        let found = inventory(root.path(), &config);
        let servers: Vec<_> = found
            .iter()
            .filter(|f| f.kind == ResourceKind::Mcp)
            .collect();
        assert_eq!(servers.len(), 2);
        let github = servers.iter().find(|f| f.name == "github").unwrap();
        assert_eq!(github.runs, Some(Runs::Program("github-mcp".into())));
        assert_eq!(github.claim, Some(Claim::Entry("github".into())));
        assert_eq!(github.removal, None);
        let docs = servers.iter().find(|f| f.name == "docs").unwrap();
        assert_eq!(
            docs.runs,
            Some(Runs::Url("https://example.invalid/mcp".into()))
        );

        write(
            &config,
            r#"{"mcp":{"legacy":{"type":"local","command":["legacy-mcp"]}}}"#,
        );
        let found = inventory(root.path(), &config);
        assert_eq!(names(&found, ResourceKind::Mcp), ["legacy"]);
    }

    #[test]
    fn plugins_are_hooks_and_uzes_is_claimed_by_its_receipt() {
        let root = uze_testkit::temp::TempDir::new("opencode-inventory-plugins");
        write(&root.join("plugins/uze.ts"), "// Generated by UZE");
        write(&root.join("plugins/guard.js"), "export default {}");
        write(&root.join("plugins/readme.md"), "");
        write(
            &root.join("opencode.json"),
            r#"{"plugin":["opencode-wakatime"]}"#,
        );
        let found = inventory(root.path(), &root.join("opencode.json"));
        assert_eq!(
            names(&found, ResourceKind::Hook),
            ["guard.js", "uze-rules", "opencode-wakatime"]
        );
        let uze = found.iter().find(|f| f.name == "uze-rules").unwrap();
        assert_eq!(
            uze.claim,
            Some(Claim::Owned {
                kind: "rules-engine".into(),
                selector: "plugin".into()
            })
        );
        let guard = found.iter().find(|f| f.name == "guard.js").unwrap();
        assert_eq!(guard.claim, None);
    }

    #[test]
    fn the_global_agents_md_is_a_rule_when_it_exists() {
        let root = uze_testkit::temp::TempDir::new("opencode-inventory-rules");
        assert!(
            names(
                &inventory(root.path(), &root.join("opencode.json")),
                ResourceKind::Rule
            )
            .is_empty()
        );
        write(&root.join("AGENTS.md"), "Be brief.");
        let found = inventory(root.path(), &root.join("opencode.json"));
        assert_eq!(names(&found, ResourceKind::Rule), ["AGENTS.md"]);
        assert_eq!(
            found
                .iter()
                .find(|f| f.kind == ResourceKind::Rule)
                .unwrap()
                .definition,
            Definition::Whole(root.join("AGENTS.md"))
        );
    }

    #[test]
    fn nothing_there_is_an_empty_inventory() {
        let root = uze_testkit::temp::TempDir::new("opencode-inventory-empty");
        assert!(inventory(root.path(), &root.join("opencode.json")).is_empty());
    }

    #[test]
    fn sessions_come_from_the_stats_answer_and_leave_subagents_out() {
        let payload = br#"{"range":{"from":1,"to":2},"sessions":12,"subagents":30,"prompts":40}"#;
        assert_eq!(sessions_in(payload), Some(12));
    }

    #[test]
    fn an_answer_without_sessions_is_unknown() {
        assert_eq!(sessions_in(b"{}"), None);
        assert_eq!(sessions_in(b"not json"), None);
        assert_eq!(sessions_in(br#"{"sessions":"many"}"#), None);
    }
}
