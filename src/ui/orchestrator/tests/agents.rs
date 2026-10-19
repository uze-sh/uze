//! The agents in a space's column: the root and isolated groups, their
//! captions, and which agent a row selects.

use super::WorkspaceHit;
use super::support::*;
use super::{
    AgentIdentity, AgentView, WorkStateView, WorkspaceModel,
    render::{compute_layout, task_mark},
};
use crate::ui::theme::{self, Token};
use ratatui::layout::Rect;
use ratatui::style::Color;
use std::path::{Path, PathBuf};
use uze_core::UzeHome;
use uze_terminal::{ClientEvent, ClientRequest, PaneId, SpaceId, TabId};

/// A named task reads as its name, once. The label *is* the branch's
/// subject (`worktree::label_of`), so the caption that used to carry
/// the branch was the same words with the type in front. This is where
/// a claim about what the *screen says* belongs — a journey may only
/// gate on screen text, never assert it.
#[test]
fn a_named_task_reads_as_its_name_in_the_sidebar() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    for tasks in model.remembered.tasks.values_mut() {
        tasks[0].branch = "fix/branch-naming".to_owned();
        tasks[0].label = "branch naming".to_owned();
    }
    // The tab carries the name the task took, which is what
    // `adopt_task_names` puts there in the product.
    label_every_tab(&mut model, "branch naming");

    let rows = sidebar(&model, &identities_fixture()).rows;

    assert!(
        rows.iter().any(|row| row.contains("branch naming")),
        "the name the agent chose is the row: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("fix/branch-naming")),
        "and the branch it was derived from is not repeated under it: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("agent/")),
        "nor is the generated identifier anywhere on the screen: {rows:?}"
    );
}

/// A caption too long for the column is elided, not cut. It used to
/// run under the row's own right-aligned caption and off the sidebar,
/// taking that caption's meaning with it and ending mid-word with
/// nothing to say it had been shortened.
#[test]
fn a_long_caption_is_elided_rather_than_run_off_the_sidebar() {
    const LONG: &str = "a-harness-named-longer-than-any-sidebar-column-could-hold";
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    for space in &mut model.session.as_mut().unwrap().workspace.spaces {
        for tab in &mut space.tabs {
            tab.pane.process = LONG.to_owned();
        }
    }
    let identities = vec![AgentIdentity {
        binary: LONG,
        integration: "long",
        display_name: "Long",
        launch: std::path::PathBuf::from(LONG),
        continuity_gap: None,
        configured: true,
    }];

    let rows = sidebar(&model, &identities).rows;
    let caption = rows
        .iter()
        .find(|row| row.contains("a-harness-named"))
        .expect("what runs there reads under the agent's name");

    // Past the caption sits the sidebar's own divider, which is the
    // proof nothing ran over the column's edge.
    assert!(
        caption
            .trim_end()
            .trim_end_matches('│')
            .trim_end()
            .ends_with('…'),
        "the name is elided, and says so: {caption}"
    );
    assert!(
        !caption.contains(LONG),
        "so the whole name cannot be on the row: {caption}"
    );
}

/// A slot outlives the tasks that run in it, and a task that ended
/// keeps naming the slot it ran in — so a reused directory is named by
/// two tasks at once. The row belongs to whoever is in it now; reading
/// the first match handed the new agent the previous one's delivered
/// arrow, which is the mark this reads for.
#[test]
fn a_reused_slot_reads_the_task_in_it_now_not_the_one_before() {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    stamp_first_tab(&mut model, "now");
    let before = AgentView {
        id: "before".into(),
        branch: "agent/before".into(),
        created_at_unix: 1,
        ..task_in(
            "/repo/.worktrees/ai",
            "before",
            WorkStateView::Integrated,
            2,
        )
    };
    let now = AgentView {
        id: "now".into(),
        branch: "agent/now".into(),
        created_at_unix: 2,
        ..task_in("/repo/.worktrees/ai", "now", WorkStateView::Running, 0)
    };
    model
        .remembered
        .tasks
        .insert(PathBuf::from("/repo"), vec![before, now]);

    let rows = sidebar(&model, &identities_fixture()).rows;
    let (delivered, _) = task_mark(&WorkStateView::Integrated).expect("integrated is marked");
    let name_row = rows
        .iter()
        .find(|row| row.contains("Agent"))
        .expect("the agent names its own row");
    assert!(
        !name_row.contains(&delivered),
        "a new agent delivered nothing: {name_row}"
    );
}

/// The rows the tree draws for a space's agents, by the row each
/// `SelectTab` hit was pushed for.
fn agent_rows(hits: &[(Rect, WorkspaceHit)]) -> Vec<u16> {
    let mut rows: Vec<u16> = hits
        .iter()
        .filter(|(_, hit)| matches!(hit, WorkspaceHit::SelectTab(_)))
        .map(|(rect, _)| rect.y)
        .collect();
    rows.dedup();
    rows
}

/// An agent in the root is the same two-row item an isolated one is:
/// its name, and beneath it the harness running it. They all stand in
/// one directory on one branch, so the branch never told two rows
/// apart and the harness always does.
#[test]
fn a_space_draws_each_agent_in_its_root_over_the_harness_it_runs() {
    let model = agents_in_the_root_session();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    let agents = agent_rows(&hits);
    assert_eq!(agents.len(), 4, "two rows per agent: {rows:?}");
    for (label, harness, row) in [
        ("agent 1", "claude", agents[0]),
        ("agent 2", "codex", agents[2]),
    ] {
        assert!(rows[row as usize].contains(label), "{rows:?}");
        assert!(
            rows[row as usize + 1].contains(harness),
            "what runs {label}, beneath it: {rows:?}"
        );
    }
    assert!(
        !rows.iter().any(|row| row.contains("main")),
        "and the branch they share is said by the space, not by each of them: {rows:?}"
    );
    assert_eq!(
        agents[2],
        agents[1] + 1,
        "the next agent follows directly: two rows per item, no gap: {rows:?}"
    );
    let branch = theme::glyph(theme::Symbol::TreeBranch);
    assert!(
        !rows.iter().any(|row| row.contains(&branch)),
        "nothing branches off the rail, in either group: {rows:?}"
    );
}

/// Two rows per agent and no gap between them, so what says which
/// item the keyboard is on has to be the item itself: its two rows
/// carry the neutral selection over the panel every other row sits
/// on, never a hue of its own.
#[test]
fn the_agent_receiving_keystrokes_wears_the_selection_over_the_space() {
    let model = agents_in_the_root_session();
    let Sidebar {
        buffer, hits, rows, ..
    } = sidebar(&model, &identities_in_the_root());
    let agents = agent_rows(&hits);
    let plain = theme::color(Token::SurfaceRaisedSubtle);
    let selected = theme::color(Token::SurfaceSelected);
    assert_ne!(selected, plain, "the selection is a surface of its own");

    // `agent 2` is the space's context agent (see `agents_in_the_root_session`).
    for row in [agents[2], agents[3]] {
        assert_eq!(
            buffer[(2, row)].bg,
            selected,
            "both rows of the item in front: {rows:?}"
        );
    }
    for row in [agents[0], agents[1]] {
        assert_eq!(
            buffer[(2, row)].bg,
            plain,
            "and every other agent keeps the space's own panel: {rows:?}"
        );
    }
}

/// Outside a Git repository there is no branch to name, and the row
/// reads exactly as it does inside one: the caption answers "what
/// runs here", which is a question every directory has an answer to.
#[test]
fn an_agent_outside_a_repository_reads_as_any_other() {
    let mut model = agents_in_the_root_session();
    model.remembered.branches.clear();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    let agents = agent_rows(&hits);
    assert!(rows[agents[1] as usize].contains("claude"), "{rows:?}");
    assert!(
        !rows[agents[1] as usize].contains("/repo"),
        "no directory stands in for it: {rows:?}"
    );
}

#[test]
fn agents_in_the_root_wear_the_same_status_glyphs_as_isolated_ones() {
    let model = agents_in_the_root_session();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    let agents = agent_rows(&hits);
    use crate::ui::theme::Symbol;
    let idle = theme::glyph(Symbol::StatusIdle);
    let selected = theme::glyph(crate::ui::theme::Symbol::StatusSelected);
    assert!(rows[agents[0] as usize].contains(&idle), "{rows:?}");
    assert!(rows[agents[2] as usize].contains(&selected), "{rows:?}");
}

/// The agent receiving keystrokes lights its stretch of the gutter —
/// both of its rows, and nothing else — in its own group's hue.
#[test]
fn the_selected_agent_lights_its_stretch_of_the_gutter_and_nothing_else() {
    let model = agents_in_the_root_session();
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&model, &identities_in_the_root());
    let agents = agent_rows(&hits);
    assert_eq!(
        lit_gutter_rows(&buffer, gutter_column(&hits)),
        vec![agents[2], agents[3]],
        "an agent in the root lights the hue of its group: {rows:?}"
    );

    let tree = agent_with_task(WorkStateView::Ready, 1);
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&tree, &identities_fixture());
    let agents = agent_rows(&hits);
    assert_eq!(
        lit_gutter_rows(&buffer, gutter_column(&hits)),
        vec![agents[0], agents[1]],
        "and an isolated one the other: {rows:?}"
    );
}

#[test]
fn an_empty_space_draws_its_root_as_a_caption() {
    let session = session("/repo");
    let model = model_of(session);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    assert!(
        rows.iter().any(|row| without_chrome(row) == "/repo"),
        "{rows:?}"
    );
    assert!(
        hits.iter()
            .filter(|(_, hit)| matches!(hit, WorkspaceHit::SelectSpace(_)))
            .count()
            >= 2,
        "the caption selects the space like the header does"
    );
}

#[test]
fn an_agent_in_the_root_whose_harness_exited_is_not_an_agent_row() {
    let mut model = agents_in_the_root_session();
    model
        .session
        .as_mut()
        .unwrap()
        .update_pane_status(PaneId(3), "/repo".into(), "bash".into());
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    assert_eq!(agent_rows(&hits).len(), 2, "one two-row item: {rows:?}");
}

/// An agent in the root has no task, so its row has nothing to deliver and no state
/// to mark; and every row selects the agent it names.
#[test]
fn a_row_in_the_root_selects_its_own_agent_and_offers_no_delivery() {
    let model = agents_in_the_root_session();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_))),
        "nothing to send: {rows:?}"
    );
    let marks = [
        theme::Symbol::PlusMinus,
        theme::Symbol::TaskReady,
        theme::Symbol::ArrowExternal,
        theme::Symbol::MarkAttention,
    ]
    .map(theme::glyph);
    assert!(
        !rows
            .iter()
            .any(|row| marks.iter().any(|mark| row.contains(mark.as_str()))),
        "no task mark: {rows:?}"
    );
    let space = &model.session.as_ref().unwrap().workspace.spaces[0];
    for (rect, hit) in &hits {
        let WorkspaceHit::SelectTab(tab) = hit else {
            continue;
        };
        let label = &space
            .tabs
            .iter()
            .find(|candidate| candidate.id == *tab)
            .expect("the hit names a tab of the space")
            .label;
        // The label row, or the caption row beneath it.
        let item = [rect.y, rect.y.saturating_sub(1)];
        assert!(
            item.iter()
                .any(|row| rows[*row as usize].contains(label.as_str())),
            "the row at {} selects {label}: {rows:?}",
            rect.y
        );
    }
}

/// A space whose agents all work in checkouts of their own, so the
/// group's trunk has to carry through one row and close on the next.
fn a_space_of_isolated_agents() -> WorkspaceModel {
    let mut model = agents_in_the_root_session();
    let mut tasks = Vec::new();
    for id in ["a1", "a2"] {
        let checkout = format!("/repo/.worktrees/{id}");
        let stamped = model.session.as_mut().unwrap().workspace.spaces[0]
            .tabs
            .iter_mut()
            .find(|tab| {
                tab.env.iter().any(|(key, value)| {
                    key == uze_terminal::launch::AGENT_IDENTITY_VARIABLE && value == id
                })
            })
            .expect("the agent its launch stamped");
        stamped.pane.cwd = PathBuf::from(&checkout);
        let mut task = task_in(&checkout, id, WorkStateView::Running, 0);
        task.id = id.into();
        task.branch = format!("agent/{id}");
        tasks.push(task);
    }
    model.remembered.tasks.insert(PathBuf::from("/repo"), tasks);
    model
}

/// The isolated group hangs off a trunk of its own, one column in
/// from the space's gutter: a corner beside each agent's name and the
/// line running on through the caption under it. Both ends close — a
/// `├` carries a stem upward as well, so at the top of the group it
/// pointed at the blank row above and read as a line broken off
/// rather than one starting.
#[test]
fn the_isolated_group_hangs_off_a_trunk_of_its_own() {
    let model = a_space_of_isolated_agents();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    let at = agent_rows(&hits);
    assert_eq!(at.len(), 4, "two rows per agent: {rows:?}");
    let column = gutter_column(&hits) + 1;
    let glyph = |row: u16| {
        rows[row as usize]
            .chars()
            .nth(column as usize)
            .expect("the trunk column is drawn")
            .to_string()
    };
    let stem = |symbol| {
        theme::glyph(symbol)
            .chars()
            .next()
            .expect("the glyph")
            .to_string()
    };
    use crate::ui::theme::Symbol;
    assert_eq!(glyph(at[0]), stem(Symbol::TreeFirst), "opens: {rows:?}");
    assert_eq!(glyph(at[1]), stem(Symbol::TreeVertical), "{rows:?}");
    assert_eq!(glyph(at[2]), stem(Symbol::TreeLast), "closes: {rows:?}");
    assert_eq!(glyph(at[3]), " ", "the group closed above: {rows:?}");
    assert_ne!(
        stem(Symbol::TreeFirst),
        stem(Symbol::TreeBranch),
        "the opening corner is not the carrying one: {rows:?}"
    );

    // An agent in the space's own root spends no column on a trunk:
    // that column is where its own status glyph stands. Nor does a
    // group of one, which has nothing for a line to join.
    let marks = [
        theme::glyph(Symbol::StatusIdle),
        theme::glyph(Symbol::StatusSelected),
    ];
    for model in [agents_in_the_root_session(), a_space_with_both_groups()] {
        let drawn = sidebar(&model, &identities_in_the_root());
        let column = (gutter_column(&drawn.hits) + 1) as usize;
        for at in agent_rows(&drawn.hits).into_iter().step_by(2) {
            assert!(
                marks.contains(
                    &drawn.rows[at as usize]
                        .chars()
                        .nth(column)
                        .expect("the column is drawn")
                        .to_string()
                ),
                "the agent stands there itself: {:?}",
                drawn.rows
            );
        }
    }
}

#[test]
fn the_two_groups_are_drawn_in_order_with_one_blank_row_between_them() {
    let model = a_space_with_both_groups();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    let agents = agent_rows(&hits);
    assert_eq!(agents.len(), 4, "two rows per agent: {rows:?}");

    assert!(
        rows[agents[0] as usize].contains("agent 1"),
        "the root's agent leads: {rows:?}"
    );
    assert!(
        rows[agents[2] as usize].contains("agent 2"),
        "the isolated one follows: {rows:?}"
    );
    let gap = agents[1] + 1;
    assert_eq!(
        gap + 1,
        agents[2],
        "exactly one row stands between the groups: {rows:?}"
    );
    assert!(
        without_chrome(&rows[gap as usize]).is_empty(),
        "and it says nothing: {rows:?}"
    );
}

#[test]
fn a_space_whose_agents_share_one_group_has_no_separator() {
    let model = agents_in_the_root_session();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    let agents = agent_rows(&hits);
    assert_eq!(agents.len(), 4, "two rows per agent: {rows:?}");
    assert_eq!(
        agents[1] + 1,
        agents[2],
        "one agent's item runs straight into the next: {rows:?}"
    );
}

/// Isolating an agent moves its row out of the root's group and into
/// the other one, which is how the operator sees the action happened
/// — the same tab, in a different place, under a separator that was
/// not there before.
#[test]
fn isolating_an_agent_moves_its_row_into_the_other_group() {
    let row_of = |hits: &[(Rect, WorkspaceHit)]| {
        hits.iter()
            .find_map(|(rect, hit)| match hit {
                WorkspaceHit::SelectTab(tab) if *tab == TabId(3) => Some(rect.y),
                _ => None,
            })
            .expect("the second agent's row")
    };
    let before = agents_in_the_root_session();
    let Sidebar { rows, hits, .. } = sidebar(&before, &identities_in_the_root());
    let second = row_of(&hits);
    assert!(
        !without_chrome(&rows[second as usize - 1]).is_empty(),
        "in the root it follows the agent above it: {rows:?}"
    );

    let after = a_space_with_both_groups();
    let Sidebar { rows, hits, .. } = sidebar(&after, &identities_in_the_root());
    let moved = row_of(&hits);
    assert!(moved > second, "it moved down past the separator: {rows:?}");
    assert!(
        without_chrome(&rows[moved as usize - 1]).is_empty(),
        "and now stands in the other collection: {rows:?}"
    );
}

/// The agent chords walk a space's agents in the order they are
/// drawn, wrapping at the ends.
#[test]
fn the_agent_chords_walk_a_spaces_agents() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-flat-step"));
    let model = agents_in_the_root_session();
    let Sidebar {
        rows, hits: drawn, ..
    } = sidebar(&model, &identities_in_the_root());
    let space = &model.session.as_ref().unwrap().workspace.spaces[0];
    let top = space
        .tabs
        .iter()
        .find(|tab| rows[agent_rows(&drawn)[0] as usize].contains(tab.label.as_str()))
        .expect("the top row names a tab")
        .id;
    assert_ne!(space.selected_tab, top, "the fixture selects the last row");
    let mut driven = driven(model, &home);
    driven.attach.identities = identities_in_the_root();
    let next = uze_keys::active()
        .chord_for(uze_keys::Action::NextAgent, &[uze_keys::Scope::Workspace])
        .expect("stepping agents is reachable from the keyboard");

    driven.press_key(key_event(next));

    assert!(
        driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::SelectTab { tab } if *tab == top)),
        "from the last row, the next agent is the top row"
    );
}

/// The window between a placement and the evaluation that lists its
/// task: the only task on record in the slot is the one before, and
/// reading it by directory named the new agent's tab after it for good.
/// The tab is for the agent its launch named, and nothing else.
#[test]
fn a_new_agent_never_takes_the_name_of_the_task_before_it_in_the_slot() {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    let tab = first_tab(&model).id;
    stamp_first_tab(&mut model, "now");
    let before = AgentView {
        id: "before".into(),
        ..task_in(
            "/repo/.worktrees/ai",
            "nerd font symbols",
            WorkStateView::Integrated,
            2,
        )
    };
    model
        .remembered
        .tasks
        .insert(PathBuf::from("/repo"), vec![before]);

    assert!(
        model.tab_task(tab).is_none(),
        "the task placed here is not listed yet, and nothing stands in for it"
    );

    let now = AgentView {
        id: "now".into(),
        created_at_unix: 2,
        ..task_in("/repo/.worktrees/ai", "now", WorkStateView::Running, 0)
    };
    model
        .remembered
        .tasks
        .get_mut(Path::new("/repo"))
        .unwrap()
        .push(now);
    assert_eq!(
        model.tab_task(tab).map(|task| task.id.as_str()),
        Some("now")
    );
}

/// An agent's caption says what runs there, and never the branch: a
/// task's label is derived from its branch, so the caption used to
/// repeat the name above it — `agent/t1` under `t1` for work nobody
/// named, `fix/thing` under `thing` for work somebody did.
#[test]
fn an_agents_caption_names_the_harness_running_it_never_its_branch() {
    let model = agent_with_task(WorkStateView::Running, 0);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let caption = agent_rows(&hits)[1] as usize;
    assert!(
        rows[caption].contains("agent") && !rows[caption].contains("agent/t1"),
        "the harness id, not the branch: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("agent/t1")),
        "and the branch is nowhere in the column: {rows:#?}"
    );
}

/// The foreground the caption under the agent labelled `agent` — the
/// row beneath its name — is drawn in, checked to be captioning `text`.
fn caption_color_of(model: &WorkspaceModel, agent: &str, text: &str) -> Color {
    let Sidebar { buffer, rows, .. } = sidebar(model, &identities_fixture());
    let row = rows
        .iter()
        .position(|row| row.contains(agent))
        .unwrap_or_else(|| panic!("{agent} is named in the tree: {rows:?}"))
        + 1;
    let offset = rows[row]
        .find(text)
        .unwrap_or_else(|| panic!("{text} captions {agent}: {rows:?}"));
    // A byte offset is not a column once the caption holds small caps
    // or subscript digits: one cell, several bytes.
    let column = rows[row][..offset].chars().count();
    buffer[(column as u16, row as u16)].fg
}

/// Every agent has a slot, so a slot is nothing to announce: neither
/// the `.worktrees/<id>` tail — two more segments in a column this
/// narrow — nor a mark of its own on the name row.
#[test]
fn an_agent_in_a_slot_is_left_unmarked_and_says_where_nowhere() {
    let model = agent_session_in("/repo/.worktrees/ai");
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let caption = &rows[agent_rows(&hits)[1] as usize];
    assert!(caption.contains("agent"), "what runs there: {caption}");
    assert!(!caption.contains(".worktrees"), "{caption}");
    assert!(!caption.contains("/repo"), "{caption}");
    assert!(
        !caption.contains('\u{22d4}'),
        "one mark, not two: {caption}"
    );
}

/// The column in front of an agent's name answers one question — how
/// that agent is doing — so which agent the keystrokes reach is left
/// to the caption's hue, wherever that agent stands: a slot of its
/// own or the operator's tree, both read the same way.
#[test]
fn the_agent_receiving_keystrokes_is_the_one_captioned_in_the_warning_hue() {
    let mut model = two_agent_session("/repo/.worktrees/ai", "/repo/src");
    let rows = sidebar(&model, &identities_fixture()).rows;
    let name_row = rows
        .iter()
        .find(|row| row.contains("Agent"))
        .expect("the agent is named in the tree");
    assert!(
        name_row.contains('\u{25cb}') || name_row.contains('\u{25cf}'),
        "the status glyph still leads: {name_row}"
    );
    assert_eq!(
        caption_color_of(&model, "Agent", "agent"),
        theme::color(Token::StateWarning),
        "a slot is no exception — the selected agent wears the hue"
    );
    assert_eq!(
        caption_color_of(&model, "Second", "agent"),
        theme::color(Token::TextDim),
        "and every other agent stays dim, slot or not"
    );

    select_second_agent(&mut model);
    assert_eq!(
        caption_color_of(&model, "Agent", "agent"),
        theme::color(Token::TextDim)
    );
    assert_eq!(
        caption_color_of(&model, "Second", "agent"),
        theme::color(Token::StateWarning)
    );
}

/// An agent outside any slot reads as one inside it — what runs
/// there, and nothing about where. The branch its directory was
/// evaluated on belongs to the space, which says it once.
#[test]
fn an_agent_outside_any_slot_reads_as_one_inside_it() {
    let mut model = agent_session_in("/repo/src");
    model
        .remembered
        .branches
        .insert(PathBuf::from("/repo/src"), "feature/x".into());
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let caption = &rows[agent_rows(&hits)[1] as usize];
    assert!(caption.contains("agent"), "what runs there: {caption}");
    assert!(
        !caption.contains("feature/x") && !caption.contains("/repo/src"),
        "neither the branch nor the directory: {caption}"
    );
}

/// The column spends no width on saying an agent is in its space: the
/// header's fold sits against the gutter with its name just after,
/// and an agent in that space's own root puts its status glyph in the
/// fold's column and its name in the header's, with its caption under
/// that name — the same in either kind of space.
///
/// One column further for agents isolated in checkouts of their own,
/// and that is the only indent in the column: the group that is
/// somewhere else is the only one with somewhere else to be. A group
/// of one takes no step — it has no trunk to hang off — and the blank
/// row above it is what sets it apart.
#[test]
fn agents_sit_one_step_inside_their_space() {
    let column_of = |row: &str, text: &str| {
        let byte = row
            .find(text)
            .unwrap_or_else(|| panic!("{text:?} in {row:?}"));
        row[..byte].chars().count()
    };
    use crate::ui::theme::Symbol;
    let idle = theme::glyph(Symbol::StatusIdle);
    let tree = three_spaces();
    let Sidebar { rows, hits, .. } = sidebar(&tree, &identities_fixture());
    let header = space_header(&hits, SpaceId(1)).y as usize;
    let name = column_of(&rows[header], "one");
    let fold = column_of(&rows[header], &theme::glyph(Symbol::ChevronExpanded));
    assert_eq!(column_of(&rows[header + 1], &idle), fold, "{rows:?}");
    let agent = column_of(&rows[header + 1], "shell");
    assert_eq!(agent, name, "{rows:?}");
    assert_eq!(column_of(&rows[header + 2], "agent"), agent, "{rows:?}");

    let flat = agents_in_the_root_session();
    let Sidebar { rows, hits, .. } = sidebar(&flat, &identities_in_the_root());
    let header = space_header(&hits, SpaceId(1)).y as usize;
    let name = column_of(&rows[header], "repo");
    let fold = column_of(&rows[header], &theme::glyph(Symbol::ChevronExpanded));
    assert_eq!(column_of(&rows[header + 1], &idle), fold, "{rows:?}");
    assert_eq!(column_of(&rows[header + 1], "agent 1"), name, "{rows:?}");
    assert_eq!(column_of(&rows[header + 2], "claude"), name, "{rows:?}");

    let both = a_space_with_both_groups();
    let Sidebar { rows, hits, .. } = sidebar(&both, &identities_in_the_root());
    let agents = agent_rows(&hits);
    let root = column_of(&rows[agents[0] as usize], "agent 1");
    let lone = column_of(&rows[agents[2] as usize], "agent 2");
    assert_eq!(lone, root, "a group of one takes no step: {rows:?}");

    let isolated = a_space_of_isolated_agents();
    let Sidebar { rows, hits, .. } = sidebar(&isolated, &identities_in_the_root());
    let header = space_header(&hits, SpaceId(1)).y as usize;
    let name = column_of(&rows[header], "repo");
    let agents = agent_rows(&hits);
    assert_eq!(
        column_of(&rows[agents[0] as usize], "agent 1"),
        name + 1,
        "a step further in: {rows:?}"
    );
    assert_eq!(
        column_of(&rows[agents[1] as usize], "claude"),
        name + 1,
        "and its caption keeps under its name: {rows:?}"
    );
}

/// Walking away from an agent and coming back returns to the tab it
/// was left on. A space holds one selection, so a shell opened beside
/// an agent used to be forgotten the moment the user looked at
/// another agent — they came back to the agent's own tab and had to
/// find their shell again in the strip.
#[test]
fn an_agent_is_re_entered_on_the_tab_it_was_left_on() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-strip-memory"));
    let (mut model, first, second) = two_agents_with_shells();
    let shell = model.session.as_ref().expect("session").workspace.spaces[0]
        .tabs
        .iter()
        .find(|tab| tab.agent == Some(first))
        .expect("the first agent has a shell")
        .id;

    // Left working in the first agent's shell, then away to the second.
    for tab in [shell, second] {
        let mut session = model.session.clone().expect("session");
        session.select_tab(tab);
        model.apply(
            ClientEvent::SessionUpdated { session },
            &identities_fixture(),
        );
    }

    let mut driven = driven(model, &home);
    driven.frame();
    let layout = compute_layout(Rect::new(0, 0, 80, 24), driven.attach.model.sidebar_width);
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(rect, hit)| {
            rect.x < layout.sidebar.right()
                && matches!(hit, WorkspaceHit::SelectTab(tab) if *tab == first)
        })
        .map(|(rect, _)| *rect)
        .expect("the first agent has a sidebar row");
    driven.press(row.x + 4, row.y);

    assert!(
        driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::SelectTab { tab } if *tab == shell)),
        "the shell it was left in, not the agent tab"
    );
}

/// A harness started by hand in the shell an agent was left on makes
/// that shell an agent of its own. Re-entering the first agent then
/// means the first agent: following the remembered shell would land
/// every click on its row in the other agent, for good.
#[test]
fn a_shell_that_became_an_agent_is_not_where_its_first_agent_is_re_entered() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-strip-shell-agent"));
    let (mut model, first, second) = two_agents_with_shells();
    let shell = model.session.as_ref().expect("session").workspace.spaces[0]
        .tabs
        .iter()
        .find(|tab| tab.agent == Some(first))
        .expect("the first agent has a shell")
        .id;

    let mut session = model.session.clone().expect("session");
    session.select_tab(shell);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    let mut session = model.session.clone().expect("session");
    for tab in &mut session.workspace.spaces[0].tabs {
        if tab.id == shell {
            tab.pane.process = "agent".into();
        }
    }
    session.select_tab(second);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );

    let mut driven = driven(model, &home);
    driven.frame();
    let layout = compute_layout(Rect::new(0, 0, 80, 24), driven.attach.model.sidebar_width);
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(rect, hit)| {
            rect.x < layout.sidebar.right()
                && matches!(hit, WorkspaceHit::SelectTab(tab) if *tab == first)
        })
        .map(|(rect, _)| *rect)
        .expect("the first agent has a sidebar row");
    driven.press(row.x + 4, row.y);

    assert!(
        driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::SelectTab { tab } if *tab == first)),
        "the first agent's own tab, not the agent started in its shell"
    );
}

/// The same click, when the user is already inside that agent: it
/// means the agent's own tab, and the strip is right there for
/// anything else.
#[test]
fn clicking_the_agent_you_are_already_in_selects_the_agent_itself() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-strip-same-agent"));
    let (mut model, first, _second) = two_agents_with_shells();
    let shell = model.session.as_ref().expect("session").workspace.spaces[0]
        .tabs
        .iter()
        .find(|tab| tab.agent == Some(first))
        .expect("the first agent has a shell")
        .id;
    let mut session = model.session.clone().expect("session");
    session.select_tab(shell);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );

    let mut driven = driven(model, &home);
    driven.frame();
    let layout = compute_layout(Rect::new(0, 0, 80, 24), driven.attach.model.sidebar_width);
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(rect, hit)| {
            rect.x < layout.sidebar.right()
                && matches!(hit, WorkspaceHit::SelectTab(tab) if *tab == first)
        })
        .map(|(rect, _)| *rect)
        .expect("the first agent has a sidebar row");
    driven.press(row.x + 4, row.y);

    assert!(
        driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::SelectTab { tab } if *tab == first)),
        "the agent's own tab"
    );
}
