//! Continuing an agent on another harness: which harnesses its menu offers,
//! and the question a busy agent is asked before it is interrupted.
//!
//! The two harnesses are whatever the shared fixture's agents run, read off
//! their panes, so nothing here depends on which vendors those are.

use super::support::{agents_in_the_root_session, driven, frame_rows};
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

/// What the menu's group entry reads as.
const CONTINUE_IN: &str = "Continue in…";

/// The harnesses the open menu's group entry offers, as they read.
fn offered(driven: &support::Driven<'_>) -> Vec<String> {
    let menu = driven
        .attach
        .model
        .context_menu
        .as_ref()
        .expect("a menu is open");
    let index = menu
        .labels
        .iter()
        .position(|label| label == CONTINUE_IN)
        .expect("the menu offers continuing elsewhere");
    menu.group(index)
        .unwrap()
        .iter()
        .map(|choice| choice.display_name.clone())
        .collect()
}

/// One key, by the chord the active keymap gives `action` in `scope`.
fn press(driven: &mut support::Driven<'_>, action: uze_keys::Action, scope: uze_keys::Scope) {
    let chord = uze_keys::active()
        .chord_for(
            action,
            &[uze_keys::Scope::Global, uze_keys::Scope::Workspace, scope],
        )
        .expect("the action has a key");
    driven.press_key(support::key_event(chord));
}

/// Opens the selected agent's menu from the keyboard and takes its
/// "Continue in…" group to the first harness.
fn continue_from_the_keyboard(driven: &mut support::Driven<'_>) {
    driven.frame();
    press(driven, uze_keys::Action::OpenMenu, uze_keys::Scope::Pane);
    assert!(
        driven.attach.model.context_menu.is_some(),
        "Alt+. opened the menu"
    );
    press(
        driven,
        uze_keys::Action::SelectNext,
        uze_keys::Scope::ContextMenu,
    );
    press(
        driven,
        uze_keys::Action::FocusNext,
        uze_keys::Scope::ContextMenu,
    );
    assert_eq!(
        driven.attach.model.context_menu.as_ref().unwrap().submenu,
        Some(0),
        "Right opened the group"
    );
    press(
        driven,
        uze_keys::Action::Activate,
        uze_keys::Scope::ContextMenu,
    );
}

/// The menu a right-click on `wanted`'s row opens, as its entries read.
fn menu_on(driven: &mut support::Driven<'_>, wanted: TabId) -> Vec<String> {
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
        ["Rename", CONTINUE_IN, "To worktree", "Close tab"],
        "one entry, whatever the number of harnesses"
    );
    assert_eq!(
        offered(&driven),
        [display_name(&harness_of(TabId(3)))],
        "the other harness, never its own nor one not set up"
    );
    driven.attach.model.context_menu = None;
    menu_on(&mut driven, TabId(3));
    assert_eq!(offered(&driven), [display_name(&harness_of(TabId(2)))]);
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
        !labels.iter().any(|label| label == CONTINUE_IN),
        "{labels:?}"
    );
}

#[test]
fn the_group_opens_beside_its_row_and_closes_back_to_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-group"));
    let mut driven = driven(agents_in_the_root_session(), &home);
    driven.attach.identities = identities();
    menu_on(&mut driven, TabId(2));
    press(
        &mut driven,
        uze_keys::Action::SelectNext,
        uze_keys::Scope::ContextMenu,
    );
    press(
        &mut driven,
        uze_keys::Action::Activate,
        uze_keys::Scope::ContextMenu,
    );
    assert_eq!(
        driven.attach.model.context_menu.as_ref().unwrap().submenu,
        Some(0),
        "Enter on a group opens it rather than acting"
    );
    driven.frame();
    assert!(
        driven
            .attach
            .model
            .hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::ContextSubmenuAction(0))),
        "the group is drawn"
    );

    press(
        &mut driven,
        uze_keys::Action::FocusPrevious,
        uze_keys::Scope::ContextMenu,
    );
    let menu = driven
        .attach
        .model
        .context_menu
        .as_ref()
        .expect("Left keeps the menu");
    assert_eq!(menu.submenu, None);
    assert_eq!(menu.selected, 1);
    press(
        &mut driven,
        uze_keys::Action::FocusNext,
        uze_keys::Scope::ContextMenu,
    );
    press(
        &mut driven,
        uze_keys::Action::Dismiss,
        uze_keys::Scope::ContextMenu,
    );
    assert!(
        driven.attach.model.context_menu.is_some(),
        "Esc in a group goes back a level first"
    );
}

#[test]
fn an_idle_agent_is_switched_from_the_keyboard_without_a_question() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-idle"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();

    continue_from_the_keyboard(&mut driven);

    assert!(driven.attach.model.context_menu.is_none());
    assert!(driven.attach.model.interrupt.is_none());
    assert!(
        driven.attach.model.placement_pending,
        "the switch was placed"
    );
}

#[test]
fn a_busy_agent_is_asked_before_it_is_interrupted() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-busy"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let pane = pane_of(&model, TabId(2));
    model.note_agent_prompt_submission(pane, &identities(), Some("keep going"));
    assert_eq!(model.agent_tab_status(pane, false), AgentTabStatus::Working);
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();

    continue_from_the_keyboard(&mut driven);

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
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let pane = pane_of(&model, TabId(2));
    model.note_agent_prompt_submission(pane, &identities(), Some("keep going"));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    continue_from_the_keyboard(&mut driven);

    driven.attach.interrupt_action(uze_keys::Action::FocusNext);
    driven.attach.interrupt_action(uze_keys::Action::Activate);

    assert!(driven.attach.model.interrupt.is_none());
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

#[test]
fn a_click_opens_the_group_and_a_click_on_a_harness_switches() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-click"));
    let mut driven = driven(agents_in_the_root_session(), &home);
    driven.attach.identities = identities();
    menu_on(&mut driven, TabId(2));
    driven.frame();
    let rect_of = |driven: &support::Driven<'_>, wanted: WorkspaceHit| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == wanted).then_some(*rect))
            .expect("drawn")
    };

    let group = rect_of(&driven, WorkspaceHit::ContextMenuAction(1));
    driven.press(group.x + 1, group.y);
    assert_eq!(
        driven
            .attach
            .model
            .context_menu
            .as_ref()
            .map(|menu| menu.submenu),
        Some(Some(0)),
        "the group's row opens it and keeps the menu"
    );
    driven.frame();
    let harness = rect_of(&driven, WorkspaceHit::ContextSubmenuAction(0));
    driven.press(harness.x + 1, harness.y);

    assert!(driven.attach.model.context_menu.is_none());
    assert!(
        driven.attach.model.placement_pending,
        "the switch was placed"
    );
}
