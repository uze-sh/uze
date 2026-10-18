//! The tab strip: which tabs it shows for the agent in front, their numbers
//! and chips, dragging a tab, and the labels a tab carries.

use super::WorkspaceHit;
use super::support::*;
use super::{
    AgentGroup, DraggingTab, PendingDrop, TabDragGroup, WorkStateView, WorkspaceModel,
    adopt_agent_labels, next_shell_label, pending_tab_drop,
    render::{LeadingSlot, render_tab_strip},
    space_context_agent, space_own_tab, strip_label, strip_tabs, tab_drag_group,
    tab_drag_group_members,
};
use crate::ui::theme::{self, Token};
use ratatui::{Terminal, backend::TestBackend};
use std::time::Duration;
use uze_core::UzeHome;
use uze_terminal::{ClientRequest, SpaceId, TabId};

/// The strip is about one agent at a time: the agent leads it, its own
/// shells follow, and another agent's shells are simply elsewhere.
#[test]
fn the_strip_shows_the_selected_agent_and_only_its_own_shells() {
    let (mut model, first, second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);

    let (rows, _) = tab_strip(&model);
    let strip = rows.join(" ");
    assert!(strip.contains("Agent one"), "the agent leads: {strip}");
    assert!(strip.contains("Agent one shell"), "its own shell: {strip}");
    assert!(
        !strip.contains("Agent two"),
        "and nothing of the other agent: {strip}"
    );
    assert!(
        !strip.contains("○ shell"),
        "not the space's own bootstrap shell either: {strip}"
    );

    model.session.as_mut().expect("session").select_tab(second);
    let (rows, _) = tab_strip(&model);
    let strip = rows.join(" ");
    assert!(strip.contains("Agent two shell"), "{strip}");
    assert!(!strip.contains("Agent one"), "{strip}");
}

/// The space button stands in every context of the space: the agent
/// leading the strip, a shell it opened, and the space's own shells.
/// It is about the space, which every one of them is in.
#[test]
fn the_space_button_stands_in_every_context() {
    let (mut model, first, _second) = two_agents_with_shells();
    let button_is_drawn = |model: &WorkspaceModel| {
        tab_strip(model)
            .1
            .iter()
            .any(|(_, hit)| *hit == WorkspaceHit::OpenOverview)
    };
    let (shell, bootstrap) = {
        let space = model.session.as_ref().expect("session").selected_space();
        let shell = space
            .tabs
            .iter()
            .find(|tab| tab.agent == Some(first))
            .expect("the agent's own shell")
            .id;
        let bootstrap = space
            .tabs
            .iter()
            .find(|tab| tab.agent.is_none() && tab.pane.process != "agent")
            .expect("the space's bootstrap shell")
            .id;
        (shell, bootstrap)
    };
    for tab in [first, shell, bootstrap] {
        model.session.as_mut().expect("session").select_tab(tab);
        assert!(button_is_drawn(&model), "no space button over {tab:?}");
    }
}

/// A tab number means "that chip", so it is counted along the strip —
/// which is contextual — and never along the space's own tab list.
///
/// The list held every agent in the space and both their shells, so a
/// number walked past the chips on screen and landed on another
/// agent: a gesture that only ever meant "the second one here" changed
/// which agent the workspace was about. One list now answers for both
/// the chips and the numbers, which is the only way they can agree.
#[test]
fn a_tab_number_counts_along_the_strip_and_not_past_it() {
    let (mut model, first, second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(second);
    let identities = identities_fixture();
    let session = model.session.as_ref().expect("session");
    let space = session.selected_space();
    let strip = strip_tabs(space, space_context_agent(space, &identities), &identities);

    assert_eq!(strip.len(), 2, "the agent in front, and its own shell");
    assert_eq!(strip[0].id, second, "the agent leads its own strip");
    assert_ne!(
        strip[1].id, first,
        "and the other agent is not on it at any position"
    );

    assert!(
        space.tabs.len() > strip.len(),
        "the space holds more than the strip shows — the bootstrap \
         shell and the other agent's context"
    );
    assert_ne!(
        space.tabs[1].id, strip[1].id,
        "so counting along the space would land somewhere no chip is"
    );
}

/// Selecting one of an agent's shells keeps the strip on that agent —
/// the context is the agent, not whichever tab is selected.
#[test]
fn a_shell_keeps_the_strip_on_the_agent_it_belongs_with() {
    let (mut model, first, _) = two_agents_with_shells();
    let shell = model
        .session
        .as_ref()
        .expect("session")
        .selected_space()
        .tabs
        .iter()
        .find(|tab| tab.agent == Some(first))
        .expect("the agent's own shell")
        .id;
    model.session.as_mut().expect("session").select_tab(shell);

    let (rows, _) = tab_strip(&model);
    let strip = rows.join(" ");
    assert!(strip.contains("Agent one"), "{strip}");
    assert!(strip.contains("Agent one shell"), "{strip}");
}

/// The space's own shells are its own context, reached from its row in
/// the sidebar — no agent leads the strip there.
#[test]
fn the_spaces_own_shell_is_a_context_of_its_own() {
    let (mut model, _, _) = two_agents_with_shells();
    let own = first_tab(&model).id;
    model.session.as_mut().expect("session").select_tab(own);

    let (rows, _) = tab_strip(&model);
    let strip = rows.join(" ");
    assert!(strip.contains("shell"), "{strip}");
    assert!(!strip.contains("Agent"), "{strip}");
}

/// A shell is numbered within the group it joins, so an agent's first
/// shell is "shell 1" however many tabs the space already holds.
#[test]
fn a_shells_number_counts_only_its_own_group() {
    let (mut model, first, _) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);

    assert_eq!(next_shell_label(&model, &identities_fixture()), "shell 2");

    let own = first_tab(&model).id;
    model.session.as_mut().expect("session").select_tab(own);
    assert_eq!(
        next_shell_label(&model, &identities_fixture()),
        "shell 2",
        "the space's own group counts neither agent"
    );
}

/// An agent tab is the agent's from its first frame, before the
/// server's probe has seen the harness in its foreground: a shell
/// opened beside it then is the agent's "shell 1", not the space's
/// "shell 3" counting the bootstrap shell and the agent itself.
#[test]
fn an_agent_not_yet_probed_is_still_the_context_of_its_first_shell() {
    let mut session = session("/repo");
    let space = session.workspace.selected_space;
    let agent = session.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
    let tab = session.workspace.spaces[0]
        .tabs
        .iter_mut()
        .find(|tab| tab.pane.id == agent)
        .expect("agent tab");
    assert_eq!(tab.pane.process, "shell", "nothing has probed it yet");
    tab.env = vec![(
        uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
        "a1".to_owned(),
    )];
    let agent_tab = tab.id;
    session.select_tab(agent_tab);
    let model = model_of(session);
    let identities = identities_fixture();

    let space = model.session.as_ref().expect("session").selected_space();
    assert_eq!(space_context_agent(space, &identities), Some(agent_tab));
    assert_eq!(next_shell_label(&model, &identities), "shell 1");
}

/// The space's row in the sidebar is the way back to the space's own
/// shells: it lands on one, and stays put when you are already there.
#[test]
fn a_spaces_row_lands_on_a_shell_of_its_own() {
    let (mut model, first, _) = two_agents_with_shells();
    let session = model.session.as_mut().expect("session");
    let own = session.workspace.spaces[0].tabs[0].id;

    session.select_tab(first);
    assert_eq!(
        space_own_tab(&session.workspace.spaces[0], &identities_fixture()),
        Some(own),
        "from an agent, back to the space's own shell"
    );

    session.select_tab(own);
    assert_eq!(
        space_own_tab(&session.workspace.spaces[0], &identities_fixture()),
        Some(own),
        "and it stays where it already is"
    );
}

/// An agent leaves the strip only through the sidebar's confirmation,
/// so its chip offers no × for a stray click to land on.
#[test]
fn the_agent_chip_carries_no_close_button() {
    let (mut model, first, _) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);

    let (_, hits) = tab_strip(&model);
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::CloseTab(tab) if *tab == first)),
        "no close hit for the agent"
    );
    assert!(
        hits.iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::CloseTab(_))),
        "its shell still closes"
    );
}

/// A hit's rect alone says which drag group it belongs to: a sidebar
/// row's own agent's space, or the tab strip's own (space, context)
/// pair — the same tab can appear in both (its sidebar row and, when
/// it's the context agent, its own strip chip too), so the rect is
/// what tells them apart, not the tab id.
#[test]
fn tab_drag_group_classifies_by_the_region_a_rect_landed_in() {
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let space = model.session.as_ref().unwrap().workspace.selected_space;
    let layout = full_frame(&mut model);

    let sidebar_rect = model
        .hits
        .iter()
        .find(|(rect, hit)| {
            matches!(hit, WorkspaceHit::SelectTab(tab) if *tab == first)
                && rect.x < layout.sidebar.right()
        })
        .map(|(rect, _)| *rect)
        .expect("the agent's own sidebar row");
    assert_eq!(
        tab_drag_group(&model, &identities_fixture(), &layout, sidebar_rect, first),
        Some(TabDragGroup::Agents(space, AgentGroup::Isolated))
    );

    let strip_rect = model
        .hits
        .iter()
        .find(|(rect, hit)| {
            matches!(hit, WorkspaceHit::SelectTab(tab) if *tab == first)
                && rect.x >= layout.sidebar.right()
        })
        .map(|(rect, _)| *rect)
        .expect("the agent's own strip chip");
    assert_eq!(
        tab_drag_group(&model, &identities_fixture(), &layout, strip_rect, first),
        Some(TabDragGroup::Strip(space, Some(first))),
        "the very same tab, but its strip chip's rect names the strip's group"
    );

    assert_eq!(
        tab_drag_group(&model, &identities_fixture(), &layout, layout.pane, first),
        None,
        "the pane itself belongs to no drag group"
    );
}

#[test]
fn tab_drag_group_members_are_sorted_along_the_groups_axis_and_scoped_to_it() {
    let (mut model, first, second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let space = model.session.as_ref().unwrap().workspace.selected_space;
    let layout = full_frame(&mut model);

    let agents = tab_drag_group_members(
        &model,
        &identities_fixture(),
        &layout,
        TabDragGroup::Agents(space, AgentGroup::Isolated),
    );
    assert_eq!(
        agents.iter().map(|(_, tab)| *tab).collect::<Vec<_>>(),
        vec![first, second],
        "sidebar rows top to bottom, one entry per agent despite each pushing two hits"
    );

    let strip = tab_drag_group_members(
        &model,
        &identities_fixture(),
        &layout,
        TabDragGroup::Strip(space, Some(first)),
    );
    let strip_ids: Vec<TabId> = strip.iter().map(|(_, tab)| *tab).collect();
    assert!(
        strip_ids.contains(&first),
        "the agent chip itself: {strip_ids:?}"
    );
    assert!(
        !strip_ids.contains(&second),
        "never the other agent's group: {strip_ids:?}"
    );
}

/// Reproduces the real sidebar geometry for four agent tabs (each two
/// rows, a gap row between siblings) end to end, dragging the first
/// one: releasing on the second agent's own label row — the bold row
/// a click naturally lands on — has to move it, not silently do
/// nothing because that label row's near half used to read as "put it
/// back where it was".
#[test]
fn dragging_the_first_agent_onto_the_seconds_own_label_row_reorders_it() {
    let mut session = session("/repo");
    let space = session.workspace.selected_space;
    let mut agent_ids = Vec::new();
    for (label, cwd) in [
        ("Agent one", "/repo/.worktrees/a"),
        ("Agent two", "/repo/.worktrees/b"),
        ("Agent three", "/repo/.worktrees/c"),
        ("Agent four", "/repo/.worktrees/d"),
    ] {
        let pane = session.add_tab(space, label.into(), None, 80, 24, cwd.into());
        session.update_pane_status(pane, cwd.into(), "agent".into());
        agent_ids.push(session.selected_space().selected_tab);
    }
    let mut model = model_of(session);
    let layout = full_frame(&mut model);
    let dragged = agent_ids[0];

    let all = tab_drag_group_members(
        &model,
        &identities_fixture(),
        &layout,
        TabDragGroup::Agents(space, AgentGroup::Isolated),
    );
    let origin = all
        .iter()
        .find(|(_, tab)| *tab == dragged)
        .expect("the dragged tab's own row")
        .0
        .y;
    let second_label_row = all
        .iter()
        .find(|(_, tab)| *tab == agent_ids[1])
        .expect("the second agent's own row")
        .0
        .y;
    let members: Vec<_> = all.into_iter().filter(|(_, tab)| *tab != dragged).collect();

    let pending = pending_tab_drop(
        &members,
        TabDragGroup::Agents(space, AgentGroup::Isolated),
        second_label_row,
        origin,
    );
    assert_eq!(
        pending,
        Some(PendingDrop::Before(agent_ids[2])),
        "landing right before the third agent puts the dragged one \
         straight after the second — releasing here must actually move it"
    );

    let PendingDrop::Before(before) = pending.expect("computed above") else {
        unreachable!("asserted Before above");
    };
    let server_session = model.session.as_mut().unwrap();
    assert!(
        server_session.reorder_tab(dragged, Some(before)),
        "a real move, not the no-op dropping on the immediate successor used to be"
    );
    let order: Vec<TabId> = server_session
        .selected_space()
        .tabs
        .iter()
        .map(|t| t.id)
        .filter(|id| agent_ids.contains(id))
        .collect();
    assert_eq!(
        order,
        vec![agent_ids[1], agent_ids[0], agent_ids[2], agent_ids[3]],
        "agent one now sits right after agent two: {order:?}"
    );
}

/// The sidebar draws its one insertion indicator on the pending drop's
/// target row, and nowhere when the drag isn't armed yet — the plain
/// click a press-without-movement still is (see `TAB_DRAG_THRESHOLD`).
#[test]
fn sidebar_indicator_marks_the_pending_drop_row_only_once_armed() {
    let (mut model, first, second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let space = model.session.as_ref().unwrap().workspace.selected_space;
    model.dragging_tab = Some(DraggingTab {
        tab: first,
        group: TabDragGroup::Agents(space, AgentGroup::Isolated),
        origin: 0,
        armed: true,
        pending: Some(PendingDrop::Before(second)),
    });

    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&model, &identities_fixture());
    let second_row = hits
        .iter()
        .find(|(_, hit)| matches!(hit, WorkspaceHit::SelectTab(tab) if *tab == second))
        .map(|(rect, _)| rect.y)
        .expect("the drop target's own row");
    let column = gutter_column(&hits);
    assert!(
        lit_gutter_rows(&buffer, column).contains(&second_row),
        "indicator on the target row: {rows:?}"
    );

    model.dragging_tab = model
        .dragging_tab
        .map(|d| DraggingTab { armed: false, ..d });
    let buffer = sidebar(&model, &identities_fixture()).buffer;
    assert!(
        !lit_gutter_rows(&buffer, column).contains(&second_row),
        "no indicator before the drag is armed"
    );
}

/// A task that acquired a name renames the tab that is running it —
/// without this the name is stored everywhere except where a person
/// looks, since the strip and the sidebar read the *tab's* label.
#[test]
fn a_task_that_acquired_a_name_renames_its_tab() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    for tasks in model.remembered.tasks.values_mut() {
        tasks[0].branch = "fix/branch-naming".to_owned();
        tasks[0].label = "branch naming".to_owned();
    }
    label_every_tab(&mut model, "agent 1");

    let requests = crate::ui::orchestrator::adopt_task_names(&mut model);

    assert!(
        requests.iter().any(|request| matches!(
            request,
            uze_terminal::ClientRequest::RenameTab { label, .. } if label == "branch naming"
        )),
        "the tab takes the name the work acquired: {requests:?}"
    );
    // Once the session echoes the rename, the tab already says what the
    // task says and nothing more is owed.
    label_every_tab(&mut model, "branch naming");
    assert!(
        crate::ui::orchestrator::adopt_task_names(&mut model).is_empty(),
        "a tab already carrying its task's name is left alone"
    );
}

/// A task renamed again — by its agent, or by the operator's own
/// `git branch -m` — carries the tab with it, because the label on
/// that tab is one this mechanism put there.
#[test]
fn a_task_renamed_again_carries_the_tab_it_already_named() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    for tasks in model.remembered.tasks.values_mut() {
        tasks[0].label = "branch naming".to_owned();
    }
    label_every_tab(&mut model, "agent 1");
    let first = crate::ui::orchestrator::adopt_task_names(&mut model);
    assert_eq!(first.len(), 1);
    label_every_tab(&mut model, "branch naming");

    for tasks in model.remembered.tasks.values_mut() {
        tasks[0].label = "renamed by hand".to_owned();
    }
    let second = crate::ui::orchestrator::adopt_task_names(&mut model);

    assert!(
        second.iter().any(|request| matches!(
            request,
            uze_terminal::ClientRequest::RenameTab { label, .. } if label == "renamed by hand"
        )),
        "the tab follows the new name: {second:?}"
    );
}

/// A label the user chose is theirs and stays — the same rule the shell
/// adoption already follows.
#[test]
fn a_tab_the_user_named_is_never_renamed_by_its_task() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    for tasks in model.remembered.tasks.values_mut() {
        tasks[0].label = "branch naming".to_owned();
    }
    label_every_tab(&mut model, "my own name");

    assert!(crate::ui::orchestrator::adopt_task_names(&mut model).is_empty());
}

/// A task still carrying its generated identifier has no name to give.
#[test]
fn an_unnamed_task_renames_nothing() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    for tasks in model.remembered.tasks.values_mut() {
        let id = tasks[0].id.clone();
        tasks[0].label = id;
    }
    label_every_tab(&mut model, "agent 1");

    assert!(crate::ui::orchestrator::adopt_task_names(&mut model).is_empty());
}

/// The sidebar and the strip name the same agent the same way — the
/// tab's own label, the one renaming edits — however its task is
/// labelled. The strip used to prefer the task's label, so a renamed
/// agent read as "Agent" in the sidebar and as its task slug up top.
#[test]
fn an_agent_carries_its_own_name_in_both_the_sidebar_and_the_strip() {
    let model = agent_with_task(WorkStateView::Ready, 1);
    assert!(
        sidebar(&model, &identities_fixture())
            .rows
            .iter()
            .any(|row| row.contains("Agent")),
        "the sidebar names the tab"
    );

    let (rows, _) = tab_strip(&model);
    let strip = rows.join(" ");
    assert!(strip.contains("Agent"), "and so does the strip: {strip}");
    assert!(
        !strip.contains("fix-auth-redirect"),
        "not the task's label: {strip}"
    );
}

/// A shell opened beside an agent is part of that agent's context:
/// typing into the shell must not unselect the agent in the tree.
#[test]
fn the_agent_stays_selected_while_one_of_its_shells_is() {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    let session = model.session.as_mut().unwrap();
    let agent = session.workspace.spaces[0].tabs[0].id;
    session.add_tab(
        SpaceId(1),
        "shell 1".into(),
        Some(agent),
        80,
        24,
        "/repo/.worktrees/ai".into(),
    );
    let shell = session.workspace.spaces[0].tabs[1].id;
    session.select_tab(shell);
    assert_eq!(session.selected_space().selected_tab, shell);

    let rows = sidebar(&model, &identities_fixture()).rows;
    let name_row = rows
        .iter()
        .find(|row| row.contains("Agent"))
        .expect("the agent is named in the tree");
    assert!(
        name_row.contains('\u{25cf}'),
        "the agent still reads as selected: {name_row}"
    );
}

/// The "/" closes the agent off from its shells and the "+" beside
/// them, in the zone hairlines' own faint hue.
#[test]
fn the_strip_separates_the_agent_from_its_shells() {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    let session = model.session.as_mut().unwrap();
    let agent = session.workspace.spaces[0].tabs[0].id;
    session.add_tab(
        SpaceId(1),
        "shell 1".into(),
        Some(agent),
        80,
        24,
        "/repo/.worktrees/ai".into(),
    );

    let (rows, _) = tab_strip(&model);
    let strip = rows
        .iter()
        .find(|row| row.contains("shell 1"))
        .expect("the shell has a tab");
    let slash = strip.find('/').expect("a separator is drawn");
    let shell = strip.find("shell 1").unwrap();
    let plus = strip.rfind('+').expect("the new-shell button is drawn");
    assert!(
        slash < shell && shell < plus,
        "agent / shells +, in that order: {strip:?}"
    );

    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    terminal
        .draw(|frame| {
            render_tab_strip(
                frame,
                frame.area(),
                &model,
                &identities_fixture(),
                LeadingSlot::AgentTabs,
                &mut Vec::new(),
            );
        })
        .unwrap();
    let row = rows.iter().position(|row| row == strip).unwrap() as u16;
    let column = strip[..slash].chars().count() as u16;
    assert_eq!(
        terminal.backend().buffer()[(column, row)].fg,
        theme::color(Token::TextFaint)
    );
}

/// A shell the user typed an agent into keeps nothing of its generated
/// label: it takes the `agent N` label it would have opened with. A
/// label the user chose stays theirs.
#[test]
fn a_shell_that_starts_running_an_agent_takes_an_agent_label() {
    let mut model = agent_session_in("/repo");
    let session = model.session.as_mut().unwrap();
    session.workspace.spaces[0].tabs[0].label = "agent 1".into();
    for label in ["shell 2", "my shell", "shell"] {
        session.add_tab(SpaceId(1), label.into(), None, 80, 24, "/repo".into());
    }
    for tab in &mut session.workspace.spaces[0].tabs {
        tab.pane.process = "agent".into();
    }

    let requests = adopt_agent_labels(&mut model, &identities_fixture());
    assert_eq!(
        requests,
        vec![
            ClientRequest::RenameTab {
                tab: TabId(2),
                label: "agent 2".into(),
            },
            ClientRequest::RenameTab {
                tab: TabId(4),
                label: "agent 3".into(),
            },
        ]
    );
    assert!(
        adopt_agent_labels(&mut model, &identities_fixture()).is_empty(),
        "each tab is asked once"
    );

    let session = model.session.as_mut().unwrap();
    assert!(session.rename_tab(TabId(2), "agent 2".into()));
    assert!(session.rename_tab(TabId(4), "agent 3".into()));
    assert!(adopt_agent_labels(&mut model, &identities_fixture()).is_empty());
    assert!(
        model.remembered.label_adoptions.is_empty(),
        "a confirmed rename leaves the ledger"
    );
}

/// A plain shell stays a shell: nothing runs in it that could earn an
/// agent label.
#[test]
fn a_shell_running_no_agent_keeps_its_label() {
    let mut model = agent_session_in("/repo");
    let session = model.session.as_mut().unwrap();
    session.add_tab(SpaceId(1), "shell 2".into(), None, 80, 24, "/repo".into());
    assert!(adopt_agent_labels(&mut model, &identities_fixture()).is_empty());
}

/// A name typed over an agent's tab is the agent's own, so the agent
/// is found by it once its tab is gone: the rename reaches the
/// record, not only the tab.
#[test]
fn renaming_an_agents_tab_names_the_agent() {
    let repository = uze_testkit::git::Repository::new("orchestrator-rename-agent");
    let root = repository.root().to_path_buf();
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-rename-agent-home"));
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let placement = app
        .workspace()
        .place_new_agent(
            &root,
            Some(uze_application::PlacementKind::Isolated),
            "claude-code",
            &[],
        )
        .unwrap();
    let id = placement.placement.agent().as_str().to_owned();
    let mut model = agent_session();
    stamp_first_tab(&mut model, &id);
    first_tab_mut(&mut model).pane.cwd = placement.cwd.clone();
    let tab = first_tab_mut(&mut model).id;
    model.renaming = Some((
        crate::ui::orchestrator::RenameTarget::Tab(tab),
        crate::ui::orchestrator::RenameBuffer::new("vscode workspace".to_owned()),
    ));
    let mut driven = driven(model, &home);

    let enter = uze_keys::active()
        .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::Rename])
        .expect("enter takes a rename");
    driven.press_key(key_event(enter));
    let answer = driven
        .attach
        .channels
        .labels
        .receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("the name is recorded");

    assert_eq!(answer.outcome, Ok(()));
    assert!(
        app.workspace()
            .tasks(&root)
            .iter()
            .any(|task| task.id == id && task.label == "vscode workspace")
    );
}

/// A shell nobody named reads as the program it is running, and as
/// itself again once that program returns to the prompt; a name the
/// person typed never moves.
#[test]
fn an_unnamed_shell_is_labelled_by_what_it_runs() {
    let mut tab = agent_session().session.unwrap().workspace.spaces[0].tabs[0].clone();
    tab.label = "shell 2".into();

    tab.pane.process = "nvim".into();
    assert_eq!(strip_label(&tab), "nvim");

    tab.pane.process = "zsh".into();
    assert_eq!(strip_label(&tab), "shell 2", "a prompt keeps the number");

    tab.pane.process = "shell".into();
    assert_eq!(strip_label(&tab), "shell 2", "an unprobed pane too");

    tab.label = "server".into();
    tab.pane.process = "cargo".into();
    assert_eq!(strip_label(&tab), "server", "a typed name stands");
}
