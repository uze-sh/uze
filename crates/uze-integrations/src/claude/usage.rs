//! Which skills, agents and MCP servers Claude Code's own transcripts say
//! were used, measured in the Lab (`claude/usage-trace`,
//! `claude/user-invocation`, 2.1.316).
//!
//! - The person: a `user` entry holding `<command-name>/<name></command-name>`
//!   (a skill, a plugin's `plugin:skill`, an MCP prompt
//!   `mcp__<server>__<prompt>`), and an `agent_mention` attachment.
//! - The model: an `assistant` entry's `tool_use` block, `Skill`
//!   (`input.skill`), `Agent` (`input.subagent_type`) or
//!   `mcp__<server>__<tool>` (a plugin's `mcp__plugin_<plugin>_<server>__`).
//!
//! Every other entry is passed over, including the ones that repeat earlier
//! blocks (`api-request-blob`) or list tools without calling them
//! (`deferred_tools_record`). A mention and the dispatch it led to in the
//! same turn are one use, the person's. Only names are taken: what the
//! person or the model wrote around them never leaves this module.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use uze_core::{
    inventory::ResourceKind,
    usage::{Invoker, Resume, UsageRead, UsageRecord, Use},
};

use crate::shared::transcript::{jsonl_records, records_from, unix_seconds};

/// `projects/<dir>/<session>.jsonl` and
/// `projects/<dir>/<session>/subagents/<agent>.jsonl`.
const DEPTH: usize = 3;

/// How long after a mention the dispatch it asked for is still that
/// mention's, not a use of its own. A plain prompt holds no marker and is
/// never parsed, so the turn's end cannot be seen; a mention the model never
/// acted on must not swallow a dispatch an hour later.
const MENTION_ANSWERED_WITHIN: Duration = Duration::from_secs(30 * 60);

/// A line is parsed only if it holds one of these.
const MARKERS: [&str; 5] = [
    "<command-name>",
    "\"agent_mention\"",
    "\"name\":\"Skill\"",
    "\"name\":\"Agent\"",
    "\"name\":\"mcp__",
];

pub(super) fn records(projects: &Path, since: SystemTime) -> Vec<UsageRecord> {
    jsonl_records(projects, DEPTH, since)
}

pub(super) fn read(record: &UsageRecord, from: Resume) -> Option<UsageRead> {
    let (entries, offset) = records_from(Path::new(&record.id), from, &MARKERS)?;
    Some(UsageRead {
        uses: uses_in(entries),
        resume_at: Some(offset),
    })
}

fn uses_in(entries: Vec<Value>) -> Vec<Use> {
    let mut uses = Vec::new();
    let mut mentioned: BTreeMap<String, SystemTime> = BTreeMap::new();
    let mut calls: BTreeSet<String> = BTreeSet::new();
    for entry in entries {
        let Some(at) = entry
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(unix_seconds)
            .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds))
        else {
            continue;
        };
        match entry.get("type").and_then(Value::as_str) {
            Some("user") => {
                let Some(text) = typed_text(&entry) else {
                    continue;
                };
                for name in commands(text) {
                    uses.push(match mcp_server(name) {
                        Some(server) => Use::named(ResourceKind::Mcp, server, at, Invoker::Person),
                        None => Use::named(ResourceKind::Skill, name, at, Invoker::Person),
                    });
                }
            }
            Some("attachment") => {
                let attachment = entry.get("attachment");
                if attachment
                    .and_then(|a| a.get("type"))
                    .and_then(Value::as_str)
                    != Some("agent_mention")
                {
                    continue;
                }
                if let Some(agent) = attachment
                    .and_then(|a| a.get("agentType"))
                    .and_then(Value::as_str)
                {
                    mentioned.insert(agent.to_owned(), at);
                    uses.push(Use::named(ResourceKind::Agent, agent, at, Invoker::Person));
                }
            }
            Some("assistant") => {
                for block in tool_uses(&entry) {
                    let id = block.get("id").and_then(Value::as_str).unwrap_or_default();
                    if !id.is_empty() && !calls.insert(id.to_owned()) {
                        continue;
                    }
                    let input = block.get("input");
                    let field = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
                    match block.get("name").and_then(Value::as_str) {
                        Some("Skill") => {
                            if let Some(skill) = field("skill") {
                                uses.push(Use::named(
                                    ResourceKind::Skill,
                                    skill,
                                    at,
                                    Invoker::Model,
                                ));
                            }
                        }
                        Some("Agent") => {
                            if let Some(agent) = field("subagent_type")
                                && !answers_mention(&mut mentioned, agent, at)
                            {
                                uses.push(Use::named(
                                    ResourceKind::Agent,
                                    agent,
                                    at,
                                    Invoker::Model,
                                ));
                            }
                        }
                        Some(tool) => {
                            if let Some(server) = mcp_server(tool) {
                                uses.push(Use::named(
                                    ResourceKind::Mcp,
                                    server,
                                    at,
                                    Invoker::Model,
                                ));
                            }
                        }
                        None => {}
                    }
                }
            }
            _ => {}
        }
    }
    uses
}

/// Whether dispatching `agent` at `at` is what an earlier mention asked
/// for, which it then consumes.
fn answers_mention(
    mentioned: &mut BTreeMap<String, SystemTime>,
    agent: &str,
    at: SystemTime,
) -> bool {
    let Some(when) = mentioned.remove(agent) else {
        return false;
    };
    at.duration_since(when)
        .is_ok_and(|after| after <= MENTION_ANSWERED_WITHIN)
}

/// What the person typed, when the entry is a prompt rather than an
/// expanded body (`isMeta`) or a tool's result.
fn typed_text(entry: &Value) -> Option<&str> {
    if entry.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    match entry.get("message")?.get("content")? {
        Value::String(text) => Some(text),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .find_map(|block| block.get("text").and_then(Value::as_str))
            .filter(|text| text.contains("<command-name>")),
        _ => None,
    }
}

/// Every `<command-name>/<name></command-name>` in `text`, as `<name>`.
fn commands(text: &str) -> impl Iterator<Item = &str> {
    text.split("<command-name>").skip(1).filter_map(|rest| {
        let name = rest.split_once("</command-name>")?.0.trim();
        let name = name.strip_prefix('/').unwrap_or(name);
        (!name.is_empty() && !name.contains(char::is_whitespace)).then_some(name)
    })
}

fn tool_uses(entry: &Value) -> impl Iterator<Item = &Value> {
    entry
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
}

/// The server of `mcp__<server>__<tool>`, a plugin's
/// `mcp__plugin_<plugin>_<server>__<tool>` named by its own `<server>`, as
/// the inventory lists it. A plugin's name never holds `_`.
fn mcp_server(tool: &str) -> Option<&str> {
    let middle = tool.strip_prefix("mcp__")?.rsplit_once("__")?.0;
    let server = match middle.strip_prefix("plugin_") {
        Some(rest) => rest.split_once('_')?.1,
        None => middle,
    };
    (!server.is_empty()).then_some(server)
}

#[cfg(test)]
mod tests;
