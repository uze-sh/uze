//! The space tree: spaces and their rows, folding, the gutter, scrolling,
//! the reads it asks for and the column's width.

use super::WorkspaceHit;
use super::support::*;
use super::{
    ScrollDirection, WorkspaceModel,
    render::{FrameMetrics, render_sidebar},
    scroll_tree, space_cwd, toggle_space_collapsed,
};
use crate::ui::theme::{self, Token};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::{Path, PathBuf};
use uze_core::UzeHome;
use uze_terminal::{ClientRequest, SpaceId};

#[test]
fn only_the_active_spaces_agent_carries_the_selected_dot() {
    // A background space keeps a `selected_tab` of its own — where it
    // would resume, not where the user is. Drawing the dot from that
    // alone gave the sidebar one "this is the agent you are talking to"
    // per open space.
    let mut session = session("/tmp");
    session.workspace.spaces[0].tabs[0].label = "Agent".into();
    session.workspace.spaces[0].tabs[0].pane.process = "agent".into();
    session.create_space(
        Some("second".into()),
        uze_terminal::SpaceSeat {
            root: "/tmp/second".into(),
        },
        80,
        24,
    );
    session.workspace.spaces[1].tabs[0].label = "Agent".into();
    session.workspace.spaces[1].tabs[0].pane.process = "agent".into();
    let model = model_of(session);

    let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                &model,
                &identities_fixture(),
                &mut hits,
                &mut FrameMetrics::default(),
            )
        })
        .unwrap();
    let glyphs = |needle: &str| {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|cell| cell.symbol() == needle)
            .count()
    };
    assert_eq!(glyphs("\u{25cf}"), 1, "one selected agent, sidebar-wide");
    assert_eq!(glyphs("\u{25cb}"), 1, "the other space's agent reads idle");
}

/// A space header says what the space is called, and the fold beside
/// it is what asks where that is: minimized, the caption under the
/// name says the directory, pinned to the row's right edge like every
/// other caption in the column. The whole row still selects the space
/// — the header carries no control of its own but the fold.
#[test]
fn the_fold_is_what_asks_a_space_where_it_is() {
    let mut model = agent_session_in("/repo");
    // A name the root does not contain, so each reads as itself alone.
    let label = "workbench".to_owned();
    let space = {
        let session = model.session.as_mut().unwrap();
        let id = session.workspace.selected_space;
        session.workspace.spaces[0].label = label.clone();
        id
    };

    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let header = space_header(&hits, space);
    let open = rows[header.y as usize].clone();
    assert!(open.contains(&label) && !open.contains("/repo"), "{open}");
    assert!(
        !rows.iter().any(|row| row.contains('\u{21c4}')),
        "and nothing on it offers to flip it: {rows:?}"
    );
    assert!(
        hits.iter().any(|(rect, hit)| {
            matches!(hit, WorkspaceHit::SelectSpace(id) if *id == space) && rect.width > 1
        }),
        "the row itself selects the space"
    );

    toggle_space_collapsed(&mut model, space);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let header = space_header(&hits, space);
    let caption = rows[header.y as usize + 1].clone();
    let at = caption.find("/repo");
    assert!(at.is_some(), "minimized, it says where: {caption}");
    // Against the row's trailing column, the one every caption in the
    // sidebar ends in, whatever the path's length.
    // By column, not by byte: the gutter ahead of it is three bytes
    // wide and one column.
    let at = at.expect("asserted above");
    let end = (caption[..at].chars().count() + "/repo".chars().count()) as u16;
    assert_eq!(
        end,
        header.right() - crate::ui::widget::TRAILING_PAD,
        "pinned to the right edge: {rows:?}"
    );
}

/// A tree taller than the column scrolls rather than ending wherever
/// the column ran out: the spaces past the foot — under a long tree,
/// or under the timeline that holds that foot — were unreachable, not
/// merely out of view.
#[test]
fn the_space_tree_scrolls_to_what_the_column_cannot_show() {
    let mut session = session("/tmp");
    session.workspace.spaces[0].tabs[0].label = "Agent".into();
    session.workspace.spaces[0].tabs[0].pane.process = "agent".into();
    for index in 1..8 {
        session.create_space(
            Some(format!("space {index}")),
            uze_terminal::SpaceSeat {
                root: format!("/tmp/{index}").into(),
            },
            80,
            24,
        );
        session.workspace.spaces[index].tabs[0].label = "Agent".into();
        session.workspace.spaces[index].tabs[0].pane.process = "agent".into();
    }
    let mut model = model_of(session);

    let Sidebar { rows, metrics, .. } = sidebar(&model, &identities_fixture());
    assert!(metrics.tree_overflow > 0, "the tree outgrows the column");
    assert!(
        !rows.iter().any(|row| row.contains("space 7")),
        "the last space starts past the foot: {rows:?}"
    );

    model.tree_overflow = metrics.tree_overflow;
    for _ in 0..metrics.tree_overflow {
        scroll_tree(&mut model, ScrollDirection::Down);
    }
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(
        rows.iter().any(|row| row.contains("space 7")),
        "scrolled to the foot: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("space 1")),
        "the head scrolled out of view: {rows:?}"
    );

    scroll_tree(&mut model, ScrollDirection::Down);
    assert_eq!(
        model.remembered.tree_scroll, metrics.tree_overflow,
        "the wheel stops at the foot"
    );
    for _ in 0..=metrics.tree_overflow {
        scroll_tree(&mut model, ScrollDirection::Up);
    }
    assert_eq!(
        model.remembered.tree_scroll, 0,
        "and comes back to the head"
    );
}

/// No history, no section — a checkout with nothing committed, or no
/// checkout at all, leaves the column to the spaces.
#[test]
fn without_history_the_sidebar_ends_with_the_spaces() {
    let model = agent_session_in("/repo");
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());

    assert!(rows.iter().all(|row| !row.contains("timeline")), "{rows:?}");
    assert_eq!(timeline_hit(&hits), None);
}

/// A space's own row lands on a shell of the space's, not on whichever
/// agent the strip was showing: it is the way back to the space's
/// shells. A space whose first shell became an agent when a harness
/// was typed into it has no such tab, so the click opens one — the
/// space ends where "new" leaves it, not bound to the agent.
#[test]
fn a_space_row_lands_on_its_own_shell_and_otherwise_opens_one() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-row"));
    let click_space_row = |model: WorkspaceModel| {
        let mut driven = driven(model, &home);
        driven.frame();
        let (row, space) = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| match hit {
                WorkspaceHit::SelectSpace(space) => Some((*rect, *space)),
                _ => None,
            })
            .expect("the space row is a target");
        driven.press(row.x + 4, row.y);
        let sent = driven.sent();
        (space, sent)
    };

    let mut with_shell = session("/repo");
    let space = with_shell.selected_space().id;
    let shell = with_shell.selected_tab().id;
    let agent_pane = with_shell.add_tab(
        space,
        "agent 1".into(),
        None,
        80,
        24,
        PathBuf::from("/repo"),
    );
    with_shell.update_pane_status(agent_pane, PathBuf::from("/repo"), "agent".into());
    let (_, sent) = click_space_row(model_of(with_shell));
    assert!(
        sent.iter()
            .any(|request| matches!(request, ClientRequest::SelectTab { tab } if *tab == shell)),
        "the space's own shell was not selected: {sent:?}"
    );

    let mut only_agents = session("/repo");
    let pane = only_agents.selected_tab().pane.id;
    only_agents.update_pane_status(pane, PathBuf::from("/repo"), "agent".into());
    let (space, sent) = click_space_row(model_of(only_agents));
    let switched = sent.iter().position(|request| {
        matches!(request, ClientRequest::SelectSpace { space: selected } if *selected == space)
    });
    let opened = sent.iter().position(|request| {
        matches!(
            request,
            ClientRequest::CreateTab { agent: None, command: None, cwd: Some(cwd), .. }
                if cwd == Path::new("/repo")
        )
    });
    assert!(
        matches!((switched, opened), (Some(switched), Some(opened)) if switched < opened),
        "a space of agents alone was not switched to and given a shell of its own: {sent:?}"
    );
}

/// The selected space alone carries "new", at its header's right
/// edge, and it opens the agent picker under itself: the new agent
/// lands in the space in front, so no other header offers one.
#[test]
fn only_the_selected_space_header_offers_a_new_agent() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-new-agent"));
    let mut driven = driven(three_spaces(), &home).on_a_roomy_terminal();
    driven.frame();
    let hits = driven.attach.model.hits.clone();
    let offered: Vec<Rect> = hits
        .iter()
        .filter(|(_, hit)| *hit == WorkspaceHit::NewAgentMenu)
        .map(|(rect, _)| *rect)
        .collect();
    let header = space_header(&hits, SpaceId(3));
    assert_eq!(offered.len(), 1, "one control, whatever the spaces number");
    let new = offered[0];
    assert_eq!(new.y, header.y, "on the selected space's header");
    assert_eq!(
        new.right() + crate::ui::widget::TRAILING_PAD,
        header.right(),
        "one pad off the divider"
    );

    driven.press(new.x, new.y);

    let picker = driven
        .attach
        .model
        .agent_picker
        .as_ref()
        .expect("the picker opened");
    assert_eq!(picker.anchor, new, "anchored under the control clicked");
}

/// One lifted block in a transparent column: the space in front stands
/// on the panel, and every other space, open or minimized, stands on the
/// column itself. The lift is what says which space is in front.
#[test]
fn only_the_space_in_front_is_lifted() {
    let mut model = three_spaces();
    model.first_steps_collapsed = true;
    // One minimized, one open, neither of them the one in front.
    toggle_space_collapsed(&mut model, SpaceId(1));
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&model, &identities_fixture());
    let ground = |space: SpaceId, offset: u16| {
        let header = space_header(&hits, space);
        buffer[(header.x + 2, header.y + offset)].bg
    };
    let column = theme::color(Token::SurfaceBackground);

    for (space, offset) in [(SpaceId(1), 0), (SpaceId(2), 0), (SpaceId(2), 1)] {
        assert_eq!(
            ground(space, offset),
            column,
            "a space behind stands on the column: {rows:?}"
        );
    }
    assert_eq!(
        ground(SpaceId(3), 0),
        theme::color(Token::SurfaceRaised),
        "the space in front is lifted: {rows:?}"
    );
}

/// A space's own row is a target like any other: when it is the row
/// in front — its shell selected, or the space folded to its header —
/// it wears the same selection a selected agent row wears. With an agent in front it is the
/// plain surface again, because then the trace belongs to that agent.
#[test]
fn the_spaces_own_row_wears_the_agents_trace_when_it_is_the_one_in_front() {
    let in_front = model_of(session("/repo"));
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&in_front, &identities_fixture());
    let header = space_header(&hits, SpaceId(1));
    assert_eq!(
        buffer[(header.x + 2, header.y)].bg,
        theme::color(Token::SurfaceSelected),
        "the space's own row is the one selected, in the one overlay: {rows:?}"
    );

    let behind = agents_in_the_root_session();
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&behind, &identities_in_the_root());
    let header = space_header(&hits, SpaceId(1));
    assert_eq!(
        buffer[(header.x + 2, header.y)].bg,
        theme::color(Token::SurfaceRaised),
        "an agent is in front, so the header is the plain surface: {rows:?}"
    );
}

/// A minimized space is two rows — its header and the caption saying
/// where its work is — and they are one item: when that item is the
/// one in front, the trace runs through both rows rather than
/// stopping halfway down it.
#[test]
fn a_minimized_space_in_front_is_lit_down_both_of_its_rows() {
    let mut model = agents_in_the_root_session();
    let root = model.session.as_ref().expect("session").workspace.spaces[0]
        .root
        .clone();
    model.collapsed_space_roots.insert(root);
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&model, &identities_in_the_root());
    let header = space_header(&hits, SpaceId(1));

    let overlay = theme::color(Token::SurfaceSelected);
    assert_eq!(
        buffer[(header.x + 2, header.y)].bg,
        overlay,
        "the header is the row in front: {rows:?}"
    );
    assert_eq!(
        buffer[(header.x + 2, header.y + 1)].bg,
        overlay,
        "and the caption under it carries the same one, with no step \
         between the two rows of one item: {rows:?}"
    );
}

fn agent_rows_of(model: &WorkspaceModel, hits: &[(Rect, WorkspaceHit)], space: SpaceId) -> usize {
    let session = model.session.as_ref().unwrap();
    let space = session
        .workspace
        .spaces
        .iter()
        .find(|candidate| candidate.id == space)
        .unwrap();
    hits.iter()
        .filter(|(_, hit)| {
            matches!(hit, WorkspaceHit::SelectTab(tab) if space.tabs.iter().any(|candidate| candidate.id == *tab))
        })
        .count()
}

/// A blank row beside every expanded space, and over the first one
/// whatever it is — and none between two minimized ones, the one in
/// front included, whose caption does not make it a block of its own.
///
/// The fill alone was tried: every card but the one in front is
/// faded, so where two faded cards meet there is no edge to find, and
/// with a column of them it reads as one surface with headers in it
/// rather than as a list of blocks. Between two minimized headers a
/// row of nothing spends half the column on the spaces nobody is
/// looking into.
///
/// Measured over a column that mixes both shapes, because they are
/// drawn by different arms of the same loop.
#[test]
fn the_row_between_two_spaces_closes_when_both_are_minimized() {
    let mut model = three_spaces();
    model.first_steps_collapsed = true;
    // `three_spaces` leaves the third in front, so minimizing the
    // other two leaves each of them a single row.
    toggle_space_collapsed(&mut model, SpaceId(1));
    toggle_space_collapsed(&mut model, SpaceId(2));
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let at: Vec<u16> = (1..=3)
        .map(|id| space_header(&hits, SpaceId(id)).y)
        .collect();

    assert!(
        without_chrome(&rows[at[0] as usize - 1]).is_empty(),
        "the column's own top margin: {rows:?}"
    );
    assert_eq!(
        at[1] - at[0],
        1,
        "two single rows stand one under the other: {rows:?}"
    );
    assert_eq!(
        at[2] - at[1],
        2,
        "and the row opens again over the one that grew: {rows:?}"
    );
    assert!(
        without_chrome(&rows[at[2] as usize - 1]).is_empty(),
        "which says nothing: {rows:?}"
    );

    // Opened, it is no longer a single line and takes its row back.
    toggle_space_collapsed(&mut model, SpaceId(2));
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let at: Vec<u16> = (1..=2)
        .map(|id| space_header(&hits, SpaceId(id)).y)
        .collect();
    assert_eq!(at[1] - at[0], 2, "a row over it again: {rows:?}");

    // Minimized while it is the one in front, a space keeps its
    // caption row and still packs against its minimized neighbours.
    toggle_space_collapsed(&mut model, SpaceId(2));
    toggle_space_collapsed(&mut model, SpaceId(3));
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let at: Vec<u16> = (1..=3)
        .map(|id| space_header(&hits, SpaceId(id)).y)
        .collect();
    assert_eq!(at[2] - at[1], 1, "the one in front packs too: {rows:?}");
}

/// Exactly one name in the column is bright and bold, and it is
/// whatever is receiving keystrokes: an agent, or the header of a
/// space that is minimized or speaks for no agent of its own.
///
/// The hue used to follow the space's own context agent while the
/// weight followed the *current* one, and every space names a context
/// agent — the ones nobody is in included. Walking from one space to
/// another left the first one's agent bright and merely unbolded, so
/// two rows claimed to be where the keyboard was.
#[test]
fn one_name_in_the_column_is_bright_and_bold() {
    // By hue alone, and the weight asserted on what it finds: the
    // defect this is here for was a name left *bright* and merely
    // unbolded, which a search for both at once walks straight past.
    let bright = |model: &WorkspaceModel| {
        let Sidebar { rows, buffer, .. } = sidebar(model, &identities_fixture());
        let hue = theme::color(Token::TextBright);
        let lit: Vec<(u16, String)> = (0..buffer.area.height)
            .filter(|row| {
                (0..buffer.area.width).any(|column| {
                    let cell = &buffer[(column, *row)];
                    cell.fg == hue && cell.symbol() != " "
                })
            })
            .map(|row| (row, rows[row as usize].trim().to_owned()))
            .collect();
        for (row, _) in &lit {
            assert!(
                (0..buffer.area.width).any(|column| {
                    let cell = &buffer[(column, *row)];
                    cell.fg == hue && cell.modifier.contains(ratatui::style::Modifier::BOLD)
                }),
                "bright and not bold is the half-state this forbids: {rows:?}"
            );
        }
        (rows, lit)
    };

    let mut model = three_spaces();
    model.first_steps_collapsed = true;
    // `three_spaces` leaves the third in front, on its own agent.
    let (rows, lit) = bright(&model);
    assert_eq!(lit.len(), 1, "one row, not one per space: {rows:?}");
    assert!(lit[0].1.contains("shell"), "the agent in front: {rows:?}");

    // Walking to another space moves it rather than adding to it.
    model
        .session
        .as_mut()
        .expect("a session")
        .workspace
        .selected_space = SpaceId(1);
    let (rows, moved) = bright(&model);
    assert_eq!(moved.len(), 1, "still one: {rows:?}");
    assert_ne!(
        moved[0].0, lit[0].0,
        "and it moved rather than multiplied: {rows:?}"
    );
    assert!(
        moved[0].1.contains("shell"),
        "onto the other agent: {rows:?}"
    );

    // Minimized, the space speaks for itself and its header takes it.
    toggle_space_collapsed(&mut model, SpaceId(1));
    let (rows, folded) = bright(&model);
    assert_eq!(folded.len(), 1, "one row: {rows:?}");
    assert!(
        folded[0].1.contains("one"),
        "the header of the space in front: {rows:?}"
    );
}

/// The name of the space in front is bold whether or not its header
/// is the row receiving keystrokes — which, for a space of nothing
/// but agents, it can never be: there is no tab of its own to land
/// on, so clicking its header changed nothing a reader could see.
/// The weight answers which block, where the hue answers which row.
#[test]
fn the_name_of_the_space_in_front_is_bold_even_when_an_agent_is_current() {
    let model = three_spaces();
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&model, &identities_fixture());
    let weight = |space: SpaceId, name: &str| {
        let header = space_header(&hits, space);
        let column = rows[header.y as usize]
            .find(name)
            .map(|byte| rows[header.y as usize][..byte].chars().count())
            .unwrap_or_else(|| panic!("{name} heads its space: {rows:?}"))
            as u16;
        let cell = &buffer[(column, header.y)];
        (
            cell.modifier.contains(ratatui::style::Modifier::BOLD),
            cell.fg,
        )
    };

    // The third is in front, on an agent of its own, so its header is
    // not the current row — and its name is bold all the same.
    let (bold, hue) = weight(SpaceId(3), "three");
    assert!(bold, "the space in front: {rows:?}");
    assert_ne!(
        hue,
        theme::color(Token::TextBright),
        "without taking the hue off the agent: {rows:?}"
    );
    let (bold, _) = weight(SpaceId(1), "one");
    assert!(!bold, "and no other space's name is: {rows:?}");
}

/// The leading column carries a rail beside the selected space and
/// nothing beside any other: down that one block, in the faintest
/// tone, with the accent along the stretch that is selected within
/// it. A space nobody is in leaves the column blank — the fill under
/// its rows already says it is a block, and a rail per space said the
/// same thing three times over.
#[test]
fn the_gutter_marks_the_selected_space_and_no_other() {
    let mut model = three_spaces();
    // All three spaces in the column, with the steps folded at the foot.
    model.first_steps_collapsed = true;
    let Sidebar {
        rows, hits, buffer, ..
    } = sidebar(&model, &identities_fixture());
    use crate::ui::theme::{Symbol, Token};
    let rail = theme::glyph(Symbol::BarThin);
    let resting = theme::color(Token::TextFaint);
    let lit = theme::color(Token::Accent);
    // Space 1 is in the background; space 3 is selected, on its agent.
    for (space, drawn) in [(SpaceId(1), None), (SpaceId(3), Some([resting, lit, lit]))] {
        let header = space_header(&hits, space);
        // The header, then the agent's two rows.
        let (column, span) = (header.x, header.y..header.y + 3);
        for (row, index) in span.clone().zip(0..) {
            let cell = &buffer[(column, row)];
            match drawn {
                Some(hues) => {
                    assert_eq!(cell.symbol(), rail, "row {row} of {space:?}: {rows:?}");
                    assert_eq!(cell.fg, hues[index], "row {row} of {space:?}: {rows:?}");
                }
                None => assert_eq!(cell.symbol(), " ", "row {row} of {space:?}: {rows:?}"),
            }
        }
    }

    // The fill runs under the rail: the line is drawn inside the block
    // rather than alongside it. Filling up to the line and no further
    // left the line sitting on the column's own background, which
    // reads as a decoration outside the card with a gap between them
    // — the card's edge is where the fill ends, and the fill has to
    // end past the line for the line to be in it.
    let header = space_header(&hits, SpaceId(3));
    for row in header.y..header.y + 3 {
        assert_eq!(
            buffer[(header.x, row)].bg,
            buffer[(header.x + 1, row)].bg,
            "row {row}: the rail sits outside the block's fill: {rows:?}"
        );
    }
    // The rail and the fill say the same thing: the space behind has
    // neither (see `only_the_space_in_front_is_lifted`).
    let outside = space_header(&hits, SpaceId(1));
    assert_ne!(
        buffer[(outside.x + 2, outside.y)].bg,
        buffer[(header.x + 2, header.y)].bg,
        "only the header in front is lifted: {rows:?}"
    );
}

/// A space is where its own shell is: a `cd` there moves what the space
/// names and what its agents are placed from. With no shell of its own
/// — every tab an agent — it is the root it was opened at.
#[test]
fn a_cd_in_a_spaces_own_shell_moves_the_space() {
    let mut session = session("/repo");
    let space = session.workspace.selected_space;
    let shell = session.workspace.spaces[0].tabs[0].pane.id;
    let agent = session.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
    session.update_pane_status(agent, "/repo".into(), "claude".into());
    assert_eq!(
        space_cwd(&session.workspace.spaces[0], &identities_in_the_root()),
        PathBuf::from("/repo")
    );

    session.update_pane_status(shell, "/repo/services/api".into(), "bash".into());
    let moved = PathBuf::from("/repo/services/api");
    assert_eq!(
        space_cwd(&session.workspace.spaces[0], &identities_in_the_root()),
        moved,
        "the space followed its own shell"
    );

    let mut model = model_of(session.clone());
    toggle_space_collapsed(&mut model, space);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
    let header = space_header(&hits, space).y as usize;
    assert!(
        rows[header + 1].contains("api"),
        "the caption names where it is now: {rows:?}"
    );

    // Nothing of its own left to follow: the root it was opened at.
    session.update_pane_status(shell, "/repo/services/api".into(), "claude".into());
    assert_eq!(
        space_cwd(&session.workspace.spaces[0], &identities_in_the_root()),
        PathBuf::from("/repo"),
        "every tab an agent, so the space is its root again"
    );
}

/// A space's root and each of its agents' directories are read as soon
/// as the sidebar names them — once each, answered or still asked.
#[test]
fn every_directory_the_sidebar_names_is_read_once() {
    let mut model = three_spaces();
    let unread = |model: &WorkspaceModel| {
        let mut unread = model.unread_named_directories(&identities_fixture());
        unread.sort();
        unread
    };
    assert_eq!(
        unread(&model),
        ["/one", "/three", "/two"].map(PathBuf::from).to_vec(),
        "each root, which is also where each agent runs"
    );

    model.remembered.evaluated.insert(PathBuf::from("/one"));
    model
        .remembered
        .task_eval_pending
        .insert(PathBuf::from("/two"));
    assert_eq!(unread(&model), vec![PathBuf::from("/three")]);

    let session = model.session.as_mut().unwrap();
    let space = session.workspace.selected_space;
    let pane = session.add_tab(space, "agent 2".into(), None, 80, 24, "/three".into());
    session.update_pane_status(pane, "/three/.worktrees/x".into(), "agent".into());
    model.remembered.evaluated.insert(PathBuf::from("/three"));
    assert!(
        unread(&model).is_empty(),
        "a slot is answered by its repository's one evaluation"
    );
}

/// A question asked while the same repository's read is in flight is
/// kept, not dropped: that read may have started before the push the
/// second question is about, and its answer would stand as the last
/// word — a `↑1` that stayed up after the push had gone through.
#[test]
fn a_read_asked_for_while_one_is_in_flight_is_asked_again() {
    let mut model = three_spaces();
    let home = UzeHome::at(uze_testkit::temp::scratch("sidebar-asked-again"));
    let (sender, _answers) = std::sync::mpsc::channel();
    model.schedule_evaluation(&home, PathBuf::from("/one"), &sender);
    assert!(model.remembered.task_eval_again.is_empty());

    model.schedule_evaluation(&home, PathBuf::from("/one"), &sender);
    assert_eq!(
        model.remembered.task_eval_again.get(Path::new("/one")),
        Some(&PathBuf::from("/one")),
        "the second question waits for the first answer"
    );
}

/// The refresh clock reads every space's header, once per repository,
/// not only the selected pane's.
#[test]
fn every_space_header_is_read_on_the_clock() {
    let model = three_spaces();
    let mut directories = model.space_directories(&identities_fixture());
    directories.sort();
    assert_eq!(
        directories,
        ["/one", "/three", "/two"].map(PathBuf::from).to_vec()
    );
}

/// The fold minimizes a space to its header and opens it again, on this
/// client alone: nothing is asked of the server.
#[test]
fn the_fold_minimizes_a_space_to_its_header_and_opens_it_again() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-fold"));
    let mut driven = driven(three_spaces(), &home);
    driven.frame();
    let one = SpaceId(1);
    assert_eq!(
        agent_rows_of(&driven.attach.model, &driven.attach.model.hits, one),
        2
    );
    let fold =
        driven.hit(|hit| matches!(hit, WorkspaceHit::ToggleSpaceCollapsed(space) if *space == one));

    driven.press(fold.x, fold.y);
    driven.frame();
    let hits = driven.attach.model.hits.clone();
    assert_eq!(
        agent_rows_of(&driven.attach.model, &hits, one),
        0,
        "the agent rows are folded away"
    );
    let two = space_header(&hits, SpaceId(2));
    assert_eq!(
        two.y,
        space_header(&hits, one).y + 2,
        "nobody is in it, so the next space follows its header and the blank row"
    );
    assert!(
        driven.sent().is_empty(),
        "folding is this client's own view"
    );

    driven.press(fold.x, fold.y);
    driven.frame();
    let hits = driven.attach.model.hits.clone();
    assert_eq!(
        agent_rows_of(&driven.attach.model, &hits, one),
        2,
        "and back"
    );
}

/// Folded, a space still says that something in it wants a look, and
/// its header — all there is of it — carries the selection.
#[test]
fn a_folded_space_says_through_its_header_what_its_agents_did() {
    let mut model = three_spaces();
    let three = model.session.as_ref().unwrap().workspace.selected_space;
    let pane = model.session.as_ref().unwrap().selected_tab().pane.id;
    model.remembered.completed_agent_panes.insert(pane);
    let completed = theme::glyph(crate::ui::theme::Symbol::StatusCompleted);
    let header_row = |model: &WorkspaceModel| {
        let Sidebar {
            rows, hits, buffer, ..
        } = sidebar(model, &identities_fixture());
        let header = space_header(&hits, three).y;
        let lit = lit_gutter_rows(&buffer, gutter_column(&hits)).contains(&header);
        (rows[header as usize].clone(), lit)
    };
    let (open, lit) = header_row(&model);
    assert!(
        !open.contains(&completed),
        "open, the agent row says it: {open:?}"
    );
    assert!(!lit, "and the agent carries the selection: {open:?}");

    toggle_space_collapsed(&mut model, three);
    let (folded, lit) = header_row(&model);
    assert!(folded.contains(&completed), "{folded:?}");
    assert!(lit, "{folded:?}");
}

/// A minimized space keeps a caption under its header saying where
/// its work is — the directory, never an agent and never the branch
/// its root is on, which its agents say for themselves once it is
/// open — and only while it is the space in front. Open over its
/// agents it has no caption at all.
#[test]
fn only_the_minimized_space_in_front_says_where_it_is() {
    let rows_at = |model: &WorkspaceModel, space: SpaceId| {
        let Sidebar { rows, hits, .. } = sidebar(model, &identities_fixture());
        let header = space_header(&hits, space).y as usize;
        (rows[header].clone(), rows[header + 1].clone())
    };
    let mut model = three_spaces();
    // `three_spaces` leaves the third in front.
    let (front, behind) = (SpaceId(3), SpaceId(1));

    let (_, open) = rows_at(&model, front);
    assert!(
        open.contains("shell"),
        "open, the agents follow the header: {open:?}"
    );

    toggle_space_collapsed(&mut model, front);
    let (_, row) = rows_at(&model, front);
    assert!(
        row.contains("/three") && !row.contains("shell"),
        "minimized and in front, the directory it is in: {row:?}"
    );

    // Known, the branch stays out of it: the row answers where, and a
    // branch name does not say where.
    model
        .remembered
        .branches
        .insert(PathBuf::from("/three"), "main".into());
    let (_, row) = rows_at(&model, front);
    assert!(
        row.contains("/three") && !row.contains("main"),
        "still the directory: {row:?}"
    );

    // Behind, the name is the whole of it: a path under every space
    // in a long column is a second column to read past.
    toggle_space_collapsed(&mut model, behind);
    let (header, under) = rows_at(&model, behind);
    assert!(header.contains("one"), "its name: {header:?}");
    assert!(!under.contains("/one"), "and nothing under it: {under:?}");
}

/// The scroll bound is measured with the folds, so a column of folded
/// spaces does not scroll past rows that are no longer drawn.
#[test]
fn a_folded_space_measures_its_header_alone() {
    let mut model = three_spaces();
    let session = model.session.as_mut().unwrap();
    for index in 4..12 {
        let root = PathBuf::from(format!("/{index}"));
        session.create_space(None, uze_terminal::SpaceSeat { root: root.clone() }, 80, 24);
        let pane = session.selected_tab().pane.id;
        session.update_pane_status(pane, root, "agent".into());
    }
    let open = sidebar(&model, &identities_fixture()).metrics.tree_overflow;
    toggle_space_collapsed(&mut model, SpaceId(1));
    let folded = sidebar(&model, &identities_fixture()).metrics.tree_overflow;
    assert!(open > 2, "the column overflows: {open}");
    assert_eq!(
        open - folded,
        2,
        "nobody is in it, so the agent's two rows give way to nothing"
    );
}

/// A fold is a preference, not a transient. Minimizing a space and
/// closing uze used to open it again on the next run: the fold lived
/// with what this client had *resolved*, which dies with the process,
/// instead of with what the operator *chose*, which is the layout both
/// surfaces share. It travels by root, because the identifier the
/// server minted for a space is minted again the next time it restores
/// the workspace.
#[test]
fn a_minimized_space_is_still_minimized_on_the_next_run() {
    let mut folded = three_spaces();
    let one = SpaceId(1);
    let root = folded.session.as_ref().unwrap().workspace.spaces[0]
        .root
        .clone();

    let (recorder, recorded) = std::sync::mpsc::channel();
    folded.layout_recorder = Some(recorder);

    toggle_space_collapsed(&mut folded, one);

    let kept = recorded.try_recv().expect("the fold is recorded").workspace;
    assert!(
        kept.collapsed_space_roots.contains(&root),
        "the fold is kept, by the root the space is over: {kept:?}"
    );

    // The next run: the same workspace, drawn by a model that knows
    // nothing but the layout the last one left behind.
    let mut next = three_spaces();
    next.collapsed_space_roots = kept.collapsed_space_roots.clone();
    assert_eq!(
        sidebar(&next, &identities_fixture()).rows,
        sidebar(&folded, &identities_fixture()).rows,
        "the column opens on the space still minimized"
    );
    assert_ne!(
        sidebar(&next, &identities_fixture()).rows,
        sidebar(&three_spaces(), &identities_fixture()).rows,
        "which is not the column a run that was told nothing draws"
    );

    let (recorder, recorded) = std::sync::mpsc::channel();
    next.layout_recorder = Some(recorder);
    toggle_space_collapsed(&mut next, one);
    assert!(
        recorded
            .try_recv()
            .expect("opening it again is recorded")
            .workspace
            .collapsed_space_roots
            .is_empty(),
        "and opening it again is kept too"
    );
}

/// A space is carried by its header: while it moves, a line shows where
/// it lands, and letting go there asks the server to put it there.
#[test]
fn a_space_dragged_by_its_header_lands_where_the_line_said() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-drag"));
    let mut driven = driven(three_spaces(), &home);
    driven.frame();
    let hits = driven.attach.model.hits.clone();
    let one = space_header(&hits, SpaceId(1));
    let three = space_header(&hits, SpaceId(3));

    driven.press(three.x + 4, three.y);
    let _ = driven.sent();
    driven.mouse(three.x + 4, one.y, MouseEventKind::Drag(MouseButton::Left));
    let rows = frame_rows(&mut driven.attach.model);
    let line = theme::glyph(crate::ui::theme::Symbol::TreeDivider).repeat(8);
    assert!(
        rows[one.y as usize - 1].contains(&line),
        "the line is drawn above the space it lands before: {rows:?}"
    );

    driven.mouse(three.x + 4, one.y, MouseEventKind::Up(MouseButton::Left));
    let sent = driven.sent();
    assert!(
        sent.iter().any(|request| matches!(
            request,
            ClientRequest::ReorderSpace { space, before: Some(before) }
                if *space == SpaceId(3) && *before == SpaceId(1)
        )),
        "{sent:?}"
    );
    assert!(driven.attach.model.dragging_space.is_none());
}

/// A header clicked, or barely moved, stays a click.
#[test]
fn a_click_on_a_space_header_moves_no_space() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-click"));
    let mut driven = driven(three_spaces(), &home);
    driven.frame();
    let three = space_header(&driven.attach.model.hits.clone(), SpaceId(3));

    driven.press(three.x + 4, three.y);
    driven.mouse(
        three.x + 4,
        three.y - 1,
        MouseEventKind::Drag(MouseButton::Left),
    );
    driven.mouse(
        three.x + 4,
        three.y - 1,
        MouseEventKind::Up(MouseButton::Left),
    );

    let sent = driven.sent();
    assert!(
        !sent
            .iter()
            .any(|request| matches!(request, ClientRequest::ReorderSpace { .. })),
        "{sent:?}"
    );
}

/// The space row is a context of its own — its shells — so while the
/// operator is there its gutter is lit, in either kind, and
/// gives it up once an agent of the space is selected.
#[test]
fn the_space_row_carries_the_bar_while_its_own_shells_are_selected() {
    let header_lit = |model: &WorkspaceModel| {
        let Sidebar { hits, buffer, .. } = sidebar(model, &identities_in_the_root());
        let header = hits
            .iter()
            .find(|(_, hit)| matches!(hit, WorkspaceHit::SelectSpace(_)))
            .map(|(rect, _)| rect.y)
            .expect("the space header");
        lit_gutter_rows(&buffer, gutter_column(&hits)).contains(&header)
    };
    let mut on_shell = session("/repo");
    let space = on_shell.selected_space().id;
    let shell = on_shell.selected_tab().id;
    let agent = on_shell.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
    on_shell.update_pane_status(agent, "/repo".into(), "claude".into());
    on_shell.workspace.spaces[0].selected_tab = shell;
    assert!(header_lit(&model_of(on_shell.clone())));

    let agent_tab = on_shell.workspace.spaces[0]
        .tabs
        .iter()
        .find(|tab| tab.pane.id == agent)
        .expect("the agent's tab")
        .id;
    on_shell.workspace.spaces[0].selected_tab = agent_tab;
    assert!(!header_lit(&model_of(on_shell)));
}

/// Where the divider was let go outlives the run, like the timeline's
/// own shape: both modes share this column, so both find it at the
/// width it was left. Kept on release, not through the drag — the
/// widths it swept past are not answers.
#[test]
fn the_dragged_sidebar_width_is_kept_for_the_next_run() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-sidebar-width"));
    let (recorder, recorded) = std::sync::mpsc::channel();
    let mut model = agent_session_in("/repo");
    model.layout_recorder = Some(recorder);
    let mut driven = driven(model, &home);
    driven.frame();
    let handle = driven.hit(|hit| matches!(hit, WorkspaceHit::ResizeSidebar));

    driven.press(handle.x, handle.y);
    driven.mouse(20, 5, MouseEventKind::Drag(MouseButton::Left));
    let dragged = driven.attach.model.sidebar_width;
    assert!(dragged.is_some(), "the drag moved the divider");
    assert!(recorded.try_recv().is_err(), "nothing is written mid-drag");

    driven.mouse(20, 5, MouseEventKind::Up(MouseButton::Left));

    let shape = recorded.try_recv().expect("the release is recorded");
    assert_eq!(shape.sidebar.width, dragged);
}
