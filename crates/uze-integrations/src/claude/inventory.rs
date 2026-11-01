//! What Claude Code carries at machine level, for the machine Overview.
//!
//! Read from the files Claude Code itself reads at user scope, never from
//! a project: `~/.claude/{skills,agents,CLAUDE.md,settings.json}`, the
//! user-scope MCP servers in `~/.claude.json`, and the plugins its own
//! record (`plugins/installed_plugins.json`) says are installed for the
//! user and not switched off. Sessions are its transcripts' file times;
//! no transcript is opened.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;
use uze_core::{
    UzeHome,
    inventory::{Claim, Definition, FoundResource, ResourceKind, Runs, SessionCounts, UZE_RULES},
};

use super::plugin::ClaudeMarketplace;
use crate::shared::marketplace::{MarketplaceDialect, Origin as CatalogueOrigin};

/// The receipt kind uze's machine setup writes for its rules engine hook.
const RULES_ENGINE_KIND: &str = "rules-engine";

pub(super) struct ClaudeHome<'a> {
    /// `~/.claude`.
    pub config: &'a Path,
    /// The directory holding `.claude.json`.
    pub command_home: &'a Path,
    pub uze_home: &'a UzeHome,
}

pub(super) fn machine_inventory(home: &ClaudeHome<'_>) -> Vec<FoundResource> {
    let mut found = Vec::new();
    found.extend(skills_in(&home.config.join("skills"), None));
    found.extend(agents_in(&home.config.join("agents"), None));
    let instructions = home.config.join("CLAUDE.md");
    if instructions.is_file() {
        found.push(FoundResource::new(
            ResourceKind::Rule,
            "CLAUDE.md",
            Definition::Whole(instructions),
        ));
    }
    let settings_path = home.config.join("settings.json");
    let settings = read_json(&settings_path);
    let uze_root = home.uze_home.root().to_string_lossy();
    found.extend(hooks_in(
        &settings_path,
        settings.as_ref(),
        None,
        Some(&uze_root),
    ));
    found.extend(user_mcp_servers(home.command_home));
    found.extend(plugins(home.config, settings.as_ref()));
    found
}

pub(super) fn session_counts(transcripts_root: &Path) -> Option<SessionCounts> {
    let projects = fs::read_dir(transcripts_root).ok()?;
    let times = projects
        .flatten()
        .filter_map(|project| fs::read_dir(project.path()).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|entry| entry.metadata().ok()?.modified().ok());
    Some(SessionCounts::from_activity(times))
}

/// A plugin as Claude installed it, with what tells uze's own apart.
struct Plugin {
    key: String,
    name: String,
    root: PathBuf,
    claim: Option<Claim>,
}

impl Plugin {
    fn mark(&self, resource: FoundResource) -> FoundResource {
        let resource = resource.from_plugin(self.name.clone());
        match &self.claim {
            Some(claim) => resource.claimed(claim.clone()),
            None => resource.removed_by(format!("claude plugin uninstall {}", self.key)),
        }
    }
}

fn plugins(config: &Path, settings: Option<&Value>) -> Vec<FoundResource> {
    let Some(record) = read_json(&config.join("plugins/installed_plugins.json")) else {
        return Vec::new();
    };
    let Some(installed) = record.get("plugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (key, installs) in installed {
        if switched_off(settings, key) {
            continue;
        }
        let Some(root) = installs
            .as_array()
            .into_iter()
            .flatten()
            .find(|install| install.get("scope").and_then(Value::as_str) == Some("user"))
            .and_then(|install| install.get("installPath").and_then(Value::as_str))
        else {
            continue;
        };
        let (name, marketplace) = key.rsplit_once('@').unwrap_or((key.as_str(), ""));
        let plugin = Plugin {
            key: key.clone(),
            name: name.to_owned(),
            root: PathBuf::from(root),
            claim: uze_catalogue_kind(marketplace).map(|kind| Claim::Owned {
                kind: kind.to_owned(),
                selector: key.clone(),
            }),
        };
        if !plugin.root.is_dir() {
            found.push(
                plugin.mark(
                    FoundResource::new(
                        ResourceKind::Skill,
                        plugin.name.clone(),
                        Definition::Entry(plugin.root.clone()),
                    )
                    .noted("plugin")
                    .with_problem("plugin directory missing"),
                ),
            );
            continue;
        }
        found.extend(skills_in(&plugin.root.join("skills"), Some(&plugin)));
        found.extend(agents_in(&plugin.root.join("agents"), Some(&plugin)));
        found.extend(plugin_mcp_servers(&plugin));
        let hooks_path = plugin.root.join("hooks/hooks.json");
        let hooks = read_json(&hooks_path);
        found.extend(hooks_in(&hooks_path, hooks.as_ref(), Some(&plugin), None));
    }
    found
}

/// The receipt kind of a plugin in one of uze's own catalogues.
fn uze_catalogue_kind(marketplace: &str) -> Option<&'static str> {
    if marketplace == CatalogueOrigin::Generated.marketplace_name() {
        Some(<ClaudeMarketplace as MarketplaceDialect>::GENERATED_KIND)
    } else if marketplace == CatalogueOrigin::Explicit.marketplace_name() {
        Some(<ClaudeMarketplace as MarketplaceDialect>::EXPLICIT_KIND)
    } else {
        None
    }
}

fn switched_off(settings: Option<&Value>, key: &str) -> bool {
    settings
        .and_then(|settings| settings.get("enabledPlugins"))
        .and_then(|enabled| enabled.get(key))
        .and_then(Value::as_bool)
        == Some(false)
}

fn skills_in(root: &Path, plugin: Option<&Plugin>) -> Vec<FoundResource> {
    entries(root)
        .into_iter()
        .filter(|path| holds_a_skill(path))
        .filter_map(|dir| {
            let name = file_name(&dir)?;
            let skill = FoundResource::new(
                ResourceKind::Skill,
                qualified(plugin, &name),
                Definition::Markdown(dir.join("SKILL.md")),
            );
            Some(match plugin {
                Some(plugin) => plugin.mark(skill),
                None => skill,
            })
        })
        .collect()
}

/// A directory is a skill when it holds a `SKILL.md`; a link whose target
/// is gone still names one, and reads as broken.
fn holds_a_skill(path: &Path) -> bool {
    path.join("SKILL.md").is_file() || (path.is_symlink() && !path.exists())
}

fn agents_in(root: &Path, plugin: Option<&Plugin>) -> Vec<FoundResource> {
    entries(root)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|file| {
            let stem = file.file_stem()?.to_string_lossy().into_owned();
            let agent = FoundResource::new(
                ResourceKind::Agent,
                qualified(plugin, &stem),
                Definition::Markdown(file),
            );
            Some(match plugin {
                Some(plugin) => plugin.mark(agent),
                None => agent,
            })
        })
        .collect()
}

fn user_mcp_servers(command_home: &Path) -> Vec<FoundResource> {
    let path = super::mcp::user_config_path(command_home);
    let Some(config) = read_json(&path) else {
        return Vec::new();
    };
    servers(config.get("mcpServers"), &path)
        .into_iter()
        .map(|(name, server)| {
            server
                .claimed(Claim::Entry(name.clone()))
                .removed_by(format!("claude mcp remove --scope user {name}"))
        })
        .collect()
}

fn plugin_mcp_servers(plugin: &Plugin) -> Vec<FoundResource> {
    let path = plugin.root.join(".mcp.json");
    let Some(config) = read_json(&path) else {
        return Vec::new();
    };
    let declared = config.get("mcpServers").or(Some(&config));
    servers(declared, &path)
        .into_iter()
        .map(|(_, server)| plugin.mark(server))
        .collect()
}

fn servers(declared: Option<&Value>, config: &Path) -> Vec<(String, FoundResource)> {
    let Some(declared) = declared.and_then(Value::as_object) else {
        return Vec::new();
    };
    declared
        .iter()
        .filter_map(|(name, server)| {
            let server = server.as_object()?;
            let runs = match (server.get("url"), server.get("command")) {
                (Some(Value::String(url)), _) => Runs::Url(url.clone()),
                (_, Some(Value::String(command))) => Runs::Program(command.clone()),
                _ => return None,
            };
            let transport = server.get("type").and_then(Value::as_str).unwrap_or(
                if matches!(runs, Runs::Url(_)) {
                    "http"
                } else {
                    "stdio"
                },
            );
            let resource = FoundResource::new(
                ResourceKind::Mcp,
                name.clone(),
                Definition::Entry(config.to_path_buf()),
            )
            .running(runs)
            .noted(format!("{transport} server"));
            Some((name.clone(), resource))
        })
        .collect()
}

/// Every command hook in a `hooks` document, one resource each, named by
/// what it runs.
fn hooks_in(
    path: &Path,
    document: Option<&Value>,
    plugin: Option<&Plugin>,
    uze_root: Option<&str>,
) -> Vec<FoundResource> {
    let Some(events) = document
        .and_then(|document| document.get("hooks"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (event, groups) in events {
        for group in groups.as_array().into_iter().flatten() {
            let matcher = group
                .get("matcher")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty());
            for hook in group
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(command) = hook.get("command").and_then(Value::as_str) else {
                    continue;
                };
                let note = match matcher {
                    Some(matcher) => format!("on {event} · {matcher}"),
                    None => format!("on {event}"),
                };
                let mut resource = FoundResource::new(
                    ResourceKind::Hook,
                    qualified(plugin, &hook_label(command)),
                    Definition::Entry(path.to_path_buf()),
                )
                .running(Runs::Shell(command.to_owned()))
                .noted(note);
                if let Some(plugin) = plugin {
                    resource = plugin.mark(resource);
                } else if uze_root.is_some_and(|root| !root.is_empty() && command.contains(root)) {
                    resource.name = UZE_RULES.to_owned();
                    resource = resource.claimed(Claim::Owned {
                        kind: RULES_ENGINE_KIND.to_owned(),
                        selector: event.clone(),
                    });
                }
                found.push(resource);
            }
        }
    }
    found
}

/// A hook's name: the stem of the script it runs, or of its program.
fn hook_label(command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().collect();
    let script = words
        .iter()
        .skip(1)
        .find(|word| word.contains('/') && !word.starts_with('-'))
        .or(words.first());
    script
        .map(|word| word.trim_matches(|c| c == '"' || c == '\''))
        .and_then(|word| Path::new(word).file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "hook".to_owned())
}

fn qualified(plugin: Option<&Plugin>, name: &str) -> String {
    match plugin {
        Some(plugin) => format!("{}:{name}", plugin.name),
        None => name.to_owned(),
    }
}

fn entries(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(root)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    paths.sort();
    paths
}

fn file_name(path: &Path) -> Option<String> {
    Some(path.file_name()?.to_string_lossy().into_owned())
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests;
