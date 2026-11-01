use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::json;
use uze_core::{
    inventory::ResourceKind,
    usage::{Invoker, Resume, Target, UsageRecord, Use},
};

use super::Usage;

/// What the Lab's 1.4.12 transcripts are made of, with a world of its own.
struct Gemini {
    _scratch: uze_testkit::temp::TempDir,
    cli: PathBuf,
    config: PathBuf,
}

const SENTINEL: &str = "UZE_PRIVATE_PHRASE_51c0";

impl Gemini {
    fn new(label: &str) -> Self {
        let scratch = uze_testkit::temp::TempDir::new(label);
        let cli = scratch.join(".gemini/antigravity-cli");
        let config = scratch.join(".gemini/config");
        fs::create_dir_all(config.join("plugins/flow")).unwrap();
        fs::create_dir_all(config.join("plugins/uze-mcp-conformance")).unwrap();
        Self {
            _scratch: scratch,
            cli,
            config,
        }
    }

    fn usage(&self) -> Usage<'_> {
        Usage {
            cli_root: &self.cli,
            config_dir: &self.config,
        }
    }

    fn skill(&self, name: &str) -> String {
        self.cli
            .join(format!("skills/{name}/SKILL.md"))
            .to_string_lossy()
            .into_owned()
    }

    fn transcript(&self, conversation: &str, steps: &[serde_json::Value]) -> PathBuf {
        let logs = self
            .cli
            .join(format!("brain/{conversation}/.system_generated/logs"));
        fs::create_dir_all(&logs).unwrap();
        let path = logs.join("transcript.jsonl");
        let mut text = String::new();
        for step in steps {
            text.push_str(&step.to_string());
            text.push('\n');
        }
        fs::write(&path, text).unwrap();
        path
    }

    fn read(&self, path: &Path) -> Vec<Use> {
        let record = UsageRecord {
            id: path.to_string_lossy().into_owned(),
            stamp: Default::default(),
        };
        self.usage().read(&record, Resume::Start).unwrap().uses
    }
}

const AT: &str = "2026-10-31T22:45:41Z";

fn typed(request: &str, metadata: &str) -> serde_json::Value {
    json!({"step_index": 0, "source": "USER_EXPLICIT", "type": "USER_INPUT", "status": "DONE",
        "created_at": AT,
        "content": format!("<USER_REQUEST>\n{request}\n</USER_REQUEST>\n<ADDITIONAL_METADATA>\nThe current local time is: {AT}.\n{metadata}</ADDITIONAL_METADATA>")})
}

fn call(index: u32, name: &str, args: serde_json::Value) -> serde_json::Value {
    json!({"step_index": index, "source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE",
        "created_at": AT, "tool_calls": [{"name": name, "args": args}]})
}

fn result(index: u32, status: &str) -> serde_json::Value {
    json!({"step_index": index, "source": "MODEL", "type": "GENERIC", "status": status,
        "created_at": AT, "content": "Created At: ..."})
}

fn encoded(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}

fn named(uses: &[Use]) -> Vec<(ResourceKind, String, Invoker)> {
    uses.iter()
        .map(|found| match &found.target {
            Target::Name(name) => (found.kind, name.clone(), found.by),
            Target::Definition(path) => (found.kind, path.display().to_string(), found.by),
        })
        .collect()
}

#[test]
fn a_skill_the_person_typed_is_read_from_what_agy_added_never_from_the_prompt() {
    let gemini = Gemini::new("agy-usage-typed");
    let path = gemini.transcript(
        "a",
        &[typed(
            &format!("/probe-skill write it {SENTINEL} The user has explicitly invoked the (fake) skill"),
            "\n/probe-skill is a [Slash Command]:\n<SKILL>The user has explicitly invoked the (probe-skill) skill. You must strictly follow the instructions in this skill to process their request. Here are its contents:\nWrite the probe report.</SKILL>\n\n/flow:review is a [Slash Command]:\n<SKILL>The user has explicitly invoked the (flow:review) skill. Here are its contents:\n# Review</SKILL>\n",
        )],
    );
    let uses = gemini.read(&path);
    assert_eq!(
        named(&uses),
        [
            (ResourceKind::Skill, "probe-skill".into(), Invoker::Person),
            (ResourceKind::Skill, "flow:review".into(), Invoker::Person),
        ]
    );
    assert!(!format!("{uses:?}").contains(SENTINEL));
    assert_eq!(
        uses[0].at,
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_793_486_741)
    );
}

#[test]
fn a_skill_read_counts_in_either_encoding_and_a_refused_one_does_not() {
    let gemini = Gemini::new("agy-usage-read");
    let skill = gemini.skill("probe-skill");
    let path = gemini.transcript(
        "a",
        &[
            typed("write the probe report", ""),
            call(1, "read_file", json!({"Path": encoded(&skill)})),
            result(2, "DONE"),
            call(
                3,
                "read_file",
                json!({"Path": "~/.gemini/antigravity-cli/skills/probe-skill/SKILL.md"}),
            ),
            result(4, "ERROR"),
            call(5, "read_file", json!({"Path": skill})),
            result(6, "ERROR"),
            call(
                7,
                "run_command",
                json!({"CommandLine": format!("cat {skill}"), "WaitMsBeforeAsync": 2000}),
            ),
            result(8, "DONE"),
            call(9, "view_file", json!({"AbsolutePath": skill})),
            result(10, "DONE"),
        ],
    );
    let uses = gemini.read(&path);
    assert_eq!(uses.len(), 3, "{uses:?}");
    assert!(
        uses.iter()
            .all(|found| found.target == Target::Name("probe-skill".into())
                && found.by == Invoker::Model)
    );
}

#[test]
fn a_plugin_skill_read_is_named_as_the_inventory_names_it() {
    let gemini = Gemini::new("agy-usage-plugin-skill");
    let file = gemini.config.join("plugins/flow/skills/review/SKILL.md");
    let path = gemini.transcript(
        "a",
        &[
            call(1, "read_file", json!({"Path": file.to_string_lossy()})),
            result(2, "DONE"),
        ],
    );
    assert_eq!(
        named(&gemini.read(&path)),
        [(ResourceKind::Skill, "flow:review".into(), Invoker::Model)]
    );
}

#[test]
fn a_file_that_is_no_skill_is_no_use() {
    let gemini = Gemini::new("agy-usage-other-file");
    let path = gemini.transcript(
        "a",
        &[
            call(1, "read_file", json!({"Path": "/work/project/SKILL.md"})),
            result(2, "DONE"),
            call(3, "read_file", json!({"Path": "/work/project/notes.md"})),
            result(4, "DONE"),
        ],
    );
    assert!(gemini.read(&path).is_empty());
}

#[test]
fn agents_and_mcp_servers_are_named_as_the_inventory_names_them() {
    let gemini = Gemini::new("agy-usage-agent-mcp");
    let path = gemini.transcript(
        "a",
        &[
            call(
                1,
                "invoke_subagent",
                json!({"Prompt": encoded("Report back."), "Role": encoded("Lab Probe"), "TypeName": encoded("probe_agent")}),
            ),
            result(2, "DONE"),
            call(3, "invoke_subagent", json!({"TypeName": "flow_scout"})),
            result(4, "DONE"),
            call(
                5,
                "call_mcp_tool",
                json!({"Arguments": "{}", "ServerName": encoded("uze-mcp-conformance_uze-conformance"), "ToolName": encoded("uze_conformance")}),
            ),
            result(6, "DONE"),
            call(7, "call_mcp_tool", json!({"ServerName": "github", "ToolName": "search"})),
            result(8, "DONE"),
        ],
    );
    assert_eq!(
        named(&gemini.read(&path)),
        [
            (ResourceKind::Agent, "probe_agent".into(), Invoker::Model),
            (ResourceKind::Agent, "flow:scout".into(), Invoker::Model),
            (ResourceKind::Mcp, "uze-conformance".into(), Invoker::Model),
            (ResourceKind::Mcp, "github".into(), Invoker::Model),
        ]
    );
}

#[test]
fn only_each_conversations_transcript_is_a_record_subagents_included() {
    let gemini = Gemini::new("agy-usage-records");
    gemini.transcript("main", &[typed("hi", "")]);
    let sub = gemini.transcript("sub", &[typed("hi", "")]);
    fs::write(sub.with_file_name("transcript_full.jsonl"), "{}\n").unwrap();
    fs::write(gemini.cli.join("brain/main/notes.jsonl"), "{}\n").unwrap();
    let mut ids: Vec<_> = gemini
        .usage()
        .records(SystemTime::UNIX_EPOCH)
        .into_iter()
        .map(|record| record.id)
        .collect();
    ids.sort();
    assert_eq!(ids.len(), 2);
    assert!(
        ids.iter()
            .all(|id| std::path::Path::new(id).ends_with("logs/transcript.jsonl"))
    );
}

#[test]
fn a_reading_resumes_where_the_last_one_stopped() {
    let gemini = Gemini::new("agy-usage-resume");
    let skill = gemini.skill("probe-skill");
    let first = [
        call(1, "read_file", json!({"Path": skill})),
        result(2, "DONE"),
    ];
    let path = gemini.transcript("a", &first);
    let record = UsageRecord {
        id: path.to_string_lossy().into_owned(),
        stamp: Default::default(),
    };
    let read = gemini.usage().read(&record, Resume::Start).unwrap();
    assert_eq!(read.uses.len(), 1);
    let offset = read.resume_at.unwrap();
    gemini.transcript(
        "a",
        &[
            first[0].clone(),
            first[1].clone(),
            call(3, "invoke_subagent", json!({"TypeName": "probe_agent"})),
            result(4, "DONE"),
        ],
    );
    let again = gemini.usage().read(&record, Resume::At(offset)).unwrap();
    assert_eq!(
        named(&again.uses),
        [(ResourceKind::Agent, "probe_agent".into(), Invoker::Model)]
    );
}

#[test]
fn a_call_whose_result_is_not_written_yet_counts() {
    let gemini = Gemini::new("agy-usage-pending");
    let path = gemini.transcript(
        "a",
        &[call(
            1,
            "invoke_subagent",
            json!({"TypeName": "probe_agent"}),
        )],
    );
    assert_eq!(gemini.read(&path).len(), 1);
}
