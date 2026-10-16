//! Continuing an agent on another harness: which harnesses its menu offers,
//! and the question a busy agent is asked before it is interrupted.
//!
//! The two harnesses are whatever the shared fixture's agents run, read off
//! their panes, so nothing here depends on which vendors those are.

use super::workspace_tests::{agents_in_the_root_session, driven, frame_rows};
use super::*;
use crossterm::event::{MouseButton, MouseEventKind};

fn identity(binary: &str, configured: bool) -> AgentIdentity {
    let binary: &'static str = Box::leak(binary.to_owned().into_boxed_str());
    AgentIdentity {
        binary,
        integration: binary,
        display_name: Box::leak(display_name(binary).into_boxed_str()),
        launch: std::path::PathBuf::from(binary),
        continuity_gap: None,
        configured,
    }
}

/// How the menu names a harness in these tests: its binary, capitalized.
fn display_name(binary: &str) -> String {
    let mut letters = binary.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default()
}

/// The harness the fixture's agent in `tab` runs.
fn harness_of(tab: TabId) -> String {
    agents_in_the_root_session()
        .tabs()
        .find(|candidate| candidate.id == tab)
        .unwrap()
        .pane
        .process
        .clone()
}

/// The fixture's two harnesses, and a third not set up for the workspace.
fn identities() -> Vec<AgentIdentity> {
    vec![
        identity(&harness_of(TabId(2)), true),
        identity(&harness_of(TabId(3)), true),
        identity("unset", false),
    ]
}

fn continue_in(tab: TabId) -> String {
    format!("Continue in {}", display_name(&harness_of(tab)))
}

/// The menu a right-click on `wanted`'s row opens, as its entries read.
fn menu_on(driven: &mut workspace_tests::Driven<'_>, wanted: TabId) -> Vec<String> {
    driven
        .attach
        .model
        .session
        .as_mut()
        .expect("session")
        .select_tab(wanted);
    driven.frame();
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::SelectTab(tab) if *tab == wanted => Some(*rect),
            _ => None,
        })
        .expect("the agent's row is a target");
    driven.mouse(row.x + 1, row.y, MouseEventKind::Down(MouseButton::Right));
    driven
        .attach
        .model
        .context_menu
        .as_ref()
        .expect("a menu opened on the agent's row")
        .labels
        .clone()
}

fn pane_of(model: &WorkspaceModel, tab: TabId) -> PaneId {
    model
        .session
        .as_ref()
        .unwrap()
        .workspace
        .spaces
        .iter()
        .flat_map(|space| space.tabs.iter())
        .find(|candidate| candidate.id == tab)
        .unwrap()
        .pane
        .id
}

#[test]
fn an_agent_is_offered_every_set_up_harness_but_its_own() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-offers"));
    let mut driven = driven(agents_in_the_root_session(), &home);
    driven.attach.identities = identities();

    assert_eq!(
        menu_on(&mut driven, TabId(2)),
        [
            "Rename".to_owned(),
            continue_in(TabId(3)),
            "To worktree".to_owned(),
            "Close tab".to_owned()
        ],
        "the other harness, never its own nor one not set up"
    );
    driven.attach.model.context_menu = None;
    assert_eq!(
        menu_on(&mut driven, TabId(3))[1],
        continue_in(TabId(2)),
        "and the other agent the other way round"
    );
}

#[test]
fn an_agent_whose_harness_exited_offers_no_switch() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-exited"));
    let mut model = agents_in_the_root_session();
    let pane = pane_of(&model, TabId(2));
    model
        .session
        .as_mut()
        .unwrap()
        .update_pane_status(pane, "/repo".into(), "zsh".into());
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();

    let labels = menu_on(&mut driven, TabId(2));
    assert!(
        !labels.iter().any(|label| label.starts_with("Continue in")),
        "{labels:?}"
    );
}

#[test]
fn a_busy_agent_is_asked_before_it_is_interrupted() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-busy"));
    let mut model = agents_in_the_root_session();
    let pane = pane_of(&model, TabId(2));
    model.note_agent_prompt_submission(pane, &identities(), Some("keep going"));
    assert_eq!(model.agent_tab_status(pane, false), AgentTabStatus::Working);
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    menu_on(&mut driven, TabId(2));
    let menu = driven.attach.model.context_menu.take().unwrap();
    let (action, choice) = menu.entry(1).unwrap();

    driven
        .attach
        .perform_menu_entry(menu.target, action, choice);

    let question = driven
        .attach
        .model
        .interrupt
        .as_ref()
        .expect("the switch asks first, over the whole client");
    assert_eq!(question.choice.integration, harness_of(TabId(3)));
    assert!(!question.on_confirm, "the keyboard starts on the way out");
    assert!(
        !driven.attach.model.placement_pending,
        "nothing is placed until it is answered"
    );

    driven.attach.interrupt_action(uze_keys::Action::Dismiss);
    assert!(driven.attach.model.interrupt.is_none());
    assert!(
        !driven.attach.model.placement_pending,
        "declined, the agent keeps running"
    );
}

#[test]
fn a_confirmed_interruption_continues_the_agent() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-busy-yes"));
    let mut model = agents_in_the_root_session();
    let pane = pane_of(&model, TabId(2));
    model.note_agent_prompt_submission(pane, &identities(), Some("keep going"));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    menu_on(&mut driven, TabId(2));
    let menu = driven.attach.model.context_menu.take().unwrap();
    let (action, choice) = menu.entry(1).unwrap();
    driven
        .attach
        .perform_menu_entry(menu.target, action, choice);

    driven.attach.interrupt_action(uze_keys::Action::FocusNext);
    driven.attach.interrupt_action(uze_keys::Action::Activate);

    assert!(driven.attach.model.interrupt.is_none());
    assert!(
        driven.attach.model.placement_pending,
        "the switch was placed"
    );
}

#[test]
fn an_idle_agent_is_switched_without_a_question() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-idle"));
    let mut driven = driven(agents_in_the_root_session(), &home);
    driven.attach.identities = identities();
    menu_on(&mut driven, TabId(2));
    let menu = driven.attach.model.context_menu.take().unwrap();
    let (action, choice) = menu.entry(1).unwrap();

    driven
        .attach
        .perform_menu_entry(menu.target, action, choice);

    assert!(driven.attach.model.context_menu.is_none());
    assert!(
        driven.attach.model.placement_pending,
        "the switch was placed"
    );
}

#[test]
fn the_interruption_is_asked_over_the_whole_client() {
    let mut model = agents_in_the_root_session();
    model.interrupt = Some(InterruptDialog {
        target: MenuTarget::Tab(TabId(2)),
        label: "agent 1".into(),
        choice: HarnessChoice {
            integration: "other".into(),
            display_name: "Other".into(),
            continuity_gap: None,
        },
        on_confirm: false,
    });
    let rows = frame_rows(&mut model).join("\n");
    assert!(rows.contains("Interrupt agent 1?"), "{rows}");
    assert!(rows.contains("Continue in Other"), "{rows}");
}
