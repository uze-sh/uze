use serde_json::json;

use super::*;

fn servers() -> Vec<String> {
    vec![
        "direct-probe".into(),
        "direct".into(),
        "uze-mcp-conformance@uze-lab-uze-conformance".into(),
    ]
}

fn names(uses: &[Use]) -> Vec<(ResourceKind, String, Invoker)> {
    uses.iter()
        .map(|found| match &found.target {
            uze_core::usage::Target::Name(name) => (found.kind, name.clone(), found.by),
            other => panic!("named by a path: {other:?}"),
        })
        .collect()
}

fn tool(name: &str, status: &str, state: Value) -> Value {
    let mut state = state;
    state["status"] = json!(status);
    json!({"type": "tool", "name": name, "state": state})
}

fn assistant(created: u64, content: Vec<Value>) -> Value {
    json!({"type": "assistant", "time": {"created": created}, "content": content})
}

/// The shape `opencode session export` printed on 2.2.5 in the Lab.
fn export(messages: Vec<Value>) -> Value {
    json!({"info": {"id": "ses_probe"}, "messages": messages})
}

#[test]
fn a_skill_and_an_agent_the_person_named_are_their_uses() {
    let record = export(vec![
        json!({
            "type": "user",
            "time": {"created": 1_793_486_865_347_u64},
            "text": "@probe-skill UZE_PRIVATE_PHRASE",
            "skills": [{"id": "probe-skill", "name": "probe-skill", "text": "<skill_content/>"}],
            "agents": [{"name": "probe-agent"}],
        }),
        assistant(
            1_793_486_865_377,
            vec![json!({"type": "text", "text": "ok"})],
        ),
    ]);
    let uses = uses_in(&record, &servers()).unwrap();
    assert_eq!(
        names(&uses),
        [
            (ResourceKind::Skill, "probe-skill".into(), Invoker::Person),
            (ResourceKind::Agent, "probe-agent".into(), Invoker::Person),
        ]
    );
    assert_eq!(
        uses[0].at,
        UNIX_EPOCH + Duration::from_millis(1_793_486_865_347)
    );
    assert!(!format!("{uses:?}").contains("UZE_PRIVATE_PHRASE"));
}

#[test]
fn the_models_completed_calls_are_its_uses() {
    let record = export(vec![
        json!({"type": "user", "time": {"created": 1}, "text": "go"}),
        assistant(
            2,
            vec![
                tool(
                    "skill",
                    "completed",
                    json!({"input": {"id": "probe-skill"}}),
                ),
                tool("skill", "error", json!({"input": {"id": "missing"}})),
                tool(
                    "subagent",
                    "completed",
                    json!({"input": {"agent": "probe-agent", "prompt": "x"}}),
                ),
                tool("read", "completed", json!({"input": {"path": "/x"}})),
            ],
        ),
    ]);
    assert_eq!(
        names(&uses_in(&record, &servers()).unwrap()),
        [
            (ResourceKind::Skill, "probe-skill".into(), Invoker::Model),
            (ResourceKind::Agent, "probe-agent".into(), Invoker::Model),
        ]
    );
}

#[test]
fn a_mention_and_the_call_it_led_to_are_one_use() {
    let record = export(vec![
        json!({"type": "user", "time": {"created": 1}, "agents": [{"name": "probe-agent"}]}),
        assistant(
            2,
            vec![tool(
                "subagent",
                "completed",
                json!({"input": {"agent": "probe-agent"}}),
            )],
        ),
        assistant(
            3,
            vec![tool(
                "subagent",
                "completed",
                json!({"input": {"agent": "probe-agent"}}),
            )],
        ),
        json!({"type": "user", "time": {"created": 4}, "text": "again"}),
        assistant(
            5,
            vec![tool(
                "subagent",
                "completed",
                json!({"input": {"agent": "probe-agent"}}),
            )],
        ),
    ]);
    let invokers: Vec<_> = names(&uses_in(&record, &servers()).unwrap())
        .into_iter()
        .map(|(_, _, by)| by)
        .collect();
    assert_eq!(invokers, [Invoker::Person, Invoker::Model, Invoker::Model]);
}

#[test]
fn an_mcp_call_names_its_server_by_the_longest_configured_name() {
    let record = export(vec![assistant(
        1,
        vec![
            tool(
                "execute",
                "completed",
                json!({"input": {"code": "…"}, "metadata": {"toolCalls": [
                    {"tool": "direct-probe.uze_conformance", "status": "completed"},
                    {"tool": "uze-mcp-conformance_uze-lab-uze-conformance.uze_conformance", "status": "completed"},
                    {"tool": "direct-probe.broken", "status": "error"},
                    {"tool": "nobody.knows", "status": "completed"},
                ]}}),
            ),
            tool(
                "direct-probe_uze_conformance",
                "completed",
                json!({"input": {}}),
            ),
        ],
    )]);
    assert_eq!(
        names(&uses_in(&record, &servers()).unwrap()),
        [
            (ResourceKind::Mcp, "direct-probe".into(), Invoker::Model),
            (
                ResourceKind::Mcp,
                "uze-mcp-conformance@uze-lab-uze-conformance".into(),
                Invoker::Model
            ),
            (ResourceKind::Mcp, "direct-probe".into(), Invoker::Model),
        ]
    );
}

#[test]
fn an_execute_whose_server_was_not_connected_is_no_use() {
    let record = export(vec![assistant(
        1,
        vec![tool(
            "execute",
            "completed",
            json!({"input": {"code": "tools[\"direct-probe\"].x()"}, "metadata": {"toolCalls": [], "error": true}}),
        )],
    )]);
    assert!(uses_in(&record, &servers()).unwrap().is_empty());
}

#[test]
fn an_export_of_another_shape_cannot_be_read() {
    assert!(uses_in(&json!({"info": {}}), &servers()).is_none());
}

#[test]
fn the_listing_keeps_the_sessions_updated_in_the_period() {
    let listing = json!({"data": [
        {"id": "ses_new", "time": {"updated": 2_000_000_u64}, "location": {"directory": "/p"}},
        {"id": "ses_old", "time": {"updated": 1_000_u64}},
        {"title": "no id"},
    ]});
    let since = UNIX_EPOCH + Duration::from_secs(1_000);
    let records = sessions_in(&listing, since).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, "ses_new");
    assert_eq!(
        records[0].stamp,
        Stamp::of(0, UNIX_EPOCH + Duration::from_millis(2_000_000))
    );
}
