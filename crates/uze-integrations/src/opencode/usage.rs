//! Which skills, agents and MCP servers OpenCode's sessions used, for the
//! machine Overview.
//!
//! Asked of the harness, never of its database: the sessions are listed by
//! `opencode api session.list`, the one listing that is the machine's
//! (`session list` is the current project's), and each one that changed is
//! read back with `opencode session export`, the record the harness
//! publishes. Each answer took about 0.1 s in the Lab. An export is
//! one JSON document, rewritten as the session grows, so it is read whole
//! each time its `updated` time moves.
//!
//! What a use looks like in an export (measured on OpenCode 2.2.5,
//! `conformance/experiments/opencode/{usage-trace,user-invocation,session-listing}.py`):
//! - a skill or an agent the person named with `@` is in its `user`
//!   message's `skills[].id` or `agents[].name`;
//! - a skill the model chose is a `tool` part `skill` with `state.input.id`,
//!   an agent a `tool` part `subagent` with `state.input.agent`;
//! - an MCP tool called from Code Mode's `execute` is in that part's
//!   `state.metadata.toolCalls[].tool`, as `<server>.<tool>`; the server is
//!   named with `@` as `_`.

use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use uze_core::{
    inventory::ResourceKind,
    usage::{Invoker, Stamp, UsageRead, UsageRecord, Use},
};

use crate::shared::process;

/// Long enough for the harness's cold start, short enough that the
/// Overview never waits long on a harness that hangs.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The listing is bounded by its `limit`; this is well past the sessions a
/// machine makes in the longest period.
const LISTED: &str = "limit=100000";

/// The sessions updated since `since`, from the harness's own listing.
pub(super) fn records(
    executable: &Path,
    home: &Path,
    since: SystemTime,
) -> Option<Vec<UsageRecord>> {
    let listing = ask(
        executable,
        home,
        &["api", "session.list", "--param", LISTED],
    )?;
    sessions_in(&listing, since)
}

/// The uses in the session `id`, whose MCP servers are named in the
/// configuration by `servers`.
pub(super) fn read(
    executable: &Path,
    home: &Path,
    id: &str,
    servers: &[String],
) -> Option<UsageRead> {
    let export = ask(executable, home, &["session", "export", id])?;
    Some(UsageRead {
        uses: uses_in(&export, servers)?,
        resume_at: None,
    })
}

fn ask(executable: &Path, home: &Path, args: &[&str]) -> Option<Value> {
    let outside_every_project = std::env::temp_dir();
    let output =
        process::capture_read_only_from(executable, home, &outside_every_project, args, TIMEOUT)
            .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

/// The sessions of a `session list --format json` answer updated since
/// `since`. A session is a record of its own, subagents' included.
fn sessions_in(listing: &Value, since: SystemTime) -> Option<Vec<UsageRecord>> {
    let sessions = listing
        .as_array()
        .or_else(|| listing.get("data")?.as_array())?;
    Some(
        sessions
            .iter()
            .filter_map(|session| {
                let id = session.get("id")?.as_str()?;
                let updated = millis(
                    session
                        .pointer("/time/updated")
                        .or_else(|| session.get("updated"))?,
                )?;
                (updated >= since).then(|| UsageRecord {
                    id: id.to_owned(),
                    stamp: Stamp::of(0, updated),
                })
            })
            .collect(),
    )
}

fn millis(value: &Value) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_millis(value.as_u64()?))
}

/// Every use an export records. A skill or agent the person named counts
/// once even when the model then calls it on the same turn; every other
/// call the model makes is a use of its own. Only a call that completed
/// counts.
fn uses_in(export: &Value, servers: &[String]) -> Option<Vec<Use>> {
    let messages = export.get("messages")?.as_array()?;
    let mut uses = Vec::new();
    let mut named: Vec<(ResourceKind, String)> = Vec::new();
    for message in messages {
        let at = message
            .pointer("/time/created")
            .and_then(millis)
            .unwrap_or(UNIX_EPOCH);
        match message.get("type").and_then(Value::as_str) {
            Some("user") => {
                named.clear();
                for (list, field, kind) in [
                    ("skills", "id", ResourceKind::Skill),
                    ("agents", "name", ResourceKind::Agent),
                ] {
                    for item in message
                        .get(list)
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(name) = item.get(field).and_then(Value::as_str) {
                            uses.push(Use::named(kind, name, at, Invoker::Person));
                            named.push((kind, name.to_owned()));
                        }
                    }
                }
            }
            Some("assistant") => {
                let parts = message.get("content").and_then(Value::as_array);
                for part in parts.into_iter().flatten() {
                    for (kind, name) in called(part, servers) {
                        match named.iter().position(|seen| *seen == (kind, name.clone())) {
                            Some(index) => {
                                named.remove(index);
                            }
                            None => uses.push(Use::named(kind, name, at, Invoker::Model)),
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Some(uses)
}

/// The resources one assistant part used, when it is a completed tool call.
fn called(part: &Value, servers: &[String]) -> Vec<(ResourceKind, String)> {
    if part.get("type").and_then(Value::as_str) != Some("tool")
        || part.pointer("/state/status").and_then(Value::as_str) != Some("completed")
    {
        return Vec::new();
    }
    let input = |field: &str| {
        part.pointer(&format!("/state/input/{field}"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    match part.get("name").and_then(Value::as_str) {
        Some("skill") => input("id")
            .map(|name| vec![(ResourceKind::Skill, name)])
            .unwrap_or_default(),
        Some("subagent") => input("agent")
            .map(|name| vec![(ResourceKind::Agent, name)])
            .unwrap_or_default(),
        Some("execute") => part
            .pointer("/state/metadata/toolCalls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|call| call.get("status").and_then(Value::as_str) == Some("completed"))
            .filter_map(|call| server_of(call.get("tool")?.as_str()?, '.', servers))
            .map(|server| (ResourceKind::Mcp, server))
            .collect(),
        Some(tool) => server_of(tool, '_', servers)
            .map(|server| vec![(ResourceKind::Mcp, server)])
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

/// The configured server a tool named `<server><separator><tool>` belongs
/// to: both may hold `_`, so the longest configured name that prefixes it
/// wins. The harness spells a server's `@` as `_`.
fn server_of(tool: &str, separator: char, servers: &[String]) -> Option<String> {
    servers
        .iter()
        .filter(|server| {
            let spelled = server.replace('@', "_");
            tool.strip_prefix(spelled.as_str())
                .is_some_and(|rest| rest.starts_with(separator) && rest.len() > 1)
        })
        .max_by_key(|server| server.len())
        .cloned()
}

#[cfg(test)]
mod tests;
