//! What Antigravity CLI reads at machine level, for the machine Overview.
//!
//! Read from the files agy itself keeps, never by asking it and never by
//! opening a conversation's database:
//! - skills under `~/.gemini/antigravity-cli/skills` and, since 1.2, the
//!   `~/.gemini/config/skills` it links back to (one copy counted once);
//! - the skills agy ships, under `antigravity-cli/builtin/skills`;
//! - agents under `antigravity-cli/agents` and `config/agents`;
//! - plugins staged under `config/plugins/<name>`, with their skills,
//!   agents, MCP servers and hooks;
//! - MCP servers in `config/mcp_config.json`;
//! - named hooks in `config/hooks.json`;
//! - the global rules file `~/.gemini/GEMINI.md`
//!   (`docs/architecture/antigravity-compatibility.md`, "Context (global)").
//!
//! Sessions are the conversation databases under
//! `antigravity-cli/conversations`, counted by file and dated by the newer
//! of the database and its write-ahead log, which is where a conversation
//! being written lands first. None is opened.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::Value;
use uze_core::inventory::{Claim, Definition, FoundResource, ResourceKind, Runs, SessionCounts};

use super::plugin::GENERATED_PLUGIN_KIND;

/// The places one Antigravity installation keeps what it reads.
pub(super) struct Locations<'a> {
    pub(super) cli_root: &'a Path,
    pub(super) config_dir: &'a Path,
    pub(super) gemini_root: &'a Path,
}

impl Locations<'_> {
    pub(super) fn inventory(&self) -> Vec<FoundResource> {
        let mut found = Vec::new();
        let mut seen = BTreeSet::new();
        for root in [self.cli_root.join("skills"), self.config_dir.join("skills")] {
            for (name, file) in skill_files(&root) {
                if first_sight(&mut seen, &file) {
                    found.push(FoundResource::new(
                        ResourceKind::Skill,
                        name,
                        Definition::Markdown(file),
                    ));
                }
            }
        }
        for (name, file) in skill_files(&self.cli_root.join("builtin").join("skills")) {
            found.push(
                FoundResource::new(ResourceKind::Skill, name, Definition::Markdown(file))
                    .built_in(),
            );
        }
        for root in [self.cli_root.join("agents"), self.config_dir.join("agents")] {
            for file in markdown_files(&root) {
                if first_sight(&mut seen, &file) {
                    found.push(FoundResource::new(
                        ResourceKind::Agent,
                        file_stem(&file),
                        Definition::Markdown(file),
                    ));
                }
            }
        }
        found.extend(mcp_servers(&self.config_dir.join("mcp_config.json")));
        found.extend(named_hooks(&self.config_dir.join("hooks.json")));
        for plugin in plugin_dirs(&self.config_dir.join("plugins")) {
            found.extend(plugin_resources(&plugin));
        }
        let rules = self.gemini_root.join("GEMINI.md");
        if rules.is_file() {
            found.push(FoundResource::new(
                ResourceKind::Rule,
                "GEMINI.md",
                Definition::Whole(rules),
            ));
        }
        found
    }

    pub(super) fn session_counts(&self, now: SystemTime) -> SessionCounts {
        let conversations = self.cli_root.join("conversations");
        let activity = entries(&conversations)
            .into_iter()
            .filter(|path| path.extension().is_some_and(|extension| extension == "db"))
            .filter_map(|database| {
                let mut write_ahead = database.clone().into_os_string();
                write_ahead.push("-wal");
                [modified(&database), modified(Path::new(&write_ahead))]
                    .into_iter()
                    .flatten()
                    .max()
            });
        SessionCounts::from_activity(activity, now)
    }
}

/// What one plugin agy staged brings. Its skills and agents are known to
/// agy by the plugin's namespace (`code:polish`), and the agents uze wrote
/// spell that namespace with `_` (`code_reviewer`), which is read back as
/// the `:` every other harness shows.
fn plugin_resources(plugin: &Path) -> Vec<FoundResource> {
    let name = file_name(plugin);
    let removal = format!("agy plugin uninstall {name}");
    let claim = Claim::Owned {
        kind: GENERATED_PLUGIN_KIND.to_owned(),
        selector: name.clone(),
    };
    let belongs = |resource: FoundResource| {
        resource
            .from_plugin(name.clone())
            .claimed(claim.clone())
            .removed_by(removal.clone())
    };
    let mut found = Vec::new();
    for (skill, file) in skill_files(&plugin.join("skills")) {
        found.push(belongs(FoundResource::new(
            ResourceKind::Skill,
            format!("{name}:{skill}"),
            Definition::Markdown(file),
        )));
    }
    for file in plugin_agent_files(plugin) {
        let stem = file_stem(&file);
        let agent = match stem.strip_prefix(&format!("{name}_")) {
            Some(rest) => format!("{name}:{rest}"),
            None => format!("{name}:{stem}"),
        };
        found.push(belongs(FoundResource::new(
            ResourceKind::Agent,
            agent,
            Definition::Markdown(file),
        )));
    }
    for server in mcp_servers(&plugin.join("mcp_config.json")) {
        found.push(belongs(server));
    }
    for hook in named_hooks(&plugin.join("hooks.json")) {
        found.push(belongs(hook));
    }
    found
}

/// A plugin's agents: the directories its `agents.json` declares, and
/// `agents/` when it declares none.
fn plugin_agent_files(plugin: &Path) -> Vec<PathBuf> {
    let declared: Vec<PathBuf> = read_json(&plugin.join("agents.json"))
        .and_then(|document| document.get("entries")?.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|entry| Some(plugin.join(entry.get("path")?.as_str()?)))
        .collect();
    let directories = if declared.is_empty() {
        vec![plugin.join("agents")]
    } else {
        declared
    };
    directories
        .iter()
        .flat_map(|directory| markdown_files(directory))
        .collect()
}

/// Every server in an `mcp_config.json`, whose entries agy's own
/// `agy mcp add` writes and `agy mcp remove` takes out. A remote server
/// is keyed `url`, or `serverUrl`/`httpUrl` in older files.
fn mcp_servers(config: &Path) -> Vec<FoundResource> {
    let Some(document) = read_json(config) else {
        return Vec::new();
    };
    let Some(servers) = document.get("mcpServers").and_then(Value::as_object) else {
        return Vec::new();
    };
    servers
        .iter()
        .map(|(name, server)| {
            let remote = ["url", "serverUrl", "httpUrl"]
                .iter()
                .find_map(|key| server.get(*key)?.as_str());
            let local = server.get("command").and_then(Value::as_str);
            let mut resource = FoundResource::new(
                ResourceKind::Mcp,
                name.clone(),
                Definition::Entry(config.to_path_buf()),
            )
            .claimed(Claim::Entry(name.clone()))
            .removed_by(format!("agy mcp remove {name}"));
            if let Some(url) = remote {
                resource = resource
                    .running(Runs::Url(url.to_owned()))
                    .noted("remote server");
            } else if let Some(command) = local {
                resource = resource
                    .running(Runs::Program(command.to_owned()))
                    .noted("local server");
            }
            resource
        })
        .collect()
}

/// Every named hook in a `hooks.json`, whose root is the map of names:
/// each value maps an event to its entries, grouped (`{matcher, hooks}`)
/// for a tool event and flat (`{type, command}`) otherwise. The first
/// command a hook runs is the one judged.
fn named_hooks(config: &Path) -> Vec<FoundResource> {
    let Some(Value::Object(hooks)) = read_json(config) else {
        return Vec::new();
    };
    hooks
        .iter()
        .filter_map(|(name, events)| {
            let events = events.as_object()?;
            let event_names: Vec<&str> = events.keys().map(String::as_str).collect();
            let command = events.values().find_map(first_command);
            let mut resource = FoundResource::new(
                ResourceKind::Hook,
                name.clone(),
                Definition::Entry(config.to_path_buf()),
            )
            .claimed(Claim::Entry(name.clone()))
            .noted(format!("on {}", event_names.join(", ")));
            if let Some(command) = command {
                resource = resource.running(Runs::Shell(command));
            }
            Some(resource)
        })
        .collect()
}

fn first_command(entries: &Value) -> Option<String> {
    entries.as_array()?.iter().find_map(|entry| {
        if let Some(command) = entry.get("command").and_then(Value::as_str) {
            return Some(command.to_owned());
        }
        entry
            .get("hooks")?
            .as_array()?
            .iter()
            .find_map(|handler| handler.get("command")?.as_str().map(str::to_owned))
    })
}

/// `<root>/<name>/SKILL.md` for every skill directory under `root`.
fn skill_files(root: &Path) -> Vec<(String, PathBuf)> {
    entries(root)
        .into_iter()
        .filter_map(|directory| {
            let file = directory.join("SKILL.md");
            file.is_file().then(|| (file_name(&directory), file))
        })
        .collect()
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    entries(root)
        .into_iter()
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|extension| extension == "md")
        })
        .collect()
}

fn plugin_dirs(root: &Path) -> Vec<PathBuf> {
    entries(root)
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

/// The entries of `directory`, sorted so the Overview reads the same on
/// every collection; none when it cannot be listed.
fn entries(directory: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

/// Whether `file` has not been seen yet under another path: agy links one
/// skills root to the other, so the same file is reachable twice.
fn first_sight(seen: &mut BTreeSet<PathBuf>, file: &Path) -> bool {
    let identity = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    seen.insert(identity)
}

fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
