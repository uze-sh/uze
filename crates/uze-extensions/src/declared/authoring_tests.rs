//! What the inspector does to an engine for the author of a linked
//! extension: a rerun with edited input, a source fed by its fixture, a
//! synthetic event, fake agents, and what the engine holds (D25, D28).

use std::time::Instant;

use serde_json::json;
use uze_contract::{AgentEvent, EventName, NodeName, SourceDecl, SourceKind};

use super::source_tests::{Answering, answered, checkout};
use super::tests::{SOURCE, compiled_view, extension, n, texts, tree};
use super::*;

fn board() -> DeclaredExtension {
    let view = compiled_view(
        vec![n(NodeName::Text).arg("{count}").item()],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    let source = &mut compiled.manifest.sources[0];
    source.input.insert("phase".to_owned(), "12".to_owned());
    source.refresh_on = vec![
        EventName::CheckoutChanged,
        EventName::Agent(AgentEvent::TurnFinished),
    ];
    DeclaredExtension::new(compiled, "pt")
}

/// The questions the engine asks now, answered with `count`.
fn ask(engine: &mut DeclaredExtension, count: u64) -> Vec<Question> {
    let host = Answering::with(answered(json!({ "count": count })));
    for job in engine.jobs() {
        let answer = (job.run)(&host);
        engine.absorb(answer);
    }
    host.asked.into_inner()
}

#[test]
fn a_rerun_asks_through_the_host_with_the_edited_input_until_put_back() {
    let mut engine = board();
    engine.focus(Some(&checkout("/work/a")));
    let first = ask(&mut engine, 1);
    assert_eq!(first[0].input, json!({ "phase": "12" }));
    assert_eq!(engine.asked_input(SOURCE), Some(&json!({ "phase": "12" })));

    engine.rerun(SOURCE, json!({ "phase": "7" }));
    let rerun = ask(&mut engine, 7);
    assert_eq!(rerun.len(), 1, "asked once, as any question");
    assert_eq!(rerun[0].input, json!({ "phase": "7" }));
    assert_eq!(
        rerun[0].asking,
        Asking::Refresh,
        "past what the host remembers"
    );
    assert_eq!(
        texts(&tree(&engine)),
        ["7"],
        "the surface draws the new answer"
    );
    assert!(
        ask(&mut engine, 7).is_empty(),
        "the edit stands, asked once"
    );
    assert_eq!(engine.edited_input(SOURCE), Some(&json!({ "phase": "7" })));

    engine.restore_input(SOURCE);
    let restored = ask(&mut engine, 1);
    assert_eq!(restored[0].input, json!({ "phase": "12" }));
    assert_eq!(engine.edited_input(SOURCE), None);
}

#[test]
fn a_source_fed_by_its_fixture_asks_nothing_until_switched_back() {
    let mut engine = board();
    engine.focus(Some(&checkout("/work/a")));
    ask(&mut engine, 1);
    engine.feed_fixture(SOURCE, Ok(json!({ "count": 99 })));
    assert!(engine.fed_by_fixture(SOURCE));
    assert!(ask(&mut engine, 1).is_empty(), "the fixture feeds it");
    assert_eq!(texts(&tree(&engine)), ["99"]);
    engine.feed_live(SOURCE);
    assert_eq!(
        ask(&mut engine, 2).len(),
        1,
        "asked of what answers it again"
    );
    assert_eq!(texts(&tree(&engine)), ["2"]);
}

#[test]
fn a_synthetic_event_asks_its_sources_and_is_marked_where_it_is_shown() {
    let mut engine = board();
    engine.focus(Some(&checkout("/work/a")));
    ask(&mut engine, 1);
    let now = Instant::now();
    engine.emit(&EventName::CheckoutChanged, now);
    engine.emit(&EventName::Agent(AgentEvent::TurnFinished), now);
    let seen: Vec<&serde_json::Value> = engine.events_seen().collect();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|event| event["synthetic"] == json!(true)));
    assert_eq!(seen[0]["name"], json!("agent.turn.finished"));
    assert_eq!(seen[1]["checkout"], json!("/work/a"));
    let later = now + COALESCE * 2;
    engine.pass(&Pass {
        now: later,
        visible: &[super::tests::SECTION.to_owned()],
        happened: &[],
    });
    let asked = ask(&mut engine, 2);
    assert_eq!(asked.len(), 1, "a burst asks once");
    assert_eq!(asked[0].input["event"]["synthetic"], json!(true));
}

#[test]
fn fake_agents_stand_in_for_the_workspaces_until_taken_away() {
    let view = compiled_view(
        vec![n(NodeName::Text).arg("{count}").item()],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled
        .manifest
        .sources
        .push(SourceDecl::new("agents", SourceKind::Agents));
    let mut engine = DeclaredExtension::new(compiled, "pt");
    let real = json!([{ "agent": "real" }]);
    engine.agents(&real);
    engine.fake_agents(true);
    assert!(engine.faking_agents());
    assert_eq!(engine.data("agents"), Some(&Ok(fake_agents())));
    engine.agents(&json!([{ "agent": "newer" }]));
    assert_eq!(
        engine.data("agents"),
        Some(&Ok(fake_agents())),
        "the workspace's wait"
    );
    engine.fake_agents(false);
    assert_eq!(
        engine.data("agents"),
        Some(&Ok(json!([{ "agent": "newer" }])))
    );
}

#[test]
fn what_an_engine_holds_grows_with_its_data_and_stays_under_d28s_budget() {
    let mut engine = board();
    let empty = engine.approximate_bytes();
    assert!(empty > 0, "its compile weighs");
    let rows: Vec<serde_json::Value> = (0..200)
        .map(|row| json!({ "id": row, "name": format!("row {row}") }))
        .collect();
    engine.set_data(SOURCE, Ok(json!({ "count": 1, "rows": rows })));
    let _ = tree(&engine);
    let full = engine.approximate_bytes();
    assert!(full > empty + 4_000, "{empty} then {full}");
    assert!(full < 1024 * 1024, "{full} bytes");
}
