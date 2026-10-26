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
