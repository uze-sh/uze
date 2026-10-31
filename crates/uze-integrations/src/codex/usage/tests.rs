use std::fs;

use serde_json::json;
use uze_core::usage::Stamp;

use super::*;

const SENTINEL: &str = "UZE_PRIVATE_PHRASE_41c7";

struct World {
    root: uze_testkit::temp::TempDir,
}

impl World {
    fn new(name: &str) -> Self {
        let root = uze_testkit::temp::TempDir::new(name);
        fs::create_dir_all(root.join(".codex/sessions/2026/10/31")).unwrap();
        fs::write(
            root.join(".codex/config.toml"),
            "[mcp_servers.uze-conformance]\ncommand = \"probe\"\n",
        )
        .unwrap();
        Self { root }
    }

    fn rollout(&self) -> PathBuf {
        self.root
            .join(".codex/sessions/2026/10/31/rollout-2026-10-31T10-00-00-a.jsonl")
    }

    fn write(&self, lines: &[Value]) {
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        fs::write(self.rollout(), text).unwrap();
    }

    fn read(&self, from: Resume) -> UsageRead {
        let record = UsageRecord {
            id: self.rollout().to_string_lossy().into_owned(),
            stamp: Stamp::default(),
        };
        read(&record, from, &self.root.join(".codex/config.toml")).unwrap()
    }
}

fn line(kind: &str, payload: Value) -> Value {
    json!({"timestamp": "2026-10-31T10:00:00.000Z", "type": kind, "payload": payload})
}

fn mention(turn: &str, name: &str, path: &str) -> Value {
    line(
        "response_item",
        json!({
            "type": "message", "role": "user",
            "content": [
                {"type": "input_text", "text": format!("${name} {SENTINEL}")},
                {"type": "input_text", "text": format!("<skill>\n<name>{name}</name>\n<path>{path}</path>\nWrite the report.\n</skill>")}
            ],
            "internal_chat_message_metadata_passthrough": {"turn_id": turn}
        }),
    )
}

fn command(turn: &str, cmd: &str, path: &str, exit_code: i64) -> Value {
    line(
        "event_msg",
        json!({
            "type": "item_completed", "turn_id": turn,
            "item": {
                "type": "CommandExecution", "command": ["bash", "-lc", cmd],
                "cwd": "/work/project",
                "parsed_cmd": [{"type": "read", "cmd": cmd, "name": "SKILL.md", "path": path}],
                "exit_code": exit_code, "aggregated_output": SENTINEL
            }
        }),
    )
}

fn targets(read: &UsageRead) -> Vec<(ResourceKind, Target, Invoker, bool)> {
    read.uses
        .iter()
        .map(|found| (found.kind, found.target.clone(), found.by, found.exact))
        .collect()
}

#[test]
fn a_typed_skill_and_the_read_it_led_to_are_one_use_by_the_person() {
    let world = World::new("codex-usage-mention");
    let skill = "/home/me/.agents/skills/probe-skill/SKILL.md";
    world.write(&[
        mention("t1", "probe-skill", skill),
        command("t1", &format!("cat {skill}"), skill, 0),
        command("t2", &format!("cat {skill}"), skill, 0),
    ]);
    let read = world.read(Resume::Start);
    assert_eq!(
        targets(&read),
        [
            (
                ResourceKind::Skill,
                Target::Definition(skill.into()),
                Invoker::Person,
                true
            ),
            (
                ResourceKind::Skill,
                Target::Definition(skill.into()),
                Invoker::Model,
                true
            ),
        ]
    );
}

#[test]
fn a_read_that_failed_or_named_an_alias_counts_nothing() {
    let world = World::new("codex-usage-alias");
    world.write(&[
        command(
            "t1",
            "cat r0/probe-skill/SKILL.md",
            "r0/probe-skill/SKILL.md",
            1,
        ),
        command(
            "t1",
            "cat /home/me/.agents/skills/gone/SKILL.md",
            "/home/me/.agents/skills/gone/SKILL.md",
            1,
        ),
    ]);
    assert!(world.read(Resume::Start).uses.is_empty());
}

#[test]
fn a_relative_read_is_resolved_against_the_commands_directory() {
    let world = World::new("codex-usage-relative");
    world.write(&[command(
        "t1",
        "cat .agents/skills/local/SKILL.md",
        ".agents/skills/local/SKILL.md",
        0,
    )]);
    assert_eq!(
        targets(&world.read(Resume::Start))[0].1,
        Target::Definition("/work/project/.agents/skills/local/SKILL.md".into())
    );
}

#[test]
fn a_skill_file_named_in_an_unparsed_command_still_counts() {
    let world = World::new("codex-usage-unknown-command");
    let skill = "/home/me/.agents/skills/probe-skill/SKILL.md";
    let mut unknown = command("t1", "", skill, 0);
    unknown["payload"]["item"]["parsed_cmd"] =
        json!([{"type": "unknown", "cmd": format!("sed -n '1,200p' '{skill}'")}]);
    world.write(&[unknown]);
    assert_eq!(
        targets(&world.read(Resume::Start))[0].1,
        Target::Definition(skill.into())
    );
}

#[test]
fn an_agent_is_counted_by_its_dispatch() {
    let world = World::new("codex-usage-agent");
    world.write(&[line(
        "response_item",
        json!({
            "type": "function_call", "name": "spawn_agent", "namespace": "collaboration",
            "arguments": json!({"task_name": "lab", "message": SENTINEL, "agent_type": "probe_agent"}).to_string(),
            "call_id": "c1", "internal_chat_message_metadata_passthrough": {"turn_id": "t1"}
        }),
    )]);
    assert_eq!(
        targets(&world.read(Resume::Start)),
        [(
            ResourceKind::Agent,
            Target::Name("probe_agent".into()),
            Invoker::Model,
            true
        )]
    );
}

#[test]
fn an_mcp_tool_spelled_in_a_script_is_an_estimate_of_the_configured_server() {
    let world = World::new("codex-usage-mcp");
    let script = |body: &str| {
        line(
            "response_item",
            json!({"type": "custom_tool_call", "name": "exec", "call_id": "c1", "input": body,
                   "internal_chat_message_metadata_passthrough": {"turn_id": "t1"}}),
        )
    };
    world.write(&[
        script("const r = await tools.mcp__uze_conformance__uze_conformance({});"),
        script("const r = await tools.mcp__uze_conformance__uze_conformance({});"),
        script("const f = ALL_TOOLS.find((t) => t.name.includes('mcp__unknown__x'));"),
    ]);
    assert_eq!(
        targets(&world.read(Resume::Start)),
        vec![
            (
                ResourceKind::Mcp,
                Target::Name("uze-conformance".into()),
                Invoker::Model,
                false
            );
            2
        ]
    );
}

#[test]
fn a_reading_resumes_where_the_last_one_stopped() {
    let world = World::new("codex-usage-resume");
    let skill = "/home/me/.agents/skills/probe-skill/SKILL.md";
    world.write(&[mention("t1", "probe-skill", skill)]);
    let first = world.read(Resume::Start);
    let offset = first.resume_at.unwrap();
    world.write(&[
        mention("t1", "probe-skill", skill),
        mention("t2", "probe-skill", skill),
    ]);
    let next = world.read(Resume::At(offset));
    assert_eq!(next.uses.len(), 1);
    assert!(next.resume_at.unwrap() > offset);
}

#[test]
fn nothing_the_person_or_a_command_said_reaches_a_use() {
    let world = World::new("codex-usage-private");
    let skill = "/home/me/.agents/skills/probe-skill/SKILL.md";
    world.write(&[
        mention("t1", "probe-skill", skill),
        command("t1", &format!("cat {skill}"), skill, 0),
    ]);
    let read = world.read(Resume::Start);
    assert!(!format!("{:?}", read.uses).contains(SENTINEL));
}

#[test]
fn only_rollouts_are_records() {
    let world = World::new("codex-usage-records");
    world.write(&[]);
    fs::write(
        world.root.join(".codex/sessions/2026/10/31/notes.jsonl"),
        "",
    )
    .unwrap();
    let found = records(&world.root.join(".codex/sessions"), UNIX_EPOCH);
    assert_eq!(found.len(), 1);
    assert!(found[0].id.ends_with("rollout-2026-10-31T10-00-00-a.jsonl"));
}
