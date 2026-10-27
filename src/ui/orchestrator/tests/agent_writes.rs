//! An extension typing into an agent (`agent-bridge`, B5): shown first,
//! typed only once allowed, limited, and held by the pause key.

use super::support::*;
use super::{Owner, WorkspaceHit};
use uze_core::UzeHome;
use uze_extensions::contract::{AgentAsk, AgentRequest};
use uze_terminal::ClientRequest;

fn launched_agent() -> super::WorkspaceModel {
    let mut model = agent_session_in("/repo");
    let session = model.session.as_mut().unwrap();
    session.workspace.spaces[0].tabs[0].env = vec![(
        uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
        "a1".to_owned(),
    )];
    model.note_space_projects(&identities_fixture());
    let agents = super::bridged_agents(&model, &identities_fixture());
    model.see_agents(agents);
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
