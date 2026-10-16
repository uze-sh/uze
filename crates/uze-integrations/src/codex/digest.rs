//! What a Codex rollout says a conversation was about, for the briefing of
//! an agent that continues on another harness.
//!
//! Codex writes two accounts of every turn: `response_item` records, which
//! are what the model was sent — the operator's words, but also the
//! `AGENTS.md` and environment Codex injects as if the operator had said
//! them — and `event_msg` `item_completed` records, which are typed by what
//! happened. Only the second is read:
//! - `UserMessage` is the operator's turn, `AgentMessage` the agent's own
//!   text, `CommandExecution` a command it ran with its `exit_code`;
//! - a top-level `compacted` record is Codex's own summary of everything
//!   before it;
//! - `token_count` records carry the plan's `rate_limits`, which say when a
//!   limit was reached and when each window resets.
//!
//! Reasoning, file changes and everything else are left out. Older Codex
//! builds wrote `user_message`/`agent_message` events instead; those read
//! as no turns, and the briefing falls back on what Git says.

use std::path::Path;

use serde_json::Value;
use uze_core::session::{ConversationDigest, LastCommand, Speaker, Turn, UsageLimit};

use crate::shared::transcript;

/// How much of a rollout's end is enough to find its latest `token_count`.
const LIMIT_TAIL_BYTES: u64 = 64 * 1024;

pub(super) fn read(rollout: &Path, since_unix: Option<u64>) -> Option<ConversationDigest> {
    let mut digest = ConversationDigest::default();
    for record in transcript::records(rollout)? {
        let at = record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(transcript::unix_seconds);
        let recent = since_unix.is_none_or(|since| at.is_some_and(|at| at >= since));
        match record.get("type").and_then(Value::as_str) {
            Some("compacted") if recent => {
                if let Some(summary) = record.pointer("/payload/message").and_then(Value::as_str) {
                    digest.summary = Some(summary.to_owned());
                    digest.turns.clear();
                }
            }
            Some("event_msg") => match record.pointer("/payload/type").and_then(Value::as_str) {
                Some("item_completed") => {
                    let Some(item) = record.pointer("/payload/item") else {
                        continue;
                    };
                    item_completed(&mut digest, item, at, recent);
                }
                Some("token_count") => digest.limit = reached(&record),
                _ => {}
            },
            _ => {}
        }
    }
    digest.limit = digest.limit.filter(transcript::still_in_force);
    Some(digest)
}

fn item_completed(digest: &mut ConversationDigest, item: &Value, at: Option<u64>, recent: bool) {
    match item.get("type").and_then(Value::as_str) {
        Some("UserMessage") => {
            let text = text_of(item);
            if text.is_empty() {
                return;
            }
            if digest.goal.is_none() {
                digest.goal = Some(text.clone());
            }
            if recent {
                digest.turns.push(turn(Speaker::Operator, text, at));
            }
        }
        Some("AgentMessage") if recent => {
            let text = text_of(item);
            if !text.is_empty() {
                digest.turns.push(turn(Speaker::Agent, text, at));
            }
        }
        Some("CommandExecution") => {
            let Some(argv) = item.get("command").and_then(Value::as_array) else {
                return;
            };
            let line = argv
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            digest.last_command = Some(LastCommand {
                line,
                failed: item
                    .get("exit_code")
                    .and_then(Value::as_i64)
                    .map(|code| code != 0),
            });
        }
        _ => {}
    }
}

/// Whether the rollout ends at a usage limit that has not lifted yet.
pub(super) fn limit(rollout: &Path) -> Option<UsageLimit> {
    transcript::tail_records(rollout, LIMIT_TAIL_BYTES)
        .iter()
        .rev()
        .find(|record| {
            record.pointer("/payload/type").and_then(Value::as_str) == Some("token_count")
        })
        .and_then(reached)
        .filter(transcript::still_in_force)
}

/// The limit a `token_count` record reports reached: the window that is
/// spent, and when it resets.
fn reached(record: &Value) -> Option<UsageLimit> {
    let limits = record.pointer("/payload/rate_limits")?;
    let spent = ["primary", "secondary"]
        .iter()
        .filter_map(|window| limits.get(*window))
        .find(|window| {
            window
                .get("used_percent")
                .and_then(Value::as_f64)
                .is_some_and(|used| used >= 100.0)
        });
    let flagged = limits
        .get("rate_limit_reached_type")
        .is_some_and(|reached| !reached.is_null());
    if spent.is_none() && !flagged {
        return None;
    }
    Some(UsageLimit {
        resets_at_unix: spent
            .and_then(|window| window.get("resets_at"))
            .and_then(Value::as_u64),
    })
}

/// The text parts of an item's `content`, joined.
fn text_of(item: &Value) -> String {
    item.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn turn(speaker: Speaker, text: String, at_unix: Option<u64>) -> Turn {
    Turn {
        speaker,
        text,
        at_unix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rollout(label: &str, records: &[Value]) -> std::path::PathBuf {
        let path = uze_testkit::temp::scratch(label).join("rollout.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: Vec<String> = records.iter().map(Value::to_string).collect();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    fn completed(at: &str, item: Value) -> Value {
        json!({"timestamp":at,"type":"event_msg","payload":{"type":"item_completed","item":item}})
    }

    fn token_count(at: &str, used: f64, reached: Value) -> Value {
        json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","rate_limits":{
            "primary":{"used_percent":used,"resets_at":4_102_444_800_u64,"window_minutes":300},
            "secondary":{"used_percent":10.0,"resets_at":4_102_999_999_u64,"window_minutes":10080},
            "rate_limit_reached_type":reached}}})
    }

    /// The shapes codex-cli 0.162 writes, scrubbed to what the reader keys on.
    fn conversation() -> Vec<Value> {
        vec![
            json!({"timestamp":"2026-10-06T00:00:00Z","type":"session_meta","payload":{"session_id":"s","cwd":"/w"}}),
            json!({"timestamp":"2026-10-06T00:00:00Z","type":"response_item","payload":{"type":"message","role":"user",
                "content":[{"type":"input_text","text":"# AGENTS.md instructions"}]}}),
            completed(
                "2026-10-06T00:00:01Z",
                json!({"type":"UserMessage","id":"u1",
                "content":[{"type":"text","text":"make branch names short","text_elements":[]}]}),
            ),
            completed(
                "2026-10-06T00:00:02Z",
                json!({"type":"Reasoning","id":"r1","summary_text":["private"]}),
            ),
            completed(
                "2026-10-06T00:00:03Z",
                json!({"type":"AgentMessage","id":"a1","phase":"commentary",
                "content":[{"type":"Text","text":"Looking at the namer."}]}),
            ),
            completed(
                "2026-10-06T00:00:04Z",
                json!({"type":"CommandExecution","id":"c1",
                "command":["cargo","test","-p","uze-workspace"],"exit_code":101,"status":"failed"}),
            ),
            token_count("2026-10-06T00:00:04Z", 40.0, Value::Null),
            completed(
                "2026-10-06T00:00:05Z",
                json!({"type":"UserMessage","id":"u2",
                "content":[{"type":"text","text":"also cover the root case","text_elements":[]}]}),
            ),
            completed(
                "2026-10-06T00:00:06Z",
                json!({"type":"AgentMessage","id":"a2","phase":"final_answer",
                "content":[{"type":"Text","text":"The root case now names the label."}]}),
            ),
        ]
    }

    #[test]
    fn a_rollout_reads_as_its_goal_its_turns_and_its_last_command() {
        let digest = read(&rollout("codex-digest", &conversation()), None).unwrap();

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
            "never the injected AGENTS.md, never reasoning"
        );
        assert_eq!(
            digest.last_command,
            Some(LastCommand {
                line: "cargo test -p uze-workspace".into(),
                failed: Some(true),
            })
        );
        assert_eq!(digest.limit, None);
    }

    #[test]
    fn a_compaction_replaces_what_came_before_it() {
        let mut records = conversation();
        records.insert(
            7,
            json!({"timestamp":"2026-10-06T00:00:05Z","type":"compacted",
                   "payload":{"message":"Earlier: the namer was rewritten.","replacement_history":[]}}),
        );
        let digest = read(&rollout("codex-digest-compact", &records), None).unwrap();
        assert_eq!(
            digest.summary.as_deref(),
            Some("Earlier: the namer was rewritten.")
        );
        assert_eq!(digest.turns.len(), 2);
    }

    #[test]
    fn a_floor_keeps_only_what_was_said_since() {
        let since = transcript::unix_seconds("2026-10-06T00:00:05Z");
        let digest = read(&rollout("codex-digest-since", &conversation()), since).unwrap();
        assert_eq!(digest.turns.len(), 2);
        assert_eq!(digest.goal.as_deref(), Some("make branch names short"));
    }

    #[test]
    fn a_spent_window_is_the_limit_and_when_it_resets() {
        let mut records = conversation();
        records.push(token_count("2026-10-06T00:00:07Z", 100.0, json!("primary")));
        let path = rollout("codex-limit", &records);
        let expected = Some(UsageLimit {
            resets_at_unix: Some(4_102_444_800),
        });
        assert_eq!(limit(&path), expected);
        assert_eq!(read(&path, None).unwrap().limit, expected);
    }

    #[test]
    fn a_window_with_room_left_is_no_limit() {
        assert_eq!(limit(&rollout("codex-no-limit", &conversation())), None);
    }
}
