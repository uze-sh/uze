//! Which skills, agents and MCP servers a Codex rollout shows were used,
//! for the machine Overview's uses.
//!
//! A rollout (`sessions/YYYY/MM/DD/rollout-*.jsonl`) is appended to while
//! its session runs; a subagent's thread has a rollout of its own and is
//! read like any other. What is taken, measured on 0.166
//! (`conformance/experiments/codex/{usage-trace,user-invocation}.py`):
//! - a `$name` the person typed is injected as a user message opening with
//!   `<skill><name>…</name><path>…</path>`: a use by the person, of the
//!   skill at that path;
//! - a skill the model chose is the read of its `SKILL.md`, which Codex
//!   records as a `CommandExecution` item with the parsed `read` and the
//!   exit code. Only a read that succeeded counts. The catalog offers
//!   aliased paths (`r0/<dir>/SKILL.md`, with a roots table in the
//!   instructions), but Codex does not expand them: the model must, and a
//!   read of the alias as written fails in the shell. So a relative path is
//!   resolved against the command's directory, never the alias table;
//! - an agent is a `spawn_agent` call with its `agent_type`;
//! - an MCP tool called from code mode is recorded only inside the `exec`
//!   script that called it, when the script spells `mcp__<server>__<tool>`:
//!   an inferred use. Codex writes a server's name with every character
//!   but letters, digits and `_` as `_`, so it is read back against the
//!   keys `config.toml` declares.
//!
//! One use counts once per turn: a mention and the read it led to are one,
//! and so are two reads of one skill. Nothing else of a record is kept.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use uze_core::{
    inventory::ResourceKind,
    usage::{Invoker, Resume, Target, UsageRead, UsageRecord, Use},
};

use crate::shared::transcript;

/// The rollouts of sessions active since `since`, subagents' included.
pub(super) fn records(sessions: &Path, since: SystemTime) -> Vec<UsageRecord> {
    transcript::jsonl_records(sessions, 4, since)
        .into_iter()
        .filter(|record| {
            Path::new(&record.id)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-"))
        })
        .collect()
}

const MARKERS: [&str; 4] = ["<skill>", "CommandExecution", "spawn_agent", "mcp__"];

pub(super) fn read(record: &UsageRecord, from: Resume, config: &Path) -> Option<UsageRead> {
    let (lines, offset) = transcript::records_from(Path::new(&record.id), from, &MARKERS)?;
    let mut reader = Reader {
        config,
        servers: None,
        seen: BTreeSet::new(),
        uses: Vec::new(),
    };
    for line in &lines {
        reader.line(line);
    }
    Some(UsageRead {
        uses: reader.uses,
        resume_at: Some(offset),
    })
}

struct Reader<'a> {
    config: &'a Path,
    /// The MCP servers `config.toml` declares, read once the first script
    /// naming one is met.
    servers: Option<Vec<String>>,
    seen: BTreeSet<(String, ResourceKind, Target)>,
    uses: Vec<Use>,
}

impl Reader<'_> {
    fn line(&mut self, line: &Value) {
        let Some(payload) = line.get("payload") else {
            return;
        };
        let Some(at) = line
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(transcript::unix_seconds)
            .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds))
        else {
            return;
        };
        let turn = turn_of(payload);
        match (
            line.get("type").and_then(Value::as_str),
            payload.get("type").and_then(Value::as_str),
        ) {
            (Some("response_item"), Some("message"))
                if payload.get("role").and_then(Value::as_str) == Some("user") =>
            {
                for (name, path) in mentions(payload) {
                    let target = match path {
                        Some(path) => Target::Definition(path),
                        None => Target::Name(name),
                    };
                    self.count(
                        &turn,
                        ResourceKind::Skill,
                        target,
                        at,
                        Invoker::Person,
                        true,
                    );
                }
            }
            (Some("event_msg"), Some("item_completed")) => {
                if let Some(item) = payload.get("item") {
                    for path in skill_reads(item) {
                        let target = Target::Definition(path);
                        self.count(&turn, ResourceKind::Skill, target, at, Invoker::Model, true);
                    }
                }
            }
            (Some("response_item"), Some("function_call"))
                if payload.get("name").and_then(Value::as_str) == Some("spawn_agent") =>
            {
                if let Some(agent) = spawned(payload) {
                    let target = Target::Name(agent);
                    self.count(&turn, ResourceKind::Agent, target, at, Invoker::Model, true);
                }
            }
            (Some("response_item"), Some("custom_tool_call")) => {
                let script = payload.get("input").and_then(Value::as_str).unwrap_or("");
                if script.contains("mcp__") {
                    for server in self.servers_in(script) {
                        let target = Target::Name(server);
                        self.count(&turn, ResourceKind::Mcp, target, at, Invoker::Model, false);
                    }
                }
            }
            _ => {}
        }
    }

    fn count(
        &mut self,
        turn: &Option<String>,
        kind: ResourceKind,
        target: Target,
        at: SystemTime,
        by: Invoker,
        exact: bool,
    ) {
        // An MCP server called twice in a turn is two uses; a skill loaded
        // twice, or mentioned and then read, is one.
        if kind != ResourceKind::Mcp
            && let Some(turn) = turn
            && !self.seen.insert((turn.clone(), kind, target.clone()))
        {
            return;
        }
        let found = Use {
            kind,
            target,
            at,
            by,
            exact: true,
        };
        self.uses.push(if exact { found } else { found.inferred() });
    }

    fn servers_in(&mut self, script: &str) -> Vec<String> {
        let servers = self
            .servers
            .get_or_insert_with(|| configured_servers(self.config));
        servers
            .iter()
            .filter(|server| script.contains(&format!("mcp__{}__", tool_name(server))))
            .cloned()
            .collect()
    }
}

fn turn_of(payload: &Value) -> Option<String> {
    payload
        .get("turn_id")
        .or_else(|| payload.pointer("/internal_chat_message_metadata_passthrough/turn_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The skills an injected user message opens with, by name and path.
fn mentions(message: &Value) -> Vec<(String, Option<PathBuf>)> {
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return Vec::new();
    };
    content
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .filter(|text| text.trim_start().starts_with("<skill>"))
        .filter_map(|text| {
            let name = between(text, "<name>", "</name>")?.trim().to_owned();
            let path = between(text, "<path>", "</path>")
                .map(str::trim)
                .filter(|path| path.starts_with('/'))
                .map(PathBuf::from);
            (!name.is_empty()).then_some((name, path))
        })
        .collect()
}

fn between<'t>(text: &'t str, open: &str, close: &str) -> Option<&'t str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(&text[start..end])
}

/// The `SKILL.md` files a command that succeeded read, as absolute paths.
fn skill_reads(item: &Value) -> Vec<PathBuf> {
    if item.get("type").and_then(Value::as_str) != Some("CommandExecution")
        || item.get("exit_code").and_then(Value::as_i64) != Some(0)
    {
        return Vec::new();
    }
    let cwd = item.get("cwd").and_then(Value::as_str).map(Path::new);
    let Some(parsed) = item.get("parsed_cmd").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for command in parsed {
        let named = match command.get("type").and_then(Value::as_str) {
            Some("read") => command
                .get("path")
                .and_then(Value::as_str)
                .map(|path| vec![path.to_owned()])
                .unwrap_or_default(),
            _ => command
                .get("cmd")
                .and_then(Value::as_str)
                .map(words_naming_a_skill_file)
                .unwrap_or_default(),
        };
        for path in named.into_iter().filter(|path| is_skill_file(path)) {
            let path = PathBuf::from(path);
            let path = match (path.is_absolute(), cwd) {
                (true, _) => path,
                (false, Some(cwd)) => cwd.join(path),
                (false, None) => continue,
            };
            if !found.contains(&path) {
                found.push(path);
            }
        }
    }
    found
}

fn words_naming_a_skill_file(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|word| word.trim_matches(|c| c == '"' || c == '\''))
        .filter(|word| is_skill_file(word))
        .map(str::to_owned)
        .collect()
}

/// By its file name, in the platform's own separators: a shell on Windows
/// writes `C:\Users\me\.agents\skills\x\SKILL.md`.
fn is_skill_file(path: &str) -> bool {
    Path::new(path).file_name() == Some("SKILL.md".as_ref())
}

fn spawned(call: &Value) -> Option<String> {
    let arguments: Value = serde_json::from_str(call.get("arguments")?.as_str()?).ok()?;
    let agent = arguments.get("agent_type")?.as_str()?.trim();
    (!agent.is_empty()).then(|| agent.to_owned())
}

/// The keys of `[mcp_servers.*]`, the names the inventory lists them by.
fn configured_servers(config: &Path) -> Vec<String> {
    let Some(document) = fs::read_to_string(config)
        .ok()
        .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
    else {
        return Vec::new();
    };
    document
        .get("mcp_servers")
        .and_then(|item| item.as_table_like())
        .map(|table| table.iter().map(|(key, _)| key.to_owned()).collect())
        .unwrap_or_default()
}

/// A server's name as Codex spells it in a tool's name.
fn tool_name(server: &str) -> String {
    server
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
