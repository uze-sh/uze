//! Continuing an agent on another harness: which harnesses its menu offers,
//! and the question a busy agent is asked before it is interrupted.
//!
//! The two harnesses are whatever the shared fixture's agents run, read off
//! their panes, so nothing here depends on which vendors those are.

use super::workspace_tests::{agents_in_the_root_session, driven, frame_rows_with};
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
    let anchor = driven.attach.model.context_menu.as_ref().unwrap().anchor;
    let menu = driven.attach.model.context_menu.take().unwrap();
    let (action, choice) = menu.entry(1).unwrap();

    driven
        .attach
        .perform_menu_entry(menu.target, action, choice, anchor);

    let question = driven
        .attach
        .model
        .context_menu
        .as_ref()
        .expect("the switch asks first");
    assert_eq!(
        question.labels,
        [format!(
            "Interrupt it, continue in {}",
            display_name(&harness_of(TabId(3)))
        )]
    );
    assert!(
        !driven.attach.model.placement_pending,
        "nothing is placed until it is answered"
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
        .perform_menu_entry(menu.target, action, choice, menu.anchor);

    assert!(driven.attach.model.context_menu.is_none());
    assert!(
        driven.attach.model.placement_pending,
        "the switch was placed"
    );
}

fn limited(mut model: WorkspaceModel, agent: &str) -> WorkspaceModel {
    model.remembered.usage_limits.insert(
        agent.to_owned(),
        uze_application::UsageLimit {
            resets_at_unix: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 3 * 3_600
                    + 30,
            ),
        },
    );
    model
}

#[test]
fn an_agent_at_its_limit_lists_continuing_elsewhere_first() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-switch-limited"));
    let mut driven = driven(limited(agents_in_the_root_session(), "a1"), &home);
    driven.attach.identities = identities();

    let labels = menu_on(&mut driven, TabId(2));
    assert_eq!(labels[0], continue_in(TabId(3)), "{labels:?}");
    assert_eq!(labels[1], "Rename");
}

#[test]
fn an_agent_at_its_limit_says_so_and_when_it_resets() {
    let mut model = limited(agents_in_the_root_session(), "a1");
    let rows = frame_rows_with(&mut model, &identities());
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("{} · limit 3h", harness_of(TabId(2))))),
        "{rows:#?}"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.contains(&format!("{} · limit", harness_of(TabId(3))))),
        "only the agent that stopped at it"
    );
}
