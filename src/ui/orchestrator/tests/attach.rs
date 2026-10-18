//! One attach against the server: reads that failed, a runtime that went
//! away, what is remembered across attaches, and a burst of input.

use super::WorkspaceHit;
use super::support::*;
use super::{Flow, GitBadge, WorkStateView, WorkspaceModel, answered_or};
use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::path::PathBuf;
use uze_core::UzeHome;
use uze_terminal::PaneId;

/// A background read whose work panicked still answers.
///
/// Otherwise there is no reservation left to release and nothing on
/// screen says so: the message used to go straight into a live
/// alternate screen, which ratatui repaints by difference — so it was
/// both corrupting and, a frame later, gone. The message is readable
/// now (`ui::run` restores the terminal before the hook prints), and
/// the client carries on with the answer the read would have given
/// had it found nothing.
///
/// The panic printed while this runs is the subject of the test, not
/// a failure in it.
#[test]
fn a_read_that_panicked_answers_what_it_would_have_answered_empty() {
    let silence: Option<GitBadge> = None;
    assert!(
        answered_or(|| panic!("syntect, over whatever the tree listed"), silence).is_none(),
        "a panicked read answers, so its key is released"
    );
}

/// The terminal server going away is noticed, said, and left.
///
/// The reader thread ends — dropping its sender — when the socket
/// stops answering, and a `while let Ok(..)` reads that as "nothing
/// arrived this tick". Every pane on screen is then a frozen image in
/// a client that still redraws, scrolls and accepts keys, with no
/// message and no way back but quitting.
#[test]
fn a_terminal_runtime_that_went_away_is_said_rather_than_waited_on() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-runtime-gone"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 3), &home);
    assert!(
        matches!(driven.pump(), Flow::Continue),
        "a live runtime is just a quiet one"
    );

    driven.runtime_gone();

    assert!(
        matches!(
            driven.pump(),
            Flow::Exit(super::WorkspaceExit::Disconnected)
        ),
        "the client leaves rather than spinning against a dead socket"
    );
    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(said.contains("disconnected"), "it says why it left: {said}");
}

/// A Ctrl+O round trip to management is a detach and a fresh attach.
/// What the client resolved on its own — the sidebar's tasks,
/// branches and a completion noticed while the user was elsewhere —
/// must come back with it, while the server's view of the session and
/// the presentation state of the attach that ended must not.
#[test]
fn memory_carries_what_the_client_resolved_across_attaches() {
    let mut model = agent_with_task(WorkStateView::Ready, 1);
    model
        .remembered
        .branches
        .insert(PathBuf::from("/repo"), "agent/ai".to_owned());
    model.remembered.completed_agent_panes.insert(PaneId(1));
    model.error = Some("stale".to_owned());
    model
        .hits
        .push((Rect::new(0, 0, 1, 1), WorkspaceHit::NewSpace));

    let model = WorkspaceModel {
        remembered: model.remembered,
        ..WorkspaceModel::default()
    };

    assert_eq!(model.remembered.tasks[&PathBuf::from("/repo")].len(), 1);
    assert_eq!(
        model
            .remembered
            .branches
            .get(&PathBuf::from("/repo"))
            .map(String::as_str),
        Some("agent/ai")
    );
    assert!(model.remembered.completed_agent_panes.contains(&PaneId(1)));
    assert!(model.session.is_none());
    assert!(model.error.is_none());
    assert!(model.hits.is_empty());
}

#[test]
fn a_burst_of_input_waits_for_the_frame_only_where_geometry_matters() {
    let paste = Event::Paste("a".to_owned());
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 4,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(super::burst_admits(1, &paste, true), super::Admit::Handle);
    assert_eq!(super::burst_admits(0, &click, true), super::Admit::Handle);
    assert_eq!(super::burst_admits(1, &click, false), super::Admit::Handle);
    assert_eq!(super::burst_admits(1, &click, true), super::Admit::Hold);
    assert_eq!(
        super::burst_admits(1, &Event::Resize(80, 24), false),
        super::Admit::HandleAndDraw
    );
}
