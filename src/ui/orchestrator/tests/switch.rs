//! Restarting an agent where it stands: the menu's `Restart in` panel, the
//! harnesses and overrides it offers, what its Enter does, and the question
//! a busy agent is asked before it is interrupted.
//!
//! The two harnesses are whatever the shared fixture's agents run, read off
//! their panes, so nothing here depends on which vendors those are.

use super::support::{agents_in_the_root_session, driven, frame_buffer_at, frame_rows};
use super::*;
use crossterm::event::{MouseButton, MouseEventKind};

fn identity(binary: &str, configured: bool) -> AgentIdentity {
    let binary: &'static str = Box::leak(binary.to_owned().into_boxed_str());
    AgentIdentity {
        binary,
        integration: binary,
        display_name: Box::leak(display_name(binary).into_boxed_str()),
        launch: std::path::PathBuf::from(binary),
        through_launcher: false,
        continuity_gap: None,
        configured,
        skill_prefix: Some("/"),
        overrides: Default::default(),
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

/// What the menu's restart entry reads as.
const RESTART_IN: &str = "Restart in";

/// The open `Restart in` panel.
fn panel<'a>(driven: &'a support::Driven<'_>) -> &'a DirectoryStep {
    driven
        .attach
        .model
        .directory_step
        .as_ref()
        .filter(|step| step.restart.is_some())
        .expect("the restart panel is open")
}

/// The harnesses the panel's field walks, as they read, its own first.
fn offered(driven: &support::Driven<'_>) -> Vec<String> {
    panel(driven)
        .harnesses
        .iter()
        .map(|option| option.display_name.clone())
        .collect()
}

/// What the agent carries on `harness`, as the panel's read answers it.
fn read_back(driven: &mut support::Driven<'_>, harness: &str, words: &[&str]) {
    let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    driven
        .attach
        .model
        .directory_step
        .as_mut()
        .expect("the panel is open")
        .seed_recorded(harness, &words);
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

fn type_text(driven: &mut support::Driven<'_>, text: &str) {
    for character in text.chars() {
        driven.press_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(character),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
}

/// Opens the selected agent's menu from the keyboard and its `Restart in`
/// panel with Right, the agent's overrides read back as none.
fn open_the_panel_from_the_keyboard(driven: &mut support::Driven<'_>, tab: TabId) {
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
    panel(driven);
    read_back(driven, &harness_of(tab), &[]);
}

/// Where `wanted` was drawn in the last frame.
fn rect_of(driven: &support::Driven<'_>, wanted: WorkspaceHit) -> Rect {
    driven
        .attach
        .model
        .hits
        .iter()
        .rev()
        .find_map(|(rect, hit)| (*hit == wanted).then_some(*rect))
        .unwrap_or_else(|| panic!("{wanted:?} is drawn"))
}

/// Enter in the panel.
fn enter(driven: &mut support::Driven<'_>) {
    press(
        driven,
        uze_keys::Action::Activate,
        uze_keys::Scope::LaunchOverrides,
    );
}

/// The busy notice the header shows.
fn notice(driven: &support::Driven<'_>) -> Option<String> {
    driven
        .attach
        .model
        .notice_chip()
        .map(|chip| chip.text.to_string())
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
fn an_agent_offers_one_restart_entry_opening_a_panel() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-entry"));
    let mut driven = driven(agents_in_the_root_session(), &home);
    driven.attach.identities = identities();

    assert_eq!(
        menu_on(&mut driven, TabId(2)),
        ["Rename", RESTART_IN, "To worktree", "Close tab"],
        "one entry, whatever the number of harnesses"
    );
    let menu = driven.attach.model.context_menu.as_ref().unwrap();
    assert!(menu.opens_panel(1), "it opens the panel, not a submenu");
}

#[test]
fn the_panel_offers_the_agents_own_harness_first_and_every_other_set_up() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-offers"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(3));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();

    open_the_panel_from_the_keyboard(&mut driven, TabId(3));

    assert_eq!(
        offered(&driven),
        [
            display_name(&harness_of(TabId(3))),
            display_name(&harness_of(TabId(2)))
        ],
        "its own first, never one not set up"
    );
    assert!(
        driven.attach.model.context_menu.is_some(),
        "the menu stays behind the panel"
    );
}

#[test]
fn an_agent_whose_harness_exited_offers_no_restart() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-exited"));
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
        !labels.iter().any(|label| label == RESTART_IN),
        "{labels:?}"
    );
}

#[test]
fn the_panel_stands_beside_its_entry_and_closes_back_to_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-beside"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.attach.identities = identities();
    open_the_panel_from_the_keyboard(&mut driven, TabId(2));
    driven.frame();
    let entry = rect_of(&driven, WorkspaceHit::ContextMenuAction(1));
    let card = rect_of(&driven, WorkspaceHit::NewAgentBody);
    assert!(
        card.x > entry.right(),
        "beside the entry: {card:?} {entry:?}"
    );

    press(
        &mut driven,
        uze_keys::Action::Dismiss,
        uze_keys::Scope::LaunchOverrides,
    );

    assert!(driven.attach.model.directory_step.is_none());
    let menu = driven
        .attach
        .model
        .context_menu
        .as_ref()
        .expect("Esc keeps the menu");
    assert_eq!(menu.selected, 1, "on the entry that opened the panel");
}

/// Until the agent's set is read there is nothing to relaunch with: Enter
/// is named `close`, and closes, relaunching nothing.
#[test]
fn enter_before_the_agents_set_is_read_closes_and_relaunches_nothing() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-pending"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_unread(&mut driven);
    type_text(&mut driven, "DEBUG=1");
    enter(&mut driven);
    assert_eq!(panel(&driven).enter_word(), "close");

    enter(&mut driven);

    assert!(driven.attach.model.directory_step.is_none());
    assert!(driven.attach.model.context_menu.is_none());
    assert!(!driven.attach.model.placement_pending);
}

#[test]
fn enter_with_nothing_changed_closes_and_relaunches_nothing() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-unchanged"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_from_the_keyboard(&mut driven, TabId(2));
    read_back(&mut driven, &harness_of(TabId(2)), &[]);
    assert_eq!(panel(&driven).enter_word(), "close");

    enter(&mut driven);

    assert!(driven.attach.model.directory_step.is_none());
    assert!(driven.attach.model.context_menu.is_none());
    assert!(!driven.attach.model.placement_pending);
}

/// Opens the selected agent's `Restart in` panel from the keyboard,
/// leaving its read unanswered.
fn open_the_panel_unread(driven: &mut support::Driven<'_>) {
    driven.frame();
    press(driven, uze_keys::Action::OpenMenu, uze_keys::Scope::Pane);
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
    panel(driven);
}

/// Answers the panel's question as asked for `restarting`, with
/// `recorded` as that agent's set. The panel's own read, which the scratch
/// home answers with nothing, is waited out and dropped first, so the
/// answer absorbed is this one whatever the threads' timing.
fn answer_the_panel(
    driven: &mut support::Driven<'_>,
    restarting: Option<String>,
    recorded: &[&str],
) {
    let channel = &driven.attach.channels.recent_overrides;
    channel
        .receiver
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the panel's own read answers");
    let (harness, root) = panel(driven).recent_asked.clone().expect("the panel asked");
    channel
        .sender
        .send(RecentOverridesRead {
            harness,
            root,
            restarting,
            entries: Vec::new(),
            recorded: Some(recorded.iter().map(|word| (*word).to_owned()).collect()),
        })
        .unwrap();
    driven.attach.absorb_recent_overrides();
}

/// The agent the open panel restarts.
fn restarted(driven: &support::Driven<'_>) -> String {
    panel(driven).restart.as_ref().unwrap().agent.clone()
}

#[test]
fn the_panel_opens_on_what_the_agent_carries_checked() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-carried"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_unread(&mut driven);

    let agent = restarted(&driven);
    answer_the_panel(&mut driven, Some(agent), &["--verbose", "DEBUG=1"]);

    assert_eq!(panel(&driven).launch_words(), ["--verbose", "DEBUG=1"]);
    assert_eq!(panel(&driven).enter_word(), "close");
    let rows = frame_rows(&mut driven.attach.model).join("\n");
    assert!(rows.contains("DEBUG=1"), "{rows}");
}

/// Two agents in one project ask the same harness from the same directory:
/// an answer read for another agent, or for the new agent's popup, is not
/// this panel's, however it matches the rest of the question.
/// What the operator added before the agent's set was read stays, after
/// it: the set is taken in under it, not over it.
#[test]
fn entries_added_before_the_set_is_read_are_kept() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-early"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_unread(&mut driven);
    type_text(&mut driven, "DEBUG=1");
    enter(&mut driven);

    let agent = restarted(&driven);
    answer_the_panel(&mut driven, Some(agent), &["--verbose"]);

    assert_eq!(panel(&driven).launch_words(), ["--verbose", "DEBUG=1"]);
    assert_eq!(panel(&driven).enter_word(), "restart");
}

/// An entry the agent carries stays in the list once unchecked, there to
/// check again, whether or not the history still holds it.
#[test]
fn an_unchecked_carried_entry_stays_listed() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-uncheck"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_unread(&mut driven);
    let agent = restarted(&driven);
    answer_the_panel(&mut driven, Some(agent), &["--verbose"]);

    press(
        &mut driven,
        uze_keys::Action::SelectNext,
        uze_keys::Scope::LaunchOverrides,
    );
    press(
        &mut driven,
        uze_keys::Action::ToggleChoice,
        uze_keys::Scope::LaunchOverrides,
    );

    let step = panel(&driven);
    assert!(step.launch_words().is_empty(), "left out");
    let listed: Vec<(Vec<String>, bool)> = step
        .choices()
        .iter()
        .map(|choice| (choice.words.to_vec(), choice.checked))
        .collect();
    assert_eq!(listed, [(vec!["--verbose".to_owned()], false)]);
    assert_eq!(step.enter_word(), "restart");
}

#[test]
fn an_answer_read_for_another_asker_is_not_the_panels() {
    for asker in [Some("another-agent".to_owned()), None] {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-asker"));
        let mut model = agents_in_the_root_session();
        model.session.as_mut().unwrap().select_tab(TabId(2));
        let mut driven = driven(model, &home);
        driven.attach.identities = identities();
        open_the_panel_unread(&mut driven);

        answer_the_panel(&mut driven, asker.clone(), &["--verbose"]);

        let step = panel(&driven);
        assert!(step.launch_words().is_empty(), "{asker:?}");
        assert!(step.recent_entries().is_none(), "{asker:?}");
        assert!(step.recent_asked.is_some(), "still waiting: {asker:?}");
    }
}

#[test]
fn a_changed_set_restarts_the_agent_on_its_own_harness() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-changed"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_from_the_keyboard(&mut driven, TabId(2));

    type_text(&mut driven, "DEBUG=1");
    enter(&mut driven);
    assert_eq!(panel(&driven).launch_words(), ["DEBUG=1"]);
    assert_eq!(panel(&driven).enter_word(), "restart");
    enter(&mut driven);

    assert!(driven.attach.model.directory_step.is_none());
    assert!(driven.attach.model.context_menu.is_none());
    assert!(
        driven.attach.model.placement_pending,
        "the restart was placed"
    );
    assert_eq!(notice(&driven).as_deref(), Some("agent 1: restarting"));
}

#[test]
fn moving_the_harness_continues_the_agent_there() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-switch"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_from_the_keyboard(&mut driven, TabId(2));

    press(
        &mut driven,
        uze_keys::Action::CaretRight,
        uze_keys::Scope::LaunchOverrides,
    );
    let other = harness_of(TabId(3));
    assert_eq!(
        panel(&driven).chosen_harness().unwrap().integration,
        other,
        "with nothing typed, Right moves the harness"
    );
    read_back(&mut driven, &other, &[]);
    assert_eq!(
        panel(&driven).enter_word(),
        format!("continue in {}", display_name(&other))
    );
    enter(&mut driven);

    assert!(
        driven.attach.model.placement_pending,
        "the switch was placed"
    );
    assert_eq!(
        notice(&driven),
        Some(format!("agent 1: continuing in {}", display_name(&other)))
    );
}

#[test]
fn a_busy_agent_is_asked_before_it_is_restarted() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-busy"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let pane = pane_of(&model, TabId(2));
    model.note_agent_prompt_submission(pane, &identities(), Some("keep going"));
    assert_eq!(model.agent_tab_status(pane, false), AgentTabStatus::Working);
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_from_the_keyboard(&mut driven, TabId(2));
    type_text(&mut driven, "DEBUG=1");
    enter(&mut driven);

    enter(&mut driven);

    let question = driven
        .attach
        .model
        .interrupt
        .as_ref()
        .expect("the restart asks first, over the whole client");
    assert!(question.restart);
    assert_eq!(question.overrides, ["DEBUG=1"]);
    assert!(!question.on_confirm, "the keyboard starts on the way out");
    assert!(!driven.attach.model.placement_pending);

    driven.attach.interrupt_action(uze_keys::Action::FocusNext);
    driven.attach.interrupt_action(uze_keys::Action::Activate);
    assert!(driven.attach.model.interrupt.is_none());
    assert!(
        driven.attach.model.placement_pending,
        "confirmed, it restarts"
    );
}

#[test]
fn a_declined_interruption_leaves_the_agent_running() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-declined"));
    let mut model = agents_in_the_root_session();
    model.session.as_mut().unwrap().select_tab(TabId(2));
    let pane = pane_of(&model, TabId(2));
    model.note_agent_prompt_submission(pane, &identities(), Some("keep going"));
    let mut driven = driven(model, &home);
    driven.attach.identities = identities();
    open_the_panel_from_the_keyboard(&mut driven, TabId(2));
    type_text(&mut driven, "DEBUG=1");
    enter(&mut driven);
    enter(&mut driven);

    driven.attach.interrupt_action(uze_keys::Action::Dismiss);

    assert!(driven.attach.model.interrupt.is_none());
    assert!(!driven.attach.model.placement_pending);
}

/// A harness that cannot continue a conversation says so on a restart too,
/// and the question asked of a busy agent does not promise it.
#[test]
fn a_restart_on_a_harness_without_continuity_says_so() {
    let gap = "Its conversation will not be resumed on a later launch.";
    let gapped = || {
        let mut identities = identities();
        identities[0].continuity_gap = Some(gap.to_owned());
        identities
    };
    for busy in [false, true] {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-gap"));
        let mut model = agents_in_the_root_session();
        model.session.as_mut().unwrap().select_tab(TabId(2));
        if busy {
            let pane = pane_of(&model, TabId(2));
            model.note_agent_prompt_submission(pane, &gapped(), Some("keep going"));
        }
        let mut driven = driven(model, &home);
        driven.attach.identities = gapped();
        open_the_panel_from_the_keyboard(&mut driven, TabId(2));
        type_text(&mut driven, "DEBUG=1");
        enter(&mut driven);

        enter(&mut driven);

        if busy {
            let rows = frame_rows(&mut driven.attach.model).join("\n");
            assert!(!rows.contains("is resumed"), "{rows}");
            assert!(
                rows.contains("conversation with the new overrides"),
                "{rows}"
            );
            driven.attach.interrupt_action(uze_keys::Action::FocusNext);
            driven.attach.interrupt_action(uze_keys::Action::Activate);
        }
        let toasts = &driven.attach.model.remembered.toasts;
        assert!(
            toasts.iter().any(|toast| toast.text == gap),
            "busy {busy}: {:?}",
            toasts.iter().map(|toast| &toast.text).collect::<Vec<_>>()
        );
    }
}

#[test]
fn the_interruption_is_asked_over_the_whole_client() {
    for (restart, confirm) in [(false, "Continue in Other"), (true, "Restart")] {
        let mut model = agents_in_the_root_session();
        model.interrupt = Some(InterruptDialog {
            target: MenuTarget::Tab(TabId(2)),
            label: "agent 1".into(),
            choice: HarnessChoice {
                integration: "other".into(),
                display_name: "Other".into(),
                continuity_gap: None,
            },
            restart,
            overrides: Vec::new(),
            on_confirm: false,
        });
        let rows = frame_rows(&mut model).join("\n");
        assert!(rows.contains("Interrupt agent 1?"), "{rows}");
        assert!(rows.contains(confirm), "{rows}");
    }
}

#[test]
fn a_click_opens_the_panel_and_its_enter_hint_applies_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-click"));
    let mut driven = driven(agents_in_the_root_session(), &home);
    driven.attach.identities = identities();
    menu_on(&mut driven, TabId(2));
    driven.frame();

    let entry = rect_of(&driven, WorkspaceHit::ContextMenuAction(1));
    driven.press(entry.x + 1, entry.y);
    panel(&driven);
    assert!(driven.attach.model.context_menu.is_some(), "the menu stays");
    read_back(&mut driven, &harness_of(TabId(2)), &[]);
    type_text(&mut driven, "DEBUG=1");
    enter(&mut driven);
    driven.frame();
    let apply = rect_of(
        &driven,
        WorkspaceHit::HintAction(uze_keys::Action::Activate),
    );
    driven.press(apply.x, apply.y);

    assert!(
        driven.attach.model.placement_pending,
        "the restart was placed"
    );
}

#[test]
fn a_click_on_the_menu_puts_the_panel_away_and_one_elsewhere_both() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-away"));
    let mut driven = driven(agents_in_the_root_session(), &home).on_a_roomy_terminal();
    driven.attach.identities = identities();
    menu_on(&mut driven, TabId(2));
    driven.frame();
    let entry = rect_of(&driven, WorkspaceHit::ContextMenuAction(1));
    driven.press(entry.x + 1, entry.y);
    driven.frame();

    let back = rect_of(&driven, WorkspaceHit::HintAction(uze_keys::Action::Dismiss));
    driven.press(back.x, back.y);
    assert!(driven.attach.model.directory_step.is_none(), "back is Esc");
    assert!(driven.attach.model.context_menu.is_some());
    driven.press(entry.x + 1, entry.y);
    driven.frame();

    let rename = rect_of(&driven, WorkspaceHit::ContextMenuAction(0));
    driven.press(rename.x + 1, rename.y);
    assert!(driven.attach.model.directory_step.is_none());
    assert!(driven.attach.model.context_menu.is_some());

    driven.press(entry.x + 1, entry.y);
    driven.frame();
    driven.press(0, driven.area.height - 1);
    assert!(driven.attach.model.directory_step.is_none());
    assert!(driven.attach.model.context_menu.is_none());
}

/// At the narrowest frame and in the light theme alike, the panel says
/// what Enter does and the way back, its foot in that theme's hues.
#[test]
fn the_panel_reads_at_a_narrow_width_and_in_the_light_theme() {
    for name in ["dark", "light"] {
        let theme = uze_theme::builtin(name).expect("bundled").clone();
        uze_theme::with_active(theme, || {
            let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-restart-narrow"));
            let mut model = agents_in_the_root_session();
            model.session.as_mut().unwrap().select_tab(TabId(2));
            let mut driven = driven(model, &home);
            driven.area = Rect::new(0, 0, 60, 24);
            driven.attach.identities = identities();
            open_the_panel_from_the_keyboard(&mut driven, TabId(2));

            let buffer = frame_buffer_at(&mut driven.attach.model, driven.area);
            let rows = support::buffer_rows(&buffer);
            let (y, row) = rows
                .iter()
                .enumerate()
                .find(|(_, row)| row.contains("close") && row.contains("back"))
                .unwrap_or_else(|| panic!("the foot is drawn ({name}): {rows:#?}"));
            let x = row[..row.find("close").unwrap()].chars().count() as u16;
            assert_ne!(
                buffer[(x, y as u16)].fg,
                buffer[(0, 0)].bg,
                "the foot is legible in {name}"
            );
            assert!(
                rows.iter().any(|row| row.contains("overrides")),
                "{name}: {rows:#?}"
            );
        });
    }
}
