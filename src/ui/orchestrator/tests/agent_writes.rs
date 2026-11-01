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
        action: "review".to_owned(),
        label: "Review".to_owned(),
        unattended: false,
        summary: None,
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
    driven.frame();
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
    driven.frame();
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

/// An unattended prompt of the extension's action `action`.
fn on_its_own(action: &str, text: &str) -> AgentRequest {
    AgentRequest {
        what: AgentAsk::Prompt {
            text: text.to_owned(),
        },
        agent: Some("a1".to_owned()),
        action: action.to_owned(),
        label: action.to_owned(),
        unattended: true,
        summary: None,
    }
}

#[test]
fn allowing_one_unattended_prompt_allows_that_action_alone() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-unattended"));
    let mut driven = driven(launched_agent(), &home);
    let probe = || Owner::Builtin("probe");
    driven
        .attach
        .model
        .ask_of_agent(probe(), on_its_own("review", "Review"));
    assert!(
        driven.attach.model.agent_write.is_some(),
        "the first is asked"
    );
    driven.frame();
    driven.attach.model.answer_agent_write(true);
    assert!(driven.attach.model.agent_write.is_none());

    driven
        .attach
        .model
        .ask_of_agent(probe(), on_its_own("deploy", "Ship it to production"));
    assert!(
        driven.attach.model.agent_write.is_some(),
        "another action of the same extension still asks"
    );
    let writes = &driven.attach.model.remembered.declared.bridge.writes;
    assert_eq!(
        writes.allowed.len(),
        1,
        "only the allowed one waits to be typed"
    );
    driven.attach.model.answer_agent_write(false);

    driven
        .attach
        .model
        .ask_of_agent(probe(), on_its_own("review", "Review"));
    assert!(
        driven.attach.model.agent_write.is_none(),
        "the allowed action is not asked again"
    );
    let writes = &driven.attach.model.remembered.declared.bridge.writes;
    assert_eq!(writes.allowed.len(), 2);
}

/// `words` words of prose: a prompt of about six bytes a word.
fn prose(words: usize) -> String {
    (0..words)
        .map(|index| format!("w{index:04}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_prompt_over_the_bound_is_refused_before_it_is_shown() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-long"));
    let mut driven = driven(launched_agent(), &home);
    let long = prose(400);
    assert!(long.len() > 2048);
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt(&long));
    assert!(driven.attach.model.agent_write.is_none(), "refused unseen");
}

#[test]
fn a_prompt_taller_than_the_frame_cannot_be_allowed_until_it_is_read_whole() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-tall"));
    let mut driven = driven(launched_agent(), &home);
    let tall = prose(200);
    assert!(tall.len() <= 2048);
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt(&tall));
    driven.frame();
    assert!(
        !driven
            .attach
            .model
            .hits
            .iter()
            .any(|(_, hit)| *hit == WorkspaceHit::AgentWriteAnswer(true)),
        "no way to allow what the frame cannot show"
    );
    driven.press_key(key("right"));
    driven.press_key(key("enter"));
    driven.attach.model.answer_agent_write(true);
    driven.pump();
    assert!(typed(&driven.sent()).is_empty(), "nothing typed unread");
    assert!(
        driven.attach.model.agent_write.is_some(),
        "the question waits to be read"
    );

    // Room enough: every word is on screen, and only then may it go.
    driven.area = ratatui::layout::Rect::new(0, 0, 120, 60);
    driven.frame();
    let shown = frame_rows_at(&mut driven.attach.model, driven.area).join(" ");
    for word in tall.split_whitespace() {
        assert!(shown.contains(word), "{word} is drawn");
    }
    let allow = driven.hit(|hit| *hit == WorkspaceHit::AgentWriteAnswer(true));
    driven.press(allow.x, allow.y);
    driven.pump();
    assert_eq!(typed(&driven.sent()).len(), 1);
}

fn skill(name: &str) -> AgentRequest {
    AgentRequest {
        what: AgentAsk::Skill {
            skill: name.to_owned(),
            args: vec!["12".to_owned()],
        },
        agent: None,
        action: "run".to_owned(),
        label: "Run".to_owned(),
        unattended: false,
        summary: None,
    }
}

#[test]
fn a_skill_that_is_not_namespaced_is_refused_before_it_is_shown() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-skill"));
    let mut driven = driven(launched_agent(), &home);
    for builtin in ["clear", "compact", "gsd:run phase", "a:b:c", ":clear"] {
        driven
            .attach
            .model
            .ask_of_agent(Owner::Builtin("probe"), skill(builtin));
        assert!(
            driven.attach.model.agent_write.is_none(),
            "`{builtin}` is not a skill"
        );
    }
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), skill("gsd:execute-phase"));
    assert!(
        driven.attach.model.agent_write.is_some(),
        "a plugin's skill is asked"
    );
}

#[test]
fn an_allowed_prompt_whose_agent_is_gone_is_said_not_dropped_silently() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-gone"));
    let mut driven = driven(launched_agent(), &home);
    driven
        .attach
        .model
        .ask_of_agent(Owner::Builtin("probe"), prompt("Review PLAN.md"));
    driven.frame();
    driven.attach.model.answer_agent_write(true);
    // Its tab is a shell again: the agent exited before it was typed.
    first_tab_mut(&mut driven.attach.model).env.clear();
    driven.pump();
    assert!(typed(&driven.sent()).is_empty());
    let toasts = &driven.attach.model.remembered.toasts;
    assert!(
        toasts.iter().any(|toast| toast.text == "not sent"
            && toast.detail.contains("probe")
            && toast.detail.contains("Review")
            && toast.detail.contains("no longer open")),
        "said, attributed: {:?}",
        toasts
            .iter()
            .map(|toast| (&toast.text, &toast.detail))
            .collect::<Vec<_>>()
    );
}

/// An extension that only keeps what it was told about its writes.
struct Listener(
    std::sync::Arc<std::sync::Mutex<Vec<(String, uze_extensions::contract::AgentWritten)>>>,
);

impl uze_extensions::contract::Extension for Listener {
    fn identity(&self) -> String {
        "listener".to_owned()
    }
    fn surfaces(&self) -> Vec<uze_extensions::contract::SurfaceInfo> {
        Vec::new()
    }
    fn view(&self, _: &str, _: uze_extensions::view::Size) -> Option<uze_extensions::view::View> {
        None
    }
    fn section(&self, _: &str, _: bool) -> Option<uze_extensions::view::Section> {
        None
    }
    fn command(
        &mut self,
        _: &str,
        _: uze_extensions::view::Command,
        _: uze_extensions::view::Size,
    ) -> uze_extensions::contract::Outcome {
        uze_extensions::contract::Outcome::Stay
    }
    fn hit(
        &mut self,
        _: &str,
        _: uze_extensions::view::ViewHit,
        _: uze_extensions::view::Size,
    ) -> uze_extensions::contract::Outcome {
        uze_extensions::contract::Outcome::Stay
    }
    fn scroll(
        &mut self,
        _: &str,
        _: uze_extensions::view::ScrollTarget,
        _: uze_extensions::view::ScrollDirection,
        _: uze_extensions::view::Size,
    ) {
    }
    fn agent_write(&mut self, action: &str, written: uze_extensions::contract::AgentWritten) {
        self.0.lock().unwrap().push((action.to_owned(), written));
    }
}

impl crate::ui::orchestrator::declared::Hosted for Listener {}

#[test]
fn the_asking_extension_is_told_whether_its_write_was_typed_refused_or_declined() {
    use uze_extensions::contract::AgentWritten;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-told"));
    let mut driven = driven(launched_agent(), &home);
    let told = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let owner = Owner::Builtin("listener");
    driven
        .attach
        .model
        .remembered
        .declared
        .engines
        .insert(owner.clone(), Box::new(Listener(told.clone())));

    driven
        .attach
        .model
        .ask_of_agent(owner.clone(), prompt("first"));
    driven.attach.model.answer_agent_write(false);
    assert_eq!(
        told.lock().unwrap().pop(),
        Some(("review".to_owned(), AgentWritten::Declined))
    );

    driven
        .attach
        .model
        .ask_of_agent(owner.clone(), prompt("/clear"));
    assert!(matches!(
        told.lock().unwrap().pop(),
        Some((_, AgentWritten::Refused(_)))
    ));

    driven
        .attach
        .model
        .ask_of_agent(owner.clone(), prompt("Review PLAN.md"));
    driven.frame();
    driven.attach.model.answer_agent_write(true);
    driven.pump();
    assert_eq!(
        told.lock().unwrap().pop(),
        Some(("review".to_owned(), AgentWritten::Typed))
    );
}

/// A bundled extension's send is asked in the operator's words: what the
/// agent is told and where its answers come back, never the commands the
/// line names, which are the agent's. A plugin's summary is not taken: what
/// a plugin types is shown whole.
#[test]
fn a_bundled_extensions_ask_is_said_in_the_operators_words_and_a_plugins_whole() {
    let summarised = AgentRequest {
        summary: Some("The agent is told your comment waits for it.".to_owned()),
        ..prompt("run `uze agent review show` to read it")
    };
    let mut model = launched_agent();
    model.ask_of_agent(Owner::Builtin("review"), summarised.clone());
    let rows = frame_rows(&mut model).join("\n");
    assert!(rows.contains("your comment waits"), "{rows}");
    assert!(
        !rows.contains("uze agent"),
        "the agent's commands are not the operator's"
    );
}
