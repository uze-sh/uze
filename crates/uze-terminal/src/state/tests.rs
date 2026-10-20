use super::*;

#[test]
fn state_round_trips_and_allocates_stable_identifiers() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    assert_eq!(session.selected_tab().pane.id, PaneId(1));
    assert_eq!(
        session.add_tab(
            session.workspace.selected_space,
            "agent".into(),
            None,
            100,
            30,
            PathBuf::from("/tmp/agent")
        ),
        PaneId(2)
    );
    assert_eq!(session.selected_space().selected_tab, TabId(2));
    let pane = &session.selected_tab().pane;
    assert_eq!(pane.cwd, PathBuf::from("/tmp/agent"));
    let encoded = serde_json::to_string(&session).unwrap();
    assert_eq!(serde_json::from_str::<Session>(&encoded).unwrap(), session);
}

#[test]
fn remove_tab_refuses_the_last_tab_but_allows_the_rest() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    assert_eq!(session.remove_tab(TabId(1)), None);

    let second_pane = session.add_tab(
        session.workspace.selected_space,
        "agent".into(),
        None,
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    assert_eq!(session.selected_space().selected_tab, TabId(2));

    let removed = session.remove_tab(TabId(2)).expect("second tab removed");
    assert_eq!(removed, vec![second_pane]);
    assert_eq!(session.selected_space().tabs.len(), 1);
    assert_eq!(session.selected_space().selected_tab, TabId(1));
}

#[test]
fn remove_tab_reselects_a_neighbor_when_the_active_tab_closes() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    session.add_tab(
        session.workspace.selected_space,
        "two".into(),
        None,
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    session.add_tab(
        session.workspace.selected_space,
        "three".into(),
        None,
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    assert_eq!(session.selected_space().selected_tab, TabId(3));

    session.remove_tab(TabId(3)).expect("third tab removed");
    assert_eq!(session.selected_space().selected_tab, TabId(2));
}

/// A launch lands on the space working in its project: an agent's
/// space first, a space whose shell stands there next, and never a space
/// standing above it.
#[test]
fn a_launch_finds_the_space_working_in_its_project() {
    let mut session = Session::new(seat("/tmp/home"), 80, 24);
    let above = session.workspace.selected_space;
    assert_eq!(
        session.space_for(&seat("/tmp/home/web")),
        None,
        "a tab above the project"
    );

    let NewSpace { space: shells, .. } =
        session.create_space(None, PathBuf::from("/tmp/home/web"), 80, 24);
    assert_eq!(
        session.space_for(&seat("/tmp/home/web")),
        Some(shells),
        "a shell standing in it"
    );

    let NewSpace { space: agents, .. } =
        session.create_space(None, PathBuf::from("/tmp/elsewhere"), 80, 24);
    let pane = session.add_tab(
        agents,
        "agent".into(),
        None,
        80,
        24,
        PathBuf::from("/tmp/home/web/.worktrees/a1"),
    );
    session.record_launch(pane, vec![("UZE_AGENT".to_owned(), "a1".to_owned())]);
    assert_eq!(
        session.space_for(&seat("/tmp/home/web")),
        Some(agents),
        "an agent wins over a shell"
    );
    assert_ne!(session.space_for(&seat("/tmp/home/web")), Some(above));
}

#[test]
fn a_launch_is_recorded_on_the_tab_owning_the_pane_and_cleared_by_a_shell() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    let pane = session.add_tab(
        space,
        "agent 1".into(),
        None,
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    let stamp = vec![("UZE_AGENT".to_owned(), "abc".to_owned())];
    assert!(session.record_launch(pane, stamp.clone()));
    assert_eq!(session.selected_tab().env, stamp);
    assert!(
        !session.record_launch(pane, stamp),
        "recording the same launch changes nothing"
    );
    assert!(session.record_launch(pane, Vec::new()));
    assert!(session.selected_tab().env.is_empty());
    assert!(
        !session.record_launch(PaneId(99), Vec::new()),
        "an unknown pane records nothing"
    );
}

#[test]
fn update_pane_status_reports_change_only_when_something_actually_moved() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    assert!(!session.update_pane_status(PaneId(1), PathBuf::from("/tmp/a"), "shell".into()));
    assert!(session.update_pane_status(PaneId(1), PathBuf::from("/tmp/b"), "vim".into()));
    assert_eq!(session.selected_tab().pane.id, PaneId(1));
    let pane = &session.selected_tab().pane;
    assert_eq!(pane.cwd, PathBuf::from("/tmp/b"));
    assert_eq!(pane.process, "vim");
    assert!(!session.update_pane_status(PaneId(99), PathBuf::from("/tmp/c"), "x".into()));
}

#[test]
fn rename_tab_trims_refuses_blank_and_reports_real_changes_only() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    assert!(session.rename_tab(TabId(1), "  agent  ".into()));
    assert_eq!(session.selected_tab().label, "agent");
    assert!(!session.rename_tab(TabId(1), "agent".into()));
    assert!(!session.rename_tab(TabId(1), "   ".into()));
    assert_eq!(session.selected_tab().label, "agent");
    assert!(!session.rename_tab(TabId(99), "ghost".into()));
}

/// A space is named by the operator. One left unnamed is `space`,
/// numbered while that label is already on screen, and its shell stands
/// where it was asked to.
#[test]
fn an_unnamed_space_is_numbered_and_a_named_one_kept() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);

    session.create_space(None, PathBuf::from("/tmp/frontend"), 80, 24);
    assert_eq!(session.selected_space().label, "space");

    let NewSpace { pane, .. } =
        session.create_space(Some("  ".into()), PathBuf::from("/tmp/frontend"), 80, 24);
    assert_eq!(session.workspace.spaces.len(), 3);
    assert_eq!(session.selected_space().label, "space 2");
    assert_eq!(session.selected_tab().pane.id, pane);
    assert_eq!(
        session.selected_tab().pane.cwd,
        PathBuf::from("/tmp/frontend")
    );

    // A name the caller gives is its own business, repeated or not.
    session.create_space(Some("storefront".into()), PathBuf::from("/tmp/x"), 80, 24);
    session.create_space(Some("storefront".into()), PathBuf::from("/tmp/x"), 80, 24);
    assert_eq!(session.selected_space().label, "storefront");
}

#[test]
fn add_space_creates_a_selected_space_with_its_own_bootstrap_tab() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let NewSpace { pane, .. } =
        session.add_space("frontend".into(), PathBuf::from("/tmp/frontend"), 80, 24);
    assert_eq!(session.workspace.spaces.len(), 2);
    assert_eq!(session.selected_space().label, "frontend");
    assert_eq!(session.selected_space().tabs.len(), 1);
    assert_eq!(session.selected_tab().pane.id, pane);
}

/// "Take me to this project" is answered by the space working there, not
/// by a new one — this is the path `uze` run inside a pane takes, where a
/// second row for the same project is never what was meant. Asking for a
/// space outright is `create_space`, above.
#[test]
fn going_to_a_project_twice_lands_on_the_same_space() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);

    let first = session.open_space(seat("/tmp/frontend"), 80, 24);
    let OpenedSpace::Created(NewSpace { space, pane }) = first else {
        panic!("the first open creates: {first:?}");
    };
    assert_eq!(session.workspace.spaces.len(), 2);
    assert_eq!(session.selected_space().label, "frontend");
    assert_eq!(session.selected_tab().pane.id, pane);

    let again = session.open_space(seat("/tmp/frontend"), 80, 24);
    assert_eq!(again, OpenedSpace::Existing(space));
    assert_eq!(session.workspace.spaces.len(), 2, "no second row");
    assert_eq!(
        session.space(space).expect("still open").label,
        "frontend",
        "and the open space keeps the name it has"
    );
}

#[test]
fn remove_space_returns_every_pane_and_keeps_the_selection_on_a_survivor() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let first_space = session.workspace.selected_space;
    session.add_space("frontend".into(), PathBuf::from("/tmp/frontend"), 80, 24);
    let second_space = session.workspace.selected_space;
    session.add_tab(
        session.workspace.selected_space,
        "extra".into(),
        None,
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    assert_eq!(session.selected_space().tabs.len(), 2);

    let removed = session
        .remove_space(second_space, home(), 80, 24)
        .expect("second space removed");
    assert_eq!(removed.panes.len(), 2);
    assert_eq!(removed.replacement, None, "a space still stands");
    assert_eq!(session.workspace.spaces.len(), 1);
    assert_eq!(session.workspace.selected_space, first_space);
}

/// The last space can be closed like any other: the workspace is never
/// left with nowhere to focus, so an unnamed space takes its place, its
/// shell where the client said.
#[test]
fn removing_the_last_space_opens_the_replacement_in_its_place() {
    let mut session = Session::new(seat("/home/someone"), 80, 24);
    let only = session.workspace.selected_space;

    let removed = session
        .remove_space(only, home(), 80, 24)
        .expect("the last space closes");

    assert_eq!(removed.panes, vec![PaneId(1)]);
    let replacement = removed.replacement.expect("a replacement pane to spawn");
    assert_eq!(session.workspace.spaces.len(), 1);
    let space = session.selected_space();
    assert_ne!(space.id, only, "a new space, not the closed one kept");
    assert_eq!(space.id, replacement.space);
    assert_eq!(space.label, "space");
    assert_eq!(session.selected_tab().pane.cwd, home());
    assert_eq!(session.selected_tab().pane.id, replacement.pane);
}

fn seat(project: &str) -> SpaceSeat {
    SpaceSeat {
        project: PathBuf::from(project),
    }
}

fn home() -> PathBuf {
    PathBuf::from("/home/someone")
}

#[test]
fn rename_space_trims_refuses_blank_and_reports_real_changes_only() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    assert!(session.rename_space(space, "  frontend  ".into()));
    assert_eq!(session.selected_space().label, "frontend");
    assert!(!session.rename_space(space, "frontend".into()));
    assert!(!session.rename_space(space, "   ".into()));
    assert!(!session.rename_space(SpaceId(99), "ghost".into()));
}

#[test]
fn select_space_moves_selection_only_when_the_target_exists_and_differs() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let first_space = session.workspace.selected_space;
    session.add_space("frontend".into(), PathBuf::from("/tmp/frontend"), 80, 24);
    let second_space = session.workspace.selected_space;

    assert!(!session.select_space(second_space), "already selected");
    assert!(session.select_space(first_space));
    assert_eq!(session.workspace.selected_space, first_space);
    assert!(!session.select_space(SpaceId(99)), "unknown space");
    assert_eq!(session.workspace.selected_space, first_space);
}

/// A shell opened alongside an agent belongs with it, and only an
/// agent of its own space can be named — a tab from elsewhere leaves
/// the new one belonging to the space itself rather than dangling.
#[test]
fn a_tab_belongs_only_with_an_agent_of_its_own_space() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let first_space = session.workspace.selected_space;
    let agent = session.selected_tab().id;
    session.add_tab(
        first_space,
        "shell".into(),
        Some(agent),
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    assert_eq!(session.selected_tab().agent, Some(agent));

    session.add_space("frontend".into(), PathBuf::from("/tmp/frontend"), 80, 24);
    let elsewhere = session.workspace.selected_space;
    session.add_tab(
        elsewhere,
        "shell".into(),
        Some(agent),
        80,
        24,
        PathBuf::from("/tmp/frontend"),
    );
    assert_eq!(
        session.selected_tab().agent,
        None,
        "an agent of another space is no context of this one"
    );
}

/// A shell belongs to the agent it was opened next to — it stands in
/// that agent's checkout — so closing the agent closes it too, panes
/// and all, rather than leaving it behind for the space to accumulate.
#[test]
fn closing_an_agent_closes_the_shells_opened_alongside_it() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    let own = session.selected_tab().id;
    session.add_tab(space, "agent".into(), None, 80, 24, PathBuf::from("/tmp/a"));
    let agent = session.selected_tab().id;
    session.add_tab(
        space,
        "shell".into(),
        Some(agent),
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    let shell = session.selected_tab().id;
    let shell_pane = session.selected_tab().pane.id;

    let stopped = session.remove_tab(agent).expect("the agent is removable");

    assert!(
        stopped.contains(&shell_pane),
        "the shell's pane is handed back to be stopped"
    );
    let space = session.selected_space();
    assert!(
        !space.tabs.iter().any(|tab| tab.id == shell),
        "the shell goes with its agent"
    );
    assert_eq!(
        space.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![own],
        "and the space is left with its own"
    );
    assert_eq!(space.selected_tab, own);
}

/// The same removal is refused when it would empty the space: an agent
/// and its own shells are one context, and a space always has somewhere
/// to focus. The client opens the space's replacement shell first,
/// which is what makes the close go through.
#[test]
fn closing_an_agent_that_is_the_whole_space_is_refused() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    let bootstrap = session.selected_tab().id;
    session.add_tab(space, "agent".into(), None, 80, 24, PathBuf::from("/tmp/a"));
    let agent = session.selected_tab().id;
    session.add_tab(
        space,
        "shell".into(),
        Some(agent),
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    session
        .remove_tab(bootstrap)
        .expect("the space's own shell is removable");

    assert!(
        session.remove_tab(agent).is_none(),
        "an agent and its shells are all the space has left"
    );
    assert_eq!(
        session.selected_space().tabs.len(),
        2,
        "nothing was removed"
    );
}

/// The one genuinely new invariant this layer introduces: tab lookups
/// (`rename_tab`/`update_pane_status`) must find a tab that lives in a
/// space other than the currently selected one, not just search the
/// selected space's own list.
#[test]
fn tab_operations_reach_a_tab_in_a_non_selected_space() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let original_tab = session.selected_tab().id;
    let original_pane = session.selected_tab().pane.id;
    session.add_space("frontend".into(), PathBuf::from("/tmp/frontend"), 80, 24);
    // The newly added space is now selected; `original_tab` lives in
    // the *other*, non-selected space.
    assert_ne!(session.selected_tab().id, original_tab);

    assert!(session.rename_tab(original_tab, "renamed".into()));
    assert!(session.update_pane_status(original_pane, PathBuf::from("/tmp/moved"), "vim".into()));

    // The change must have landed on the non-selected space's tab, not
    // the currently selected one — switch back and check.
    let original_space = session.workspace.spaces[0].id;
    session.select_space(original_space);
    assert_eq!(session.selected_tab().label, "renamed");
    let pane = &session.selected_tab().pane;
    assert_eq!(pane.cwd, PathBuf::from("/tmp/moved"));
}

/// A seed names its agent by position because restoring mints fresh
/// ids; what must survive a server restart is which tab belongs with
/// which, not the numbers they happened to carry.
#[test]
fn restoring_rebuilds_which_tab_belongs_with_which() {
    let (session, _) = Session::restore(
        vec![SpaceSeed {
            id: SpaceId(1),
            label: "frontend".into(),
            tabs: vec![
                TabSeed {
                    label: "claude".into(),
                    cwd: PathBuf::from("/tmp/a/web"),
                    agent: None,
                    launch: Launch::Shell,
                },
                TabSeed {
                    label: "shell".into(),
                    cwd: PathBuf::from("/tmp/a/web"),
                    agent: Some(0),
                    launch: Launch::Shell,
                },
                TabSeed {
                    label: "loose".into(),
                    cwd: PathBuf::from("/tmp/a"),
                    agent: Some(7),
                    launch: Launch::Shell,
                },
            ],
        }],
        2,
    )
    .expect("a space stands");

    let tabs = &session.workspace.spaces[0].tabs;
    assert_eq!(tabs[1].agent, Some(tabs[0].id));
    assert_eq!(tabs[0].agent, None, "an agent belongs with nothing");
    assert_eq!(tabs[2].agent, None, "an index off the end is nobody");
}

#[test]
fn restore_rebuilds_the_seeded_shape_with_sequential_ids() {
    let (session, _) = Session::restore(
        vec![
            SpaceSeed {
                id: SpaceId(1),
                label: "frontend".into(),
                tabs: vec![
                    TabSeed {
                        label: "claude".into(),
                        cwd: PathBuf::from("/tmp/a/web"),
                        agent: None,
                        launch: Launch::Shell,
                    },
                    TabSeed {
                        label: "shell".into(),
                        cwd: PathBuf::from("/tmp/a"),
                        agent: None,
                        launch: Launch::Shell,
                    },
                ],
            },
            SpaceSeed {
                id: SpaceId(2),
                label: "backend".into(),
                tabs: vec![TabSeed {
                    label: "codex".into(),
                    cwd: PathBuf::from("/tmp/a/api"),
                    agent: None,
                    launch: Launch::Shell,
                }],
            },
        ],
        3,
    )
    .expect("a space stands");

    assert_eq!(session.workspace.spaces.len(), 2);
    assert_eq!(session.workspace.selected_space, SpaceId(1));

    let frontend = &session.workspace.spaces[0];
    assert_eq!(frontend.id, SpaceId(1));
    assert_eq!(frontend.label, "frontend");
    assert_eq!(frontend.tabs.len(), 2);
    assert_eq!(frontend.selected_tab, frontend.tabs[0].id);
    assert_eq!(frontend.tabs[0].id, TabId(1));
    assert_eq!(frontend.tabs[0].label, "claude");
    let pane = &frontend.tabs[0].pane;
    assert_eq!(pane.id, PaneId(1));
    assert_eq!(pane.cwd, PathBuf::from("/tmp/a/web"));
    assert_eq!(frontend.tabs[1].id, TabId(2));

    let backend = &session.workspace.spaces[1];
    assert_eq!(backend.id, SpaceId(2));
    assert_eq!(backend.tabs[0].id, TabId(3));
    let pane = &backend.tabs[0].pane;
    assert_eq!(pane.id, PaneId(3));

    // Ids allocated after a restore must not collide with any restored
    // one — proves the counters were advanced past the highest id
    // handed out, not reset to the defaults `Session::new` starts at.
    assert_eq!(session.next_space_id, 3);
    assert_eq!(session.next_tab_id, 4);
    assert_eq!(session.next_pane_id, 4);
}

#[test]
fn reorder_tab_moves_among_agent_tabs_and_ignores_a_no_op_move() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    // The bootstrap tab (id 1) plus two agent tabs: [1, 2, 3].
    session.add_tab(space, "b".into(), None, 80, 24, PathBuf::from("/tmp/a"));
    session.add_tab(space, "c".into(), None, 80, 24, PathBuf::from("/tmp/a"));
    let ids = |session: &Session| -> Vec<u64> {
        session
            .selected_space()
            .tabs
            .iter()
            .map(|t| t.id.0)
            .collect()
    };
    assert_eq!(ids(&session), vec![1, 2, 3]);

    // Moving tab 1 before tab 2 (its immediate successor) is already
    // the current order — a no-op.
    assert!(!session.reorder_tab(TabId(1), Some(TabId(2))));
    assert_eq!(ids(&session), vec![1, 2, 3]);

    // Moving tab 1 before tab 3 puts it between 2 and 3.
    assert!(session.reorder_tab(TabId(1), Some(TabId(3))));
    assert_eq!(ids(&session), vec![2, 1, 3]);
}

#[test]
fn reorder_tab_moves_among_one_agents_shell_tabs() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    let agent = session.selected_tab().id;
    session.add_tab(
        space,
        "shell-a".into(),
        Some(agent),
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    session.add_tab(
        space,
        "shell-b".into(),
        Some(agent),
        80,
        24,
        PathBuf::from("/tmp/a"),
    );
    // [agent(1), shell-a(2), shell-b(3)].
    let shell_a = TabId(2);
    let shell_b = TabId(3);

    assert!(session.reorder_tab(shell_b, Some(shell_a)));
    let ids: Vec<u64> = session
        .selected_space()
        .tabs
        .iter()
        .map(|t| t.id.0)
        .collect();
    assert_eq!(ids, vec![1, 3, 2], "shell-b now sits before shell-a");
    // Reordering never touches which agent a shell belongs with.
    assert_eq!(session.selected_space().tabs[1].agent, Some(agent));
    assert_eq!(session.selected_space().tabs[2].agent, Some(agent));
}

#[test]
fn reorder_tab_moves_to_the_end_when_before_is_none() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    session.add_tab(space, "b".into(), None, 80, 24, PathBuf::from("/tmp/a"));
    session.add_tab(space, "c".into(), None, 80, 24, PathBuf::from("/tmp/a"));

    // Tab 3 is already last — moving it to the end is a no-op.
    assert!(!session.reorder_tab(TabId(3), None));

    assert!(session.reorder_tab(TabId(1), None));
    let ids: Vec<u64> = session
        .selected_space()
        .tabs
        .iter()
        .map(|t| t.id.0)
        .collect();
    assert_eq!(ids, vec![2, 3, 1]);
}

#[test]
fn reorder_tab_rejects_a_target_from_a_different_space() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let first_space_tab = session.selected_tab().id;
    session.add_space("frontend".into(), PathBuf::from("/tmp/frontend"), 80, 24);
    let other_space_tab = session.selected_tab().id;
    assert_ne!(first_space_tab, other_space_tab);

    assert!(!session.reorder_tab(first_space_tab, Some(other_space_tab)));
    // Neither space's order changed.
    assert_eq!(session.workspace.spaces[0].tabs[0].id, first_space_tab);
    assert_eq!(session.workspace.spaces[1].tabs[0].id, other_space_tab);
}

#[test]
fn reorder_tab_rejects_missing_tabs_and_self_targeting() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    let space = session.workspace.selected_space;
    session.add_tab(space, "b".into(), None, 80, 24, PathBuf::from("/tmp/a"));

    assert!(
        !session.reorder_tab(TabId(99), Some(TabId(1))),
        "no such tab"
    );
    assert!(
        !session.reorder_tab(TabId(1), Some(TabId(99))),
        "no such target"
    );
    assert!(
        !session.reorder_tab(TabId(1), Some(TabId(1))),
        "before itself"
    );
}

#[test]
fn reorder_space_moves_before_a_target_or_to_the_end() {
    let mut session = Session::new(seat("/tmp/a"), 80, 24);
    session.add_space("b".into(), PathBuf::from("/tmp/b"), 80, 24);
    session.add_space("c".into(), PathBuf::from("/tmp/c"), 80, 24);
    let order = |session: &Session| -> Vec<u64> {
        session
            .workspace
            .spaces
            .iter()
            .map(|space| space.id.0)
            .collect()
    };
    let [a, b, c] = [SpaceId(1), SpaceId(2), SpaceId(3)];
    assert_eq!(order(&session), vec![1, 2, 3]);

    assert!(session.reorder_space(c, Some(a)));
    assert_eq!(order(&session), vec![3, 1, 2]);

    assert!(session.reorder_space(c, None));
    assert_eq!(order(&session), vec![1, 2, 3]);

    assert!(!session.reorder_space(a, Some(b)), "already before it");
    assert!(!session.reorder_space(c, None), "already last");
    assert!(!session.reorder_space(a, Some(a)), "before itself");
    assert!(!session.reorder_space(SpaceId(9), None), "no such space");
    assert!(
        !session.reorder_space(a, Some(SpaceId(9))),
        "no such target"
    );
    assert_eq!(order(&session), vec![1, 2, 3]);
}

#[test]
fn restore_with_no_usable_seeds_restores_nothing() {
    let restored = Session::restore(
        vec![SpaceSeed {
            id: SpaceId(1),
            label: "empty".into(),
            tabs: vec![],
        }],
        2,
    );
    assert!(restored.is_none());
}

/// A space with nothing in it is dropped on the way back, and every
/// pane after it still comes back with its own launch rather than the
/// one of whichever tab happened to sit at its position.
#[test]
fn an_empty_space_shifts_no_launch_onto_another_tab() {
    let agent = Launch::Program {
        argv: vec!["claude".to_owned()],
        env: vec![("UZE_AGENT".to_owned(), "abc".to_owned())],
    };
    let seed = |id: u64, label: &str, launch: Launch| SpaceSeed {
        id: SpaceId(id),
        label: label.into(),
        tabs: vec![TabSeed {
            label: label.into(),
            cwd: PathBuf::from("/tmp/seed"),
            agent: None,
            launch,
        }],
    };
    let empty = SpaceSeed {
        tabs: Vec::new(),
        ..seed(1, "empty", Launch::Shell)
    };

    let (session, launches) = Session::restore(
        vec![
            empty,
            seed(2, "agent", agent.clone()),
            seed(3, "shell", Launch::Shell),
        ],
        4,
    )
    .expect("two spaces stand");

    let pane_of = |label: &str| {
        session
            .workspace
            .spaces
            .iter()
            .find(|space| space.label == label)
            .expect("restored")
            .tabs[0]
            .pane
            .id
    };
    assert_eq!(
        launches,
        vec![(pane_of("agent"), agent), (pane_of("shell"), Launch::Shell)]
    );
}

/// A space keeps its id across a restart, and the id of one closed before
/// it is never handed to another: what a client keeps by id would follow
/// the wrong space.
#[test]
fn a_restored_space_keeps_its_id_and_a_closed_ones_is_not_reused() {
    let seed = |id: u64| SpaceSeed {
        id: SpaceId(id),
        label: format!("space {id}"),
        tabs: vec![TabSeed {
            label: "shell".into(),
            cwd: PathBuf::from("/tmp/seed"),
            agent: None,
            launch: Launch::Shell,
        }],
    };
    let (mut session, _) = Session::restore(vec![seed(2), seed(4)], 6).expect("two spaces stand");
    assert_eq!(
        session
            .workspace
            .spaces
            .iter()
            .map(|space| space.id)
            .collect::<Vec<_>>(),
        vec![SpaceId(2), SpaceId(4)]
    );
    let NewSpace { space, .. } = session.create_space(None, PathBuf::from("/tmp"), 80, 24);
    assert_eq!(space, SpaceId(6), "space 5 was closed before the restart");
}
