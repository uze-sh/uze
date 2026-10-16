//! What a Claude Code transcript says a conversation was about, for the
//! briefing of an agent that continues on another harness.
//!
//! The transcript is one record per line. What is read, and why only that:
//! - a `user` record whose content is a string is the operator's turn; one
//!   whose content is an array carries tool results, which are output and
//!   not something anybody said;
//! - an `assistant` record's `text` blocks are what the agent said; its
//!   `thinking` blocks are reasoning and are left out, and a `Bash`
//!   `tool_use` is a command it ran;
//! - a `user` record marked `isCompactSummary` is Claude's own summary of
//!   everything before it.
//!
//! Records marked `isSidechain` (a subagent's) or `isMeta` (Claude's own
//! bookkeeping) are skipped, as is an `assistant` record Claude writes for
//! an API error (`isApiErrorMessage`), which nobody said, and anything else
//! it writes.

use std::path::Path;

use serde_json::Value;
use uze_core::session::{ConversationDigest, LastCommand, Speaker, Turn};

use crate::shared::transcript;

/// Text Claude records as an operator turn but nobody typed as one: the
/// echo of a slash command and of its output.
const COMMAND_ECHOES: [&str; 3] = ["<command-", "<local-command-", "<bash-"];

pub(super) fn read(transcript: &Path, since_unix: Option<u64>) -> Option<ConversationDigest> {
    let mut digest = ConversationDigest::default();
    let mut pending_command: Option<String> = None;
    for record in transcript::records(transcript)? {
        if flag(&record, "isSidechain")
            || flag(&record, "isMeta")
            || flag(&record, "isApiErrorMessage")
        {
            continue;
        }
        let at = record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(transcript::unix_seconds);
        let recent = since_unix.is_none_or(|since| at.is_some_and(|at| at >= since));
        match record.get("type").and_then(Value::as_str) {
            Some("user") => {
                let content = record.pointer("/message/content");
                if let Some(text) = content.and_then(Value::as_str) {
                    if flag(&record, "isCompactSummary") {
                        if recent {
                            digest.summary = Some(text.to_owned());
                            digest.turns.clear();
                        }
                        continue;
                    }
                    if COMMAND_ECHOES.iter().any(|echo| text.starts_with(echo)) {
                        continue;
                    }
                    if digest.goal.is_none() {
                        digest.goal = Some(text.to_owned());
                    }
                    if recent {
                        digest.turns.push(turn(Speaker::Operator, text, at));
                    }
                } else if let Some(blocks) = content.and_then(Value::as_array) {
                    for result in blocks
                        .iter()
                        .filter(|block| kind(block) == Some("tool_result"))
                    {
                        let answers = result.get("tool_use_id").and_then(Value::as_str);
                        if answers.is_some()
                            && answers == pending_command.as_deref()
                            && let Some(command) = digest.last_command.as_mut()
                        {
                            command.failed = result.get("is_error").and_then(Value::as_bool);
                        }
                    }
                }
            }
            Some("assistant") => {
                let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array)
                else {
                    continue;
                };
                let said: Vec<&str> = blocks
                    .iter()
                    .filter(|block| kind(block) == Some("text"))
                    .filter_map(|block| block.get("text").and_then(Value::as_str))
                    .collect();
                if recent && !said.is_empty() {
                    digest
                        .turns
                        .push(turn(Speaker::Agent, &said.join("\n\n"), at));
                }
                for call in blocks.iter().filter(|block| {
                    kind(block) == Some("tool_use")
                        && block.get("name").and_then(Value::as_str) == Some("Bash")
                }) {
                    if let Some(line) = call.pointer("/input/command").and_then(Value::as_str) {
                        digest.last_command = Some(LastCommand {
                            line: line.to_owned(),
                            failed: None,
                        });
                        pending_command = call.get("id").and_then(Value::as_str).map(str::to_owned);
                    }
                }
            }
            _ => {}
        }
    }
    Some(digest)
}

fn turn(speaker: Speaker, text: &str, at_unix: Option<u64>) -> Turn {
    Turn {
        speaker,
        text: text.to_owned(),
        at_unix,
    }
}

fn flag(record: &Value, name: &str) -> bool {
    record.get(name).and_then(Value::as_bool) == Some(true)
}

fn kind(block: &Value) -> Option<&str> {
    block.get("type").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn transcript(label: &str, records: &[Value]) -> std::path::PathBuf {
        let path = uze_testkit::temp::scratch(label).join("session.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: Vec<String> = records.iter().map(Value::to_string).collect();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    fn operator(text: &str, at: &str) -> Value {
        json!({"type":"user","timestamp":at,"message":{"role":"user","content":text}})
    }

    fn agent(blocks: Value, at: &str) -> Value {
        json!({"type":"assistant","timestamp":at,"message":{"role":"assistant","content":blocks}})
    }

    /// The shapes Claude Code 2.1 writes, scrubbed to what the reader keys on.
    fn conversation() -> Vec<Value> {
        vec![
            json!({"type":"permission-mode","permissionMode":"default"}),
            operator("make branch names short", "2026-10-06T00:00:00Z"),
            agent(
                json!([{"type":"thinking","thinking":"private"}]),
                "2026-10-06T00:00:01Z",
            ),
            agent(
                json!([{"type":"text","text":"Looking at the namer."},
                       {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo test -p uze-workspace","description":"run"}}]),
                "2026-10-06T00:00:02Z",
            ),
            json!({"type":"user","timestamp":"2026-10-06T00:00:03Z","message":{"role":"user","content":[
                {"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"1 failed"}]}}),
            json!({"type":"user","isSidechain":true,"timestamp":"2026-10-06T00:00:04Z","message":{"role":"user","content":"a subagent's prompt"}}),
            operator(
                "<command-name>/clear</command-name>",
                "2026-10-06T00:00:05Z",
            ),
            operator("also cover the root case", "2026-10-06T00:00:06Z"),
            agent(
                json!([{"type":"text","text":"The root case now names the label."}]),
                "2026-10-06T00:00:07Z",
            ),
        ]
    }

    #[test]
    fn a_conversation_reads_as_its_goal_its_turns_and_its_last_command() {
        let digest = read(&transcript("claude-digest", &conversation()), None).unwrap();

        assert_eq!(digest.goal.as_deref(), Some("make branch names short"));
        let said: Vec<(Speaker, &str)> = digest
            .turns
            .iter()
            .map(|turn| (turn.speaker, turn.text.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                (Speaker::Operator, "make branch names short"),
                (Speaker::Agent, "Looking at the namer."),
                (Speaker::Operator, "also cover the root case"),
                (Speaker::Agent, "The root case now names the label."),
            ],
            "no reasoning, tool output, subagent or command echo"
        );
        assert_eq!(
            digest.last_command,
            Some(LastCommand {
                line: "cargo test -p uze-workspace".into(),
                failed: Some(true),
            })
        );
    }

    #[test]
    fn a_compaction_summary_replaces_what_came_before_it() {
        let mut records = conversation();
        records.insert(
            6,
            json!({"type":"user","isCompactSummary":true,"timestamp":"2026-10-06T00:00:05Z",
                   "message":{"role":"user","content":"Earlier: the namer was rewritten."}}),
        );
        let digest = read(&transcript("claude-digest-compact", &records), None).unwrap();

        assert_eq!(
            digest.summary.as_deref(),
            Some("Earlier: the namer was rewritten.")
        );
        assert_eq!(digest.goal.as_deref(), Some("make branch names short"));
        assert_eq!(digest.turns.len(), 2, "only the turns after the summary");
    }

    #[test]
    fn a_floor_keeps_only_what_was_said_since() {
        let since = transcript::unix_seconds("2026-10-06T00:00:06Z");
        let digest = read(&transcript("claude-digest-since", &conversation()), since).unwrap();
        assert_eq!(digest.turns.len(), 2);
        assert_eq!(digest.turns[0].text, "also cover the root case");
    }

    #[test]
    fn an_api_error_is_not_something_the_agent_said() {
        let mut records = conversation();
        records.push(json!({"type":"assistant","error":"rate_limit","isApiErrorMessage":true,
            "timestamp":"2026-10-06T00:00:08Z",
            "message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"API Error"}]}}));
        let digest = read(&transcript("claude-digest-api-error", &records), None).unwrap();
        assert!(digest.turns.iter().all(|turn| turn.text != "API Error"));
    }

    #[test]
    fn no_transcript_is_no_digest() {
        assert_eq!(read(Path::new("/nonexistent/session.jsonl"), None), None);
    }

    /// The transcript the real binary wrote in the Lab, replayed
    /// (`conformance/experiments/claude/transcript`, re-recorded on a version
    /// drift): what holds this reader to the format the vendor writes now
    /// rather than to the one these hand-written cases remember.
    #[test]
    fn the_transcript_the_real_binary_wrote_reads_as_its_turns() {
        let recorded = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/_fixtures/foreign/transcripts/claude.jsonl");
        let digest = read(&recorded, None).expect("the recorded transcript reads");

        let goal = digest.goal.as_deref().unwrap_or_default();
        assert!(
            goal.contains("UZE_CONFORMANCE_PECAN"),
            "the typed turn is the goal: {goal:?}"
        );
        assert!(
            digest
                .turns
                .iter()
                .any(|turn| turn.speaker == Speaker::Agent && !turn.text.is_empty()),
            "and the agent's answer is a turn: {:?}",
            digest.turns
        );
        assert_eq!(
            digest
                .turns
                .iter()
                .filter(|turn| turn.speaker == Speaker::Operator)
                .count(),
            1,
            "one operator turn, and no injected context taken for one: {:?}",
            digest.turns
        );
    }
}
