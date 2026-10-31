//! Which skills, agents and MCP servers Antigravity CLI used, read from the
//! transcript agy keeps of every conversation.
//!
//! A conversation's record is `brain/<id>/.system_generated/logs/
//! transcript.jsonl`, the log agy itself names for it (a subagent's
//! `logAbsoluteUri`); a subagent has one of its own. Its sibling
//! `transcript_full.jsonl` holds the same steps with fuller content, so it
//! is never read as well. Measured in the Lab
//! (`conformance/experiments/antigravity/{usage-trace,user-invocation}.py`,
//! 1.4.12):
//! - a skill the person typed as a leading `/<name>` reaches no tool: agy
//!   adds `<SKILL>The user has explicitly invoked the (<name>) skill` to the
//!   `USER_INPUT` step's `<ADDITIONAL_METADATA>`, which is the only part of
//!   that step read here;
//! - a skill the model uses is a read of its `SKILL.md` by absolute path:
//!   `read_file` (`Path`), a subagent's `view_file` (`AbsolutePath`), or a
//!   `run_command` that names it. One whose result step is `ERROR` (a `~`
//!   or relative path) did not happen;
//! - an agent is `invoke_subagent` (`TypeName`), an MCP tool `call_mcp_tool`
//!   (`ServerName`, a plugin's spelled `<plugin>_<server>`).
//!
//! `@<agent>` typed in a prompt is plain text to agy, and `--agent` is
//! recorded nowhere, so neither is a use here.

use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use uze_core::{
    inventory::ResourceKind,
    usage::{Invoker, Resume, UsageRead, UsageRecord, Use},
};

use crate::shared::transcript;

const TRANSCRIPT: &str = "transcript.jsonl";
/// Every step carries it: a call's result is matched to the call by order,
/// so no step can be skipped.
const STEP: &str = "\"step_index\"";
const INVOKED: &str = "The user has explicitly invoked the (";

/// The places a use names a skill or a plugin by.
pub(super) struct Usage<'a> {
    pub(super) cli_root: &'a Path,
    pub(super) config_dir: &'a Path,
}

impl Usage<'_> {
    /// Every conversation's transcript changed since `since`, subagents'
    /// included, from metadata alone.
    pub(super) fn records(&self, since: SystemTime) -> Vec<UsageRecord> {
        transcript::jsonl_records(&self.cli_root.join("brain"), 3, since)
            .into_iter()
            .filter(|record| record.id.ends_with(&format!("/{TRANSCRIPT}")))
            .collect()
    }

    pub(super) fn read(&self, record: &UsageRecord, from: Resume) -> Option<UsageRead> {
        let (steps, offset) = transcript::records_from(Path::new(&record.id), from, &[STEP])?;
        let names = Names::of(self);
        let mut uses = Vec::new();
        let mut pending: VecDeque<Use> = VecDeque::new();
        for step in &steps {
            let at = created_at(step);
            match step.get("type").and_then(Value::as_str) {
                Some("USER_INPUT") => {
                    uses.extend(
                        typed_skills(step)
                            .map(|name| Use::named(ResourceKind::Skill, name, at, Invoker::Person)),
                    );
                }
                Some("PLANNER_RESPONSE") => {
                    // A call whose result never came is taken as made.
                    uses.extend(pending.drain(..));
                    for call in step
                        .get("tool_calls")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        pending.extend(names.use_of(call, at));
                    }
                }
                _ if step.get("source").and_then(Value::as_str) == Some("MODEL") => {
                    if let Some(made) = pending.pop_front()
                        && step.get("status").and_then(Value::as_str) != Some("ERROR")
                    {
                        uses.push(made);
                    }
                }
                _ => {}
            }
        }
        uses.extend(pending);
        Some(UsageRead {
            uses,
            resume_at: Some(offset),
        })
    }
}

/// How this installation names what a call reaches, as its inventory does.
struct Names {
    skill_roots: Vec<PathBuf>,
    plugins_dir: PathBuf,
    plugins: BTreeSet<String>,
}

impl Names {
    fn of(usage: &Usage<'_>) -> Self {
        let plugins_dir = usage.config_dir.join("plugins");
        let plugins = fs::read_dir(&plugins_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        Self {
            skill_roots: vec![
                usage.cli_root.join("skills"),
                usage.config_dir.join("skills"),
                usage.cli_root.join("builtin").join("skills"),
            ],
            plugins_dir,
            plugins,
        }
    }

    /// The use one tool call is, if it is one.
    fn use_of(&self, call: &Value, at: SystemTime) -> Option<Use> {
        let name = call.get("name")?.as_str()?;
        let args = call.get("args")?;
        let found = match name {
            "read_file" => self.skill_read(&argument(args, "Path")?)?,
            "view_file" => self.skill_read(&argument(args, "AbsolutePath")?)?,
            "run_command" => argument(args, "CommandLine")?
                .split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .find_map(|word| self.skill_read(word))?,
            "invoke_subagent" => (
                ResourceKind::Agent,
                self.agent(&argument(args, "TypeName")?),
            ),
            "call_mcp_tool" => (
                ResourceKind::Mcp,
                self.server(&argument(args, "ServerName")?),
            ),
            _ => return None,
        };
        Some(Use::named(found.0, found.1, at, Invoker::Model))
    }

    /// A read of a skill's `SKILL.md` by absolute path, named as the
    /// inventory names that skill.
    fn skill_read(&self, path: &str) -> Option<(ResourceKind, String)> {
        let path = Path::new(path);
        if !path.is_absolute() || path.file_name()? != "SKILL.md" {
            return None;
        }
        let directory = path.parent()?;
        let skill = directory.file_name()?.to_string_lossy().into_owned();
        let root = directory.parent()?;
        if self.skill_roots.iter().any(|known| known == root) {
            return Some((ResourceKind::Skill, skill));
        }
        let plugin = root.parent()?;
        (root.file_name()? == "skills" && plugin.parent()? == self.plugins_dir).then(|| {
            let plugin = plugin.file_name()?.to_string_lossy().into_owned();
            Some((ResourceKind::Skill, format!("{plugin}:{skill}")))
        })?
    }

    /// A plugin's agent is spelled `<plugin>_<agent>` in the file uze writes
    /// and listed `<plugin>:<agent>`.
    fn agent(&self, type_name: &str) -> String {
        match self.plugin_prefixed(type_name) {
            Some((plugin, rest)) if !type_name.contains(':') => format!("{plugin}:{rest}"),
            _ => type_name.to_owned(),
        }
    }

    /// A plugin's server is called `<plugin>_<server>` and listed by its own
    /// name.
    fn server(&self, server: &str) -> String {
        self.plugin_prefixed(server)
            .map_or_else(|| server.to_owned(), |(_, rest)| rest.to_owned())
    }

    /// The longest staged plugin's name `name` starts with, and what follows.
    fn plugin_prefixed<'n>(&self, name: &'n str) -> Option<(&str, &'n str)> {
        self.plugins
            .iter()
            .filter_map(|plugin| {
                let rest = name.strip_prefix(plugin.as_str())?.strip_prefix('_')?;
                (!rest.is_empty()).then_some((plugin.as_str(), rest))
            })
            .max_by_key(|(plugin, _)| plugin.len())
    }
}

/// The skills agy says the person invoked, from the part of the step agy
/// writes: what the person typed is never read.
fn typed_skills(step: &Value) -> impl Iterator<Item = String> + '_ {
    let metadata = step
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| content.split_once("<ADDITIONAL_METADATA>"))
        .map_or("", |(_, metadata)| metadata);
    metadata.split(INVOKED).skip(1).filter_map(|rest| {
        let (name, _) = rest.split_once(") skill")?;
        (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| name.to_owned())
    })
}

/// An argument as agy wrote it: plain in one transcript, encoded as a JSON
/// string in the other.
fn argument(args: &Value, key: &str) -> Option<String> {
    let value = args.get(key)?.as_str()?;
    Some(
        serde_json::from_str::<String>(value)
            .ok()
            .unwrap_or_else(|| value.to_owned()),
    )
}

fn created_at(step: &Value) -> SystemTime {
    step.get("created_at")
        .and_then(Value::as_str)
        .and_then(transcript::unix_seconds)
        .map_or(UNIX_EPOCH, |seconds| {
            UNIX_EPOCH + Duration::from_secs(seconds)
        })
}

#[cfg(test)]
mod tests;
