//! A source asked again by its `every` while a surface showing it is in
//! view, and by the events its `refresh-on` names, a burst of them once.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;
use uze_contract::{EventName, NodeName};

use super::source_tests::{Answering, answered, checkout};
use super::tests::{SECTION, VIEW, compiled_view, extension, n};
use super::*;

fn board(every: Option<Duration>, refresh_on: Vec<EventName>) -> DeclaredExtension {
    let view = compiled_view(
        vec![n(NodeName::Text).arg("{count}").item()],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    let source = &mut compiled.manifest.sources[0];
    source.every = every;
    source.refresh_on = refresh_on;
    DeclaredExtension::new(compiled, "pt")
}

fn planning() -> EventName {
    EventName::PathsChanged {
        paths: vec![".planning".to_owned()],
    }
}

fn saved(root: &str) -> CheckoutEvent {
    CheckoutEvent {
        checkout: PathBuf::from(root),
        event: planning(),
    }
}

/// One pass of the host at `now`, then the jobs it hands, answered.
fn pass(
    engine: &mut DeclaredExtension,
    now: Instant,
    visible: &[&str],
    happened: &[CheckoutEvent],
) -> Vec<Asking> {
    let visible: Vec<String> = visible.iter().map(|id| (*id).to_owned()).collect();
    engine.pass(&Pass {
        now,
        visible: &visible,
        happened,
    });
    let host = Answering::with(answered(json!({ "count": 1 })));
    for job in engine.jobs() {
        let answer = (job.run)(&host);
        engine.absorb(answer);
    }
    host.asked
        .into_inner()
        .into_iter()
        .map(|question| question.asking)
        .collect()
}

#[test]
fn every_asks_again_only_while_a_surface_showing_the_source_is_in_view() {
    let start = Instant::now();
    let mut engine = board(Some(Duration::from_secs(10)), Vec::new());
    engine.focus(Some(&checkout("/work/a")));
    assert_eq!(pass(&mut engine, start, &[], &[]), [Asking::Source]);
    let later = start + Duration::from_secs(30);
    assert!(
        pass(&mut engine, later, &[], &[]).is_empty(),
        "the section is folded and no view is open"
    );
    assert_eq!(
        pass(&mut engine, later, &[SECTION], &[]),
        [Asking::Refresh],
        "past the cache: the answer is the clock's to renew"
    );
    let soon = later + Duration::from_secs(9);
    assert!(pass(&mut engine, soon, &[SECTION], &[]).is_empty());
    let due = later + Duration::from_secs(10);
    assert_eq!(pass(&mut engine, due, &[SECTION], &[]), [Asking::Refresh]);
}

#[test]
fn a_burst_of_saves_under_a_declared_path_asks_once_when_its_window_closes() {
    let start = Instant::now();
    let mut engine = board(None, vec![planning()]);
    engine.focus(Some(&checkout("/work/a")));
    assert_eq!(pass(&mut engine, start, &[SECTION], &[]).len(), 1);
    let saves = [saved("/work/a")];
    assert!(pass(&mut engine, start, &[SECTION], &saves).is_empty());
    let burst = start + Duration::from_millis(100);
    assert!(pass(&mut engine, burst, &[SECTION], &saves).is_empty());
    let closed = start + COALESCE;
    assert_eq!(
        pass(&mut engine, closed, &[SECTION], &[]),
        [Asking::Refresh]
    );
    assert!(pass(&mut engine, closed + COALESCE, &[SECTION], &[]).is_empty());
}

#[test]
fn an_event_about_another_checkout_or_path_asks_nothing() {
    let start = Instant::now();
    let mut engine = board(None, vec![planning()]);
    engine.focus(Some(&checkout("/work/a")));
    pass(&mut engine, start, &[SECTION], &[]);
    let elsewhere = [
        saved("/work/b"),
        CheckoutEvent {
            checkout: PathBuf::from("/work/a"),
            event: EventName::PathsChanged {
                paths: vec!["docs".to_owned()],
            },
        },
        CheckoutEvent {
            checkout: PathBuf::from("/work/a"),
            event: EventName::CheckoutChanged,
        },
    ];
    pass(&mut engine, start, &[SECTION], &elsewhere);
    assert!(pass(&mut engine, start + COALESCE, &[SECTION], &[]).is_empty());
}

#[test]
fn an_event_while_nothing_shows_the_source_is_asked_once_one_does() {
    let start = Instant::now();
    let mut engine = board(None, vec![EventName::CheckoutChanged]);
    engine.focus(Some(&checkout("/work/a")));
    pass(&mut engine, start, &[], &[]);
    let changed = [CheckoutEvent {
        checkout: PathBuf::from("/work/a"),
        event: EventName::CheckoutChanged,
    }];
    pass(&mut engine, start, &[], &changed);
    let closed = start + COALESCE;
    assert!(pass(&mut engine, closed, &[], &[]).is_empty(), "hidden");
    engine.open(VIEW, &checkout("/work/a"));
    let later = closed + Duration::from_secs(1);
    assert_eq!(pass(&mut engine, later, &[VIEW], &[]), [Asking::Refresh]);
    assert!(pass(&mut engine, later, &[VIEW], &[]).is_empty());
}

#[test]
fn only_a_source_declaring_an_event_is_watched_on_the_checkout_it_is_shown_on() {
    let mut quiet = board(Some(Duration::from_secs(5)), Vec::new());
    quiet.focus(Some(&checkout("/work/a")));
    assert!(quiet.watches().is_empty(), "no refresh-on, nothing watched");

    let mut engine = board(None, vec![planning(), EventName::CheckoutChanged]);
    assert!(engine.watches().is_empty(), "shown nowhere yet");
    engine.focus(Some(&checkout("/work/a")));
    engine.open(VIEW, &checkout("/work/b"));
    let watched = engine.watches();
    assert_eq!(
        watched
            .iter()
            .map(|watch| (watch.checkout.clone(), watch.event.name()))
            .collect::<Vec<_>>(),
        [
            (PathBuf::from("/work/b"), EventName::CHECKOUT_CHANGED),
            (PathBuf::from("/work/b"), EventName::PATHS_CHANGED),
        ],
        "the view's checkout: the section shows the same source"
    );
}

/// What every question asked during one pass carried as `input.event`.
fn events_asked(
    engine: &mut DeclaredExtension,
    now: Instant,
    happened: &[CheckoutEvent],
) -> Vec<serde_json::Value> {
    engine.pass(&Pass {
        now,
        visible: &[SECTION.to_owned()],
        happened,
    });
    let host = Answering::with(answered(json!({ "count": 1 })));
    for job in engine.jobs() {
        let answer = (job.run)(&host);
        engine.absorb(answer);
    }
    host.asked
        .into_inner()
        .into_iter()
        .map(|question| question.input.get("event").cloned().unwrap_or_default())
        .collect()
}

#[test]
fn an_event_reaches_its_source_as_input_and_each_is_delivered_one_by_one() {
    let start = Instant::now();
    let mut latest = board(None, vec![planning()]);
    latest.focus(Some(&checkout("/work/a")));
    events_asked(&mut latest, start, &[]);
    events_asked(&mut latest, start, &[saved("/work/a"), saved("/work/a")]);
    let asked = events_asked(&mut latest, start + COALESCE, &[]);
    assert_eq!(asked.len(), 1, "a burst asks once: {asked:?}");
    assert_eq!(asked[0]["name"], "paths.changed");
    assert_eq!(asked[0]["paths"], json!([".planning"]));

    let mut each = board(None, vec![planning()]);
    each.compiled.manifest.sources[0].deliver = uze_contract::Deliver::Each;
    each.focus(Some(&checkout("/work/a")));
    events_asked(&mut each, start, &[]);
    let burst: Vec<CheckoutEvent> = (0..events::EACH_PER_WINDOW + 2)
        .map(|_| saved("/work/a"))
        .collect();
    events_asked(&mut each, start, &burst);
    let mut delivered = Vec::new();
    for step in 1..=events::EACH_PER_WINDOW as u32 + 1 {
        delivered.extend(events_asked(&mut each, start + COALESCE * step, &[]));
    }
    assert_eq!(
        delivered.len(),
        events::EACH_PER_WINDOW,
        "one run per event, bounded per window"
    );
    assert_eq!(
        delivered[0]["dropped"], 2,
        "the first says what was dropped"
    );
    assert!(delivered[1].get("dropped").is_none());
}

#[test]
fn an_events_source_answers_what_the_extension_saw_without_asking_the_host() {
    let start = Instant::now();
    let mut engine = board(None, vec![planning()]);
    engine.compiled.manifest.sources[0].kind = uze_contract::SourceKind::Events(2);
    engine.answer_event_sources();
    engine.focus(Some(&checkout("/work/a")));
    assert_eq!(
        engine.data(super::tests::SOURCE).cloned(),
        Some(Ok(json!([])))
    );
    let asked = events_asked(
        &mut engine,
        start,
        &[saved("/work/a"), saved("/work/b"), saved("/work/c")],
    );
    assert!(asked.is_empty(), "the host is never asked about it");
    let Some(Ok(seen)) = engine.data(super::tests::SOURCE).cloned() else {
        panic!("no data");
    };
    assert_eq!(
        seen.as_array().unwrap().len(),
        2,
        "the last two, newest first"
    );
    assert_eq!(seen[0]["checkout"], "/work/c");
}

fn happened(event: uze_contract::AgentEvent, agent: &str) -> crate::contract::AgentHappened {
    crate::contract::AgentHappened {
        event,
        agent: agent.to_owned(),
        harness: "harness".to_owned(),
        checkout: PathBuf::from("/work/b"),
        detail: json!({ "tool": "Bash" }),
    }
}

#[test]
fn an_agents_event_asks_its_source_on_any_checkout_and_sends_an_unattended_prompt() {
    use uze_contract::{AgentEvent, AgentTarget, Does, EventName, PromptText};
    let start = Instant::now();
    let mut engine = board(None, vec![EventName::Agent(AgentEvent::TurnFinished)]);
    engine
        .compiled
        .manifest
        .actions
        .push(uze_contract::ActionDecl {
            id: "nudge".to_owned(),
            label: "Nudge".to_owned(),
            on: VIEW.to_owned(),
            key: None,
            confirm: None,
            does: Does::AgentPrompt {
                prompt: PromptText::Text("Continue".to_owned()),
                target: AgentTarget::Own,
                auto: Some(uze_contract::AutoPrompt {
                    on: AgentEvent::TurnFinished,
                    values: Vec::new(),
                }),
            },
            with: Vec::new(),
            then: None,
        });
    engine.focus(Some(&checkout("/work/a")));
    events_asked(&mut engine, start, &[]);
    engine.agent_events(&[happened(AgentEvent::ToolStarted, "a1")], start);
    assert!(
        engine.requests().is_empty(),
        "only the event it waits for sends it"
    );
    engine.agent_events(&[happened(AgentEvent::TurnFinished, "a1")], start);
    let asked = events_asked(&mut engine, start + COALESCE, &[]);
    assert_eq!(asked.len(), 1, "an agent on another checkout still asks it");
    assert_eq!(asked[0]["name"], "agent.turn.finished");
    assert_eq!(asked[0]["agent"], "a1");
    assert_eq!(asked[0]["tool"], "Bash");
    let sent = engine.requests();
    let [Outcome::Agent(request)] = sent.as_slice() else {
        panic!("one unattended prompt: {sent:?}");
    };
    assert_eq!(
        request.agent.as_deref(),
        Some("a1"),
        "aimed at the agent that finished"
    );
    assert!(engine.requests().is_empty(), "taken once");
}

#[test]
fn an_agents_source_is_answered_by_the_workspace_and_never_asked() {
    let start = Instant::now();
    let mut engine = board(None, Vec::new());
    engine.compiled.manifest.sources[0].kind = uze_contract::SourceKind::Agents;
    engine.focus(Some(&checkout("/work/a")));
    assert!(events_asked(&mut engine, start, &[]).is_empty());
    engine.agents(&json!([{ "id": "a1", "state": "working" }]));
    assert_eq!(
        engine.data(super::tests::SOURCE).cloned(),
        Some(Ok(json!([{ "id": "a1", "state": "working" }])))
    );
}

#[test]
fn an_agents_source_hears_an_empty_list_and_says_why_it_is_told_nothing() {
    let mut engine = board(None, Vec::new());
    engine.compiled.manifest.sources[0].kind = uze_contract::SourceKind::Agents;
    engine.agents(&json!([]));
    assert_eq!(
        engine.data(super::tests::SOURCE).cloned(),
        Some(Ok(json!([]))),
        "no agent is an answer, not a wait"
    );

    engine.workspace_sources_withheld(Some("needs consent"));
    assert_eq!(engine.withheld(super::tests::SOURCE), Some("needs consent"));
    engine.agents(&json!([{ "id": "a1" }]));
    engine.workspace_sources_withheld(None);
    assert_eq!(engine.withheld(super::tests::SOURCE), None);
    engine.agents(&json!([{ "id": "a1" }]));
    assert_eq!(
        engine.data(super::tests::SOURCE).cloned(),
        Some(Ok(json!([{ "id": "a1" }]))),
        "consented, the agents it was handed meanwhile reach it"
    );
}

#[test]
fn a_source_slower_than_its_every_is_not_asked_again_until_it_answers() {
    let start = Instant::now();
    let mut engine = board(Some(Duration::from_secs(2)), Vec::new());
    engine.focus(Some(&checkout("/work/a")));
    let visible = [SECTION.to_owned()];
    let at = |engine: &mut DeclaredExtension, now: Instant| {
        engine.pass(&Pass {
            now,
            visible: &visible,
            happened: &[],
        });
        engine.jobs()
    };
    let host = Answering::with(answered(json!({ "count": 1 })));
    let first: Vec<Opaque> = at(&mut engine, start)
        .into_iter()
        .map(|job| (job.run)(&host))
        .collect();
    assert_eq!(first.len(), 1);
    assert!(
        at(&mut engine, start + Duration::from_secs(2)).is_empty(),
        "the first question is still out"
    );
    for answer in first {
        engine.absorb(answer);
    }
    assert_eq!(
        engine.data(super::tests::SOURCE),
        Some(&Ok(json!({ "count": 1 }))),
        "the answer that took three seconds is shown"
    );
    let asked: Vec<Opaque> = at(&mut engine, start + Duration::from_secs(3))
        .into_iter()
        .map(|job| (job.run)(&host))
        .collect();
    assert_eq!(asked.len(), 1, "asked again once the answer landed");
    assert_eq!(host.asked.borrow()[1].asking, Asking::Refresh);
}
