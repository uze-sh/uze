//! An extension typing into an agent (`extension-agent-bridge`, B5): shown first,
//! typed only once allowed, limited, and held by the pause key.

use super::support::*;
use super::{Owner, WorkspaceHit};
use uze_core::UzeHome;
use uze_extensions::contract::{AgentAsk, AgentRequest};
use uze_terminal::ClientRequest;

fn launched_agent() -> super::WorkspaceModel {
    let mut model = agent_session_in("/repo");
    launch_agent_a1(&mut model);
    model
}

fn prompt(text: &str) -> AgentRequest {
    AgentRequest {
        what: AgentAsk::Prompt {
            text: text.to_owned(),
        },
        agent: None,
        label: "Review".to_owned(),
        unattended: false,
    }
}

fn typed(sent: &[ClientRequest]) -> Vec<String> {
    sent.iter()
        .filter_map(|request| match request {
            ClientRequest::Input { bytes, .. } => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_prompt_is_shown_then_typed_once_allowed_and_a_second_in_the_turn_is_dropped() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-writes"));
    let mut driven = driven(launched_agent(), &home);
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("Review PLAN.md"));
    assert!(driven.attach.model.agent_write.is_some(), "asked first");
    driven.pump();
    assert!(
        typed(&driven.sent()).is_empty(),
        "nothing typed before the answer"
    );

    driven.frame();
    let allow = driven.hit(|hit| *hit == WorkspaceHit::AgentWriteAnswer(true));
    driven.press(allow.x, allow.y);
    assert!(driven.attach.model.agent_write.is_none());
    driven.pump();
    let sent = typed(&driven.sent());
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains("Review PLAN.md"), "{sent:?}");

    // The agent is mid-turn now: the next is refused, never held.
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("And again"));
    driven.attach.model.answer_agent_write(true);
    driven.pump();
    assert!(typed(&driven.sent()).is_empty(), "dropped, not queued");
    driven.pump();
    assert!(
        typed(&driven.sent()).is_empty(),
        "and never delivered later"
    );
}

#[test]
fn a_paused_workspace_takes_no_prompt_and_a_command_character_is_refused() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-pause"));
    let mut driven = driven(launched_agent(), &home);
    driven.attach.model.toggle_agent_writes();
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("Review"));
    assert!(driven.attach.model.agent_write.is_none(), "not even asked");
    driven.attach.model.toggle_agent_writes();
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("/clear"));
    assert!(
        driven.attach.model.agent_write.is_none(),
        "a harness's command is refused before it is shown"
    );
    driven.pump();
    assert!(typed(&driven.sent()).is_empty());
}

#[test]
fn f8_pauses_with_a_surface_open_in_the_pane() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-agent-pause-surface",
    ));
    let mut driven = driven(launched_agent(), &home);
    driven.press_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::ALT));
    assert!(
        driven.attach.model.open_slot.is_some(),
        "the changes stand in the pane"
    );
    driven.press_key(KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE));
    assert!(
        driven.attach.model.remembered.declared.bridge.writes.paused,
        "the surface in front does not swallow the brake"
    );
}

fn key(text: &str) -> crossterm::event::KeyEvent {
    key_event(uze_keys::Chord::parse(text).unwrap())
}

/// Asks to type `text`, then answers the question by keyboard alone:
/// the arrow onto the allowing answer, then Enter.
fn allowed_by_keyboard(driven: &mut Driven<'_>, text: &str) -> Vec<String> {
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt(text));
    assert!(driven.attach.model.agent_write.is_some(), "asked first");
    driven.press_key(key("right"));
    driven.press_key(key("enter"));
    assert!(
        driven.attach.model.agent_write.is_none(),
        "Enter answers the question"
    );
    driven.pump();
    typed(&driven.sent())
}

#[test]
fn the_question_is_answered_by_keyboard_over_the_bare_pane() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-keys-pane"));
    let mut driven = driven(launched_agent(), &home);
    let sent = allowed_by_keyboard(&mut driven, "Review PLAN.md");
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains("Review PLAN.md"), "{sent:?}");
}

#[test]
fn esc_declines_the_question_over_the_bare_pane_and_reaches_nothing_behind_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-esc-pane"));
    let mut driven = driven(launched_agent(), &home);
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("Review"));
    driven.press_key(key("tab"));
    driven.press_key(key("esc"));
    assert!(driven.attach.model.agent_write.is_none(), "Esc declines");
    driven.pump();
    assert!(typed(&driven.sent()).is_empty(), "nothing reached the pane");
}

#[test]
fn the_question_is_answered_by_keyboard_over_the_code_surface() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-keys-code"));
    let mut driven = driven(launched_agent(), &home);
    driven.press_key(key("alt+g"));
    assert!(
        driven.attach.model.open_slot.is_some(),
        "the changes in front"
    );
    let sent = allowed_by_keyboard(&mut driven, "Review PLAN.md");
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        driven.attach.model.open_slot.is_some(),
        "the surface stays where it was"
    );
}
