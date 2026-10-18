//! The status marks and their catalog: one glyph and one hue per state,
//! and the sidebar states they are decided by.

use super::WorkspaceHit;
use super::support::*;
use super::{
    AgentTabStatus, WorkStateView,
    render::{render_status_catalog, task_mark},
};
use crate::ui::theme::{self, Token};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;
use uze_terminal::PaneId;

/// Every state a slot can be in, for the tests that have to cover the
/// whole table rather than one interesting case.
fn every_task_state() -> Vec<WorkStateView> {
    vec![
        WorkStateView::Running,
        WorkStateView::Uncommitted,
        WorkStateView::Ready,
        WorkStateView::Published,
        WorkStateView::Integrating,
        WorkStateView::Conflicted {
            files: vec![PathBuf::from("src/lib.rs")],
        },
        WorkStateView::GateFailed,
        WorkStateView::Integrated,
        WorkStateView::Shelved,
    ]
}

/// The marks are symbols, not emoji, and this is the narrow form of that
/// rule: a status mark is one column, so it is held to U+2300 — below
/// the blocks where the pictographs start — rather than merely to being
/// non-emoji.
///
/// `uze_theme::load::tests::no_bundled_glyph_is_an_emoji` is the wider
/// rule, over every glyph UZE ships rather than only these.
#[test]
fn no_status_mark_is_drawn_from_the_pictographic_blocks() {
    for state in every_task_state() {
        let Some((mark, _)) = task_mark(&state) else {
            continue;
        };
        let mut characters = mark.chars();
        let character = characters.next().expect("a mark is not empty");
        assert!(
            characters.next().is_none(),
            "{state:?}: one column, one character: {mark}"
        );
        assert!(
            (character as u32) < 0x2300,
            "{state:?}: {mark} (U+{:04X}) is a pictograph",
            character as u32
        );
    }
}

/// Color is what tells the marks apart at a glance — three states
/// sharing `theme::color(Token::TextDim)` meant the column read as one undifferentiated
/// smudge. The glyphs are distinct for the same reason, and `Ready`
/// specifically must not reuse the `✓` the agent column already spends
/// on `Completed`.
#[test]
fn each_status_mark_carries_a_glyph_and_a_hue_of_its_own() {
    let marks: Vec<(String, Color)> = every_task_state().iter().filter_map(task_mark).collect();
    for (index, (mark, hue)) in marks.iter().enumerate() {
        for (other_mark, other_hue) in &marks[index + 1..] {
            assert_ne!(mark, other_mark, "two states share a glyph");
            assert_ne!(
                hue, other_hue,
                "two states share a hue: {mark}/{other_mark}"
            );
        }
    }
    let agent_glyphs: Vec<String> = [
        AgentTabStatus::Completed,
        AgentTabStatus::Selected,
        AgentTabStatus::Idle,
    ]
    .into_iter()
    .map(|status| status.glyph(0).trim_end().to_owned())
    .collect();
    for (mark, _) in &marks {
        assert!(
            !agent_glyphs.iter().any(|glyph| glyph == mark),
            "{mark} means something else one column to the left"
        );
    }
}

/// The task mark is the one click target that opens the catalog: the
/// glyphs are the row's only wordless vocabulary, so the row has to
/// carry the way to look them up — once. The agent's own status glyph
/// in front of the name is not a second door to the same popup.
#[test]
fn only_the_task_mark_opens_the_catalog() {
    let model = agent_with_task(WorkStateView::Ready, 1);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let anchors: Vec<Rect> = hits
        .iter()
        .filter_map(|(_, hit)| match hit {
            WorkspaceHit::OpenStatusCatalog(anchor) => Some(*anchor),
            _ => None,
        })
        .collect();
    assert_eq!(anchors.len(), 1, "one door, the task mark: {rows:?}");
    // The anchor is the glyph's own cell, not the row: a click on the
    // name still selects the tab. Read back out of the drawn rows, so
    // a hit that drifts off its glyph fails here rather than opening
    // the catalog from a blank column.
    let anchor = anchors[0];
    assert_eq!((anchor.width, anchor.height), (1, 1));
    let glyph = rows[anchor.y as usize]
        .chars()
        .nth(anchor.x as usize)
        .expect("the anchor is inside the row")
        .to_string();
    let (ready, _) = task_mark(&WorkStateView::Ready).expect("ready is marked");
    assert_eq!(glyph, ready, "the anchor is the task mark: {rows:?}");
    assert!(
        hits.iter()
            .any(|(rect, hit)| matches!(hit, WorkspaceHit::SelectTab(_)) && rect.width > 1),
        "the row itself still selects the tab"
    );
}

/// The task mark stands at the row's right edge, the one trailing
/// column every row in the sidebar keeps, and the row no longer spends
/// that edge on naming the harness.
#[test]
fn a_task_mark_is_pinned_to_the_sidebars_right_column() {
    let model = agent_with_task(WorkStateView::Ready, 1);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let mark = hits
        .iter()
        .find_map(|(_, hit)| match hit {
            WorkspaceHit::OpenStatusCatalog(anchor) => Some(*anchor),
            _ => None,
        })
        .expect("the task is marked");
    let row = hits
        .iter()
        .find_map(|(rect, hit)| {
            (matches!(hit, WorkspaceHit::SelectTab(_)) && rect.y == mark.y).then_some(*rect)
        })
        .expect("the mark sits on an agent row");
    assert_eq!(
        mark.x,
        row.right() - 1 - crate::ui::widget::TRAILING_PAD,
        "one right-hand column: {rows:#?}"
    );
    let alias = crate::ui::widget::text::small_caps("agent");
    // Past the header block — its label and its count name the column,
    // not a harness.
    assert!(
        !rows.iter().skip(3).any(|row| row.contains(&alias)),
        "the harness is not named in the sidebar: {rows:#?}"
    );
}

/// The catalog explains every state that has a mark, in both columns —
/// generated from the same tables the sidebar draws with, so it cannot
/// drift from the row it explains. A state with no mark is *not*
/// listed: `Running` draws nothing, and the agent column already says
/// the process is alive.
#[test]
fn the_catalog_names_every_status_in_both_columns() {
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
    terminal
        .draw(|frame| render_status_catalog(frame, frame.area(), Rect::new(4, 2, 1, 1), 0))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    for name in [
        "working",
        "completed",
        "here",
        "idle",
        "uncommitted",
        "ready",
        "published",
        "delivering",
        "conflict",
        "checks failed",
        "delivered",
        "unfinished",
    ] {
        assert!(text.contains(name), "{name} is missing from: {text}");
    }
    for state in every_task_state() {
        if let Some((mark, _)) = task_mark(&state) {
            assert!(text.contains(&mark), "{mark} is missing from: {text}");
        }
    }
    assert!(
        task_mark(&WorkStateView::Running).is_none(),
        "the assertion below is only meaningful while Running is markless"
    );
    assert!(
        !text.contains("running"),
        "a state that draws no mark has no row in a legend of marks: {text}"
    );
}

#[test]
fn the_four_sidebar_states_are_decided_by_one_precedence() {
    // Selection is the only thing the glyph borrows from the cursor,
    // and it is the weakest claim: both states that describe the agent
    // itself outrank it, so the tab you are sitting on still shows you
    // a running turn or an unseen result rather than a plain dot.
    let mut model = agent_session();
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle
    );
    assert_eq!(
        model.agent_tab_status(PaneId(1), true),
        AgentTabStatus::Selected
    );

    model.note_agent_prompt_submission(PaneId(1), &identities_fixture(), Some("hello"));
    assert_eq!(
        model.agent_tab_status(PaneId(1), true),
        AgentTabStatus::Working
    );

    model.remembered.agent_activity.remove(&PaneId(1));
    model.remembered.completed_agent_panes.insert(PaneId(1));
    assert_eq!(
        model.agent_tab_status(PaneId(1), true),
        AgentTabStatus::Completed
    );
}

#[test]
fn each_sidebar_state_draws_its_own_glyph() {
    // Four states, four distinct indicators: the hollow dot, the filled
    // dot, the spinner and the check must never collide, or the column
    // stops answering the question it exists for.
    let glyphs = [
        AgentTabStatus::Idle.glyph(0),
        AgentTabStatus::Selected.glyph(0),
        AgentTabStatus::Working.glyph(0),
        AgentTabStatus::Completed.glyph(0),
    ];
    for (index, glyph) in glyphs.iter().enumerate() {
        assert!(!glyphs[index + 1..].contains(glyph), "duplicate {glyph}");
    }
    assert_eq!(AgentTabStatus::Idle.color(), theme::color(Token::TextFaint));
    assert_eq!(
        AgentTabStatus::Selected.color(),
        theme::color(Token::Accent)
    );
    // The check is the one that says something worked, so it alone
    // keeps a hue in the monochrome default.
    assert_eq!(
        AgentTabStatus::Completed.color(),
        theme::color(Token::StateSuccess)
    );
}
