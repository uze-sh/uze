use std::{fs, time::SystemTime};

use serde_json::json;
use uze_core::usage::Target;

use super::*;

const SENTINEL: &str = "UZE_PRIVATE_PHRASE_5e1a";

fn line(value: Value) -> String {
    format!("{}\n", serde_json::to_string(&value).unwrap())
}

fn prompt(at: &str, text: &str) -> String {
    line(json!({"type": "user", "timestamp": at, "message": {"role": "user", "content": text}}))
}

fn command(at: &str, name: &str) -> String {
    prompt(
        at,
        &format!(
            "<command-message>{name}</command-message>\n<command-name>/{name}</command-name>\n<command-args>{SENTINEL}</command-args>"
        ),
    )
}

fn expanded(at: &str) -> String {
    line(json!({"type": "user", "timestamp": at, "isMeta": true,
        "message": {"role": "user", "content": [{"type": "text", "text": format!("<command-name>/hidden</command-name> {SENTINEL}")}]}}))
}

fn call(at: &str, id: &str, name: &str, input: Value) -> String {
    line(json!({"type": "assistant", "timestamp": at,
        "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": name, "input": input}]}}))
}

fn mention(at: &str, agent: &str) -> String {
    line(
        json!({"type": "attachment", "timestamp": at, "attachment": {"type": "agent_mention", "agentType": agent}}),
    )
}

fn noise() -> String {
    let blob = line(json!({"type": "api-request-blob",
        "content": [{"type": "tool_use", "id": "x", "name": "Skill", "input": {"skill": "review"}}]}));
    let listed = line(
        json!({"type": "attachment", "timestamp": "2026-10-31T10:00:00Z",
        "attachment": {"type": "deferred_tools_record", "entries": [{"name": "mcp__github__search"}]}}),
    );
    blob + &listed
}

fn uses_of(text: &str) -> Vec<Use> {
    let root = uze_testkit::temp::TempDir::new("claude-usage");
    let path = root.join("session.jsonl");
    fs::write(&path, text).unwrap();
    let record = UsageRecord {
        id: path.to_string_lossy().into_owned(),
        stamp: Default::default(),
    };
    read(&record, Resume::Start).unwrap().uses
}

fn named(uses: &[Use]) -> Vec<(ResourceKind, &str, Invoker)> {
    uses.iter()
        .map(|found| match &found.target {
            Target::Name(name) => (found.kind, name.as_str(), found.by),
            Target::Definition(_) => panic!("Claude names every use"),
        })
        .collect()
}

#[test]
fn what_the_person_typed_is_counted_as_theirs() {
    let text = [
        command("2026-10-31T10:00:00Z", "usage-probe"),
        expanded("2026-10-31T10:00:01Z"),
        command("2026-10-31T10:01:00Z", "flow:review"),
        command("2026-10-31T10:02:00Z", "mcp__probe__probe_prompt"),
    ]
    .concat();
    assert_eq!(
        named(&uses_of(&text)),
        [
            (ResourceKind::Skill, "usage-probe", Invoker::Person),
            (ResourceKind::Skill, "flow:review", Invoker::Person),
            (ResourceKind::Mcp, "probe", Invoker::Person),
        ]
    );
}

#[test]
fn what_the_model_called_is_counted_once_per_call() {
    let text = [
        call(
            "2026-10-31T10:00:00Z",
            "t1",
            "Skill",
            json!({"skill": "usage-probe"}),
        ),
        call(
            "2026-10-31T10:00:01Z",
            "t1",
            "Skill",
            json!({"skill": "usage-probe"}),
        ),
        call(
            "2026-10-31T10:00:02Z",
            "t2",
            "Agent",
            json!({"subagent_type": "flow:scout", "prompt": SENTINEL}),
        ),
        call(
            "2026-10-31T10:00:03Z",
            "t3",
            "mcp__github__search",
            json!({"q": SENTINEL}),
        ),
        call(
            "2026-10-31T10:00:04Z",
            "t4",
            "mcp__plugin_uze-mcp-conformance_uze-conformance__uze_conformance",
            json!({}),
        ),
        call(
            "2026-10-31T10:00:05Z",
            "t5",
            "Read",
            json!({"file_path": "/x"}),
        ),
        noise(),
    ]
    .concat();
    assert_eq!(
        named(&uses_of(&text)),
        [
            (ResourceKind::Skill, "usage-probe", Invoker::Model),
            (ResourceKind::Agent, "flow:scout", Invoker::Model),
            (ResourceKind::Mcp, "github", Invoker::Model),
            (ResourceKind::Mcp, "uze-conformance", Invoker::Model),
        ]
    );
}

#[test]
fn a_mention_and_the_dispatch_it_asked_for_are_one_use() {
    let text = [
        mention("2026-10-31T10:00:00Z", "usage-probe-agent"),
        call(
            "2026-10-31T10:00:02Z",
            "t1",
            "Agent",
            json!({"subagent_type": "usage-probe-agent"}),
        ),
        call(
            "2026-10-31T10:05:00Z",
            "t2",
            "Agent",
            json!({"subagent_type": "usage-probe-agent"}),
        ),
        mention("2026-10-31T11:00:00Z", "usage-probe-agent"),
        call(
            "2026-10-31T13:00:00Z",
            "t3",
            "Agent",
            json!({"subagent_type": "usage-probe-agent"}),
        ),
    ]
    .concat();
    assert_eq!(
        named(&uses_of(&text)),
        [
            (ResourceKind::Agent, "usage-probe-agent", Invoker::Person),
            (ResourceKind::Agent, "usage-probe-agent", Invoker::Model),
            (ResourceKind::Agent, "usage-probe-agent", Invoker::Person),
            (ResourceKind::Agent, "usage-probe-agent", Invoker::Model),
        ]
    );
}

#[test]
fn nothing_said_around_a_name_leaves_the_record() {
    let text = [
        command("2026-10-31T10:00:00Z", "usage-probe"),
        prompt(
            "2026-10-31T10:00:01Z",
            &format!("<command-name>/{SENTINEL} spaced</command-name>"),
        ),
        call(
            "2026-10-31T10:00:02Z",
            "t1",
            "Agent",
            json!({"subagent_type": "a", "prompt": SENTINEL}),
        ),
    ]
    .concat();
    let found = format!("{:?}", uses_of(&text));
    assert!(!found.contains(SENTINEL), "{found}");
}

#[test]
fn a_line_without_a_time_is_no_use() {
    let text = line(json!({"type": "assistant",
        "message": {"content": [{"type": "tool_use", "id": "t", "name": "Skill", "input": {"skill": "x"}}]}}));
    assert!(uses_of(&text).is_empty());
}

#[test]
fn a_session_appended_to_is_read_from_where_it_was_left() {
    let root = uze_testkit::temp::TempDir::new("claude-usage-resume");
    let project = root.join("projects/-home-me-app");
    fs::create_dir_all(project.join("s1/subagents")).unwrap();
    let path = project.join("s1.jsonl");
    fs::write(&path, command("2026-10-31T10:00:00Z", "usage-probe")).unwrap();
    fs::write(
        project.join("s1/subagents/agent-a.jsonl"),
        call(
            "2026-10-31T10:00:00Z",
            "t9",
            "Skill",
            json!({"skill": "flow:review"}),
        ),
    )
    .unwrap();
    let listed = records(&root.join("projects"), SystemTime::UNIX_EPOCH);
    assert_eq!(listed.len(), 2, "the session and its subagent");
    let record = listed
        .into_iter()
        .find(|record| record.id.ends_with("s1.jsonl"))
        .unwrap();
    let first = read(&record, Resume::Start).unwrap();
    assert_eq!(first.uses.len(), 1);
    let mut grown = fs::read_to_string(&path).unwrap();
    grown.push_str(&command("2026-10-31T10:05:00Z", "flow:review"));
    fs::write(&path, grown).unwrap();
    let next = read(&record, Resume::At(first.resume_at.unwrap())).unwrap();
    assert_eq!(
        named(&next.uses),
        [(ResourceKind::Skill, "flow:review", Invoker::Person)]
    );
}
