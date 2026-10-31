//! What Codex reads at machine level, for the machine Overview.
//!
//! Every location here is Codex's own and is only read:
//! - skills in `~/.agents/skills` and `~/.codex/skills`, the ones under
//!   `.system` being Codex's own;
//! - the plugins `config.toml` lists under `[plugins."<name>@<market>"]`,
//!   read from Codex's plugin cache;
//! - agents in `~/.codex/agents/*.toml`;
//! - MCP servers under `[mcp_servers.*]`;
//! - hooks in `hooks.json`;
//! - `~/.codex/AGENTS.md`, loaded whole in every session.
//!
//! Sessions are rollout files counted by modification time. A rollout's
//! first line (`session_meta`) says whether it is a subagent's thread
//! (`payload.parent_thread_id`), and nothing past it is ever read.

use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::Value;
use uze_core::{
    integration::{AttachmentReceipt, ManagedArtifact},
    inventory::{Claim, Definition, FoundResource, ResourceKind, Runs, SessionCounts},
};

/// A `session_meta` line carries Codex's base instructions and runs to
/// tens of kilobytes; past this it is not read, and the rollout counts.
const FIRST_LINE_LIMIT: u64 = 1024 * 1024;

/// Where Codex keeps what it reads, rooted at a home directory.
pub(super) struct CodexPlaces {
    pub agents_skills: PathBuf,
    pub codex_home: PathBuf,
}

impl CodexPlaces {
    fn codex_skills(&self) -> PathBuf {
        self.codex_home.join("skills")
    }

    fn agents(&self) -> PathBuf {
        self.codex_home.join("agents")
    }

    fn config(&self) -> PathBuf {
        self.codex_home.join("config.toml")
    }

    fn hooks(&self) -> PathBuf {
        self.codex_home.join("hooks.json")
    }

    fn instructions(&self) -> PathBuf {
        self.codex_home.join("AGENTS.md")
    }

    fn plugin_cache(&self) -> PathBuf {
        self.codex_home.join("plugins").join("cache")
    }

    fn sessions(&self) -> PathBuf {
        self.codex_home.join("sessions")
    }
}

/// Everything Codex reads at machine level. `receipts` are this
/// integration's own, read only to name a plugin or hook the way a receipt
/// would.
pub(super) fn inventory(
    places: &CodexPlaces,
    receipts: &[AttachmentReceipt],
) -> Vec<FoundResource> {
    let config = read_config(&places.config());
    let mut found = Vec::new();
    found.extend(skills_in(&places.agents_skills));
    found.extend(skills_in(&places.codex_skills()));
    found.extend(
        skills_in(&places.codex_skills().join(".system"))
            .into_iter()
            .map(FoundResource::built_in),
    );
    found.extend(plugins(places, config.as_ref(), receipts));
    found.extend(agents_in(&places.agents(), None));
    found.extend(mcp_servers(&places.config(), config.as_ref()));
    found.extend(hooks(&places.hooks(), receipts));
    if places.instructions().is_file() {
        found.push(FoundResource::new(
            ResourceKind::Rule,
            "AGENTS.md",
            Definition::Whole(places.instructions()),
        ));
    }
    found
}

/// Sessions with activity in each period, subagent threads excluded.
pub(super) fn session_counts(places: &CodexPlaces, now: SystemTime) -> SessionCounts {
    let mut times = Vec::new();
    collect_rollouts(&places.sessions(), &mut times);
    SessionCounts::from_activity(times, now)
}

fn collect_rollouts(dir: &Path, times: &mut Vec<SystemTime>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_rollouts(&path, times);
            continue;
        }
        let is_rollout = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"));
        if !is_rollout || is_subagent_thread(&path) {
            continue;
        }
        if let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) {
            times.push(modified);
        }
    }
}

/// Whether the rollout's opening `session_meta` names a parent thread.
/// Only that first line is read; a line that cannot be read or parsed
/// leaves the rollout counted as a session of its own.
fn is_subagent_thread(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut line = String::new();
    let mut reader = BufReader::new(file.take(FIRST_LINE_LIMIT));
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|meta| {
            meta.pointer("/payload/parent_thread_id")
                .map(|parent| !parent.is_null())
        })
        .unwrap_or(false)
}

fn skills_in(dir: &Path) -> Vec<FoundResource> {
    sorted_entries(dir)
        .into_iter()
        .filter(|path| !hidden(path))
        .filter_map(|path| {
            let skill = path.join("SKILL.md");
            skill.is_file().then(|| {
                FoundResource::new(
                    ResourceKind::Skill,
                    file_name(&path),
                    Definition::Markdown(skill),
                )
            })
        })
        .collect()
}

/// Agents as Codex reads them: one TOML file each, whose `description`
/// is listed and whose `developer_instructions` load when it is used.
fn agents_in(dir: &Path, plugin: Option<&str>) -> Vec<FoundResource> {
    sorted_entries(dir)
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .map(|path| {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let found = match fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
            {
                Some(document) => {
                    let field = |key: &str| {
                        document
                            .get(key)
                            .and_then(|item| item.as_str())
                            .map(str::to_owned)
                    };
                    let name = field("name").unwrap_or(stem);
                    let per_session = [Some(name.clone()), field("description")]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("\n");
                    let mut found = FoundResource::new(
                        ResourceKind::Agent,
                        name,
                        Definition::Listed {
                            file: path.clone(),
                            per_session,
                            when_used: field("developer_instructions"),
                        },
                    );
                    if let Some(description) = field("description") {
                        found = found.noted(description);
                    }
                    found
                }
                None => {
                    FoundResource::new(ResourceKind::Agent, stem, Definition::Entry(path.clone()))
                        .with_problem("cannot be read")
                }
            };
            match plugin {
                Some(plugin) => found.from_plugin(plugin),
                None => found,
            }
        })
        .collect()
}

fn read_config(path: &Path) -> Option<toml_edit::DocumentMut> {
    fs::read_to_string(path).ok()?.parse().ok()
}

/// The plugins `config.toml` lists, read from Codex's plugin cache.
fn plugins(
    places: &CodexPlaces,
    config: Option<&toml_edit::DocumentMut>,
    receipts: &[AttachmentReceipt],
) -> Vec<FoundResource> {
    let Some(table) = config
        .and_then(|config| config.get("plugins"))
        .and_then(|item| item.as_table_like())
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (id, _) in table.iter() {
        let Some((name, market)) = id.split_once('@') else {
            continue;
        };
        let claim = owned_claim(receipts, id);
        let Some(root) = latest_version(&places.plugin_cache().join(market).join(name)) else {
            let mut missing = FoundResource::new(
                ResourceKind::Skill,
                name,
                Definition::Entry(places.plugin_cache().join(market).join(name)),
            )
            .from_plugin(name)
            .with_problem("plugin directory missing");
            if let Some(claim) = claim {
                missing = missing.claimed(claim);
            }
            found.push(missing);
            continue;
        };
        let carried = skills_in(&root.join("skills"))
            .into_iter()
            .chain(agents_in(&root.join("agents"), None))
            .map(|resource| {
                let qualified = format!("{name}:{}", resource.name);
                let mut resource = FoundResource {
                    name: qualified,
                    ..resource
                }
                .from_plugin(name)
                .removed_by(format!("codex plugin remove {id}"));
                if let Some(claim) = claim.clone() {
                    resource = resource.claimed(claim);
                }
                resource
            });
        found.extend(carried);
    }
    found
}

/// The claim a uze receipt makes on plugin `id`, if uze published it: the
/// kind is the receipt's own, explicit or generated.
fn owned_claim(receipts: &[AttachmentReceipt], id: &str) -> Option<Claim> {
    receipts.iter().find_map(|receipt| match &receipt.artifact {
        ManagedArtifact::IntegrationOwned { kind, selector, .. } if selector == id => {
            Some(Claim::Owned {
                kind: kind.clone(),
                selector: selector.clone(),
            })
        }
        _ => None,
    })
}

/// The newest version directory of a cached plugin.
fn latest_version(dir: &Path) -> Option<PathBuf> {
    sorted_entries(dir)
        .into_iter()
        .filter(|path| path.is_dir())
        .max_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        })
}

fn mcp_servers(config_path: &Path, config: Option<&toml_edit::DocumentMut>) -> Vec<FoundResource> {
    let Some(table) = config
        .and_then(|config| config.get("mcp_servers"))
        .and_then(|item| item.as_table_like())
    else {
        return Vec::new();
    };
    table
        .iter()
        .filter_map(|(name, item)| {
            let server = item.as_table_like()?;
            let field = |key: &str| {
                server
                    .get(key)
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            };
            let mut found = FoundResource::new(
                ResourceKind::Mcp,
                name,
                Definition::Entry(config_path.to_path_buf()),
            )
            .claimed(Claim::Entry(name.to_owned()))
            .removed_by(format!("codex mcp remove {name}"));
            if let Some(url) = field("url") {
                found = found
                    .running(Runs::Url(url))
                    .noted("reached over the network");
            } else if let Some(command) = field("command") {
                found = found
                    .running(Runs::Program(command))
                    .noted("started on this machine");
            }
            Some(found)
        })
        .collect()
}

/// One resource per command in `hooks.json`, named by the program it runs.
fn hooks(path: &Path, receipts: &[AttachmentReceipt]) -> Vec<FoundResource> {
    let Some(document) = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Vec::new();
    };
    let Some(events) = document.get("hooks").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (event, groups) in events {
        let commands = groups
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|group| group.get("hooks").and_then(Value::as_array))
            .flatten()
            .filter_map(|hook| hook.get("command").and_then(Value::as_str));
        for command in commands {
            let mut hook = FoundResource::new(
                ResourceKind::Hook,
                hook_name(command),
                Definition::Entry(path.to_path_buf()),
            )
            .running(Runs::Shell(command.to_owned()))
            .noted(format!("runs on {event}"));
            if let Some(claim) = rules_engine_claim(receipts, event, command) {
                hook = hook.claimed(claim);
            }
            found.push(hook);
        }
    }
    found
}

/// uze's rules engine is recorded as an integration-owned receipt per
/// event, whose detail names the engine it runs.
fn rules_engine_claim(receipts: &[AttachmentReceipt], event: &str, command: &str) -> Option<Claim> {
    receipts.iter().find_map(|receipt| match &receipt.artifact {
        ManagedArtifact::IntegrationOwned {
            kind,
            selector,
            detail,
        } if selector == event
            && detail
                .get("engine")
                .and_then(Value::as_str)
                .is_some_and(|engine| command.contains(engine)) =>
        {
            Some(Claim::Owned {
                kind: kind.clone(),
                selector: selector.clone(),
            })
        }
        _ => None,
    })
}

/// The file name of the program a command line runs, quotes taken off.
fn hook_name(command: &str) -> String {
    let program = command
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .trim_matches(|c| c == '\'' || c == '"');
    Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_owned())
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    entries
}

fn hidden(path: &Path) -> bool {
    file_name(path).starts_with('.')
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
