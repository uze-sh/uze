//! The header and the toasts: the line about work in flight, the sync it
//! offers, and outcomes stacked over the pane.

use super::WorkspaceHit;
use super::support::*;
use super::{FileResolution, code};
use super::{
    GitBadge, UpstreamSync, WorkStateView, WorkspaceModel, agent_activity_frame,
    render::{self, LeadingSlot, compute_layout, render_tab_strip},
};
use crate::ui::theme::{self, Token};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;
use std::time::Instant;
use uze_core::UzeHome;

/// The header is two zones, and the message is never allowed into the
/// other one: whatever the workspace has to say, every action keeps
/// the exact rect it had — including one about the very task the
/// message is about, which is the case that used to take the button
/// away mid-click.
#[test]
fn a_message_never_moves_an_action() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    model.remembered.git_badge = Some(GitBadge {
        cwd: PathBuf::from("/repo/.worktrees/ai"),
        summary: Some(uze_extensions::code::ChangeSummary {
            additions: 12,
            deletions: 3,
        }),
        timeline: None,
        timeline_checked_at: Instant::now(),
        checked_at: Instant::now(),
    });
    let (_, quiet) = tab_strip(&model);

    model.set_busy_notice("delivering all".to_owned());
    let (rows, speaking) = tab_strip(&model);

    assert!(rows[0].contains("delivering all │"), "{rows:?}");
    assert_eq!(
        quiet.len(),
        speaking.len(),
        "the same actions are offered either way"
    );
    for (quiet, speaking) in quiet.iter().zip(&speaking) {
        assert_eq!(
            (quiet.0, format!("{:?}", quiet.1)),
            (speaking.0, format!("{:?}", speaking.1)),
            "an action moved under the message"
        );
    }
}

/// The header carries work in flight and nothing else. It says so by
/// moving: a spinner rides in front of the words, which is what buys
/// the message the right to be two of them.
///
/// What ended is a toast, so the hint goes when the work does rather
/// than when something is said about it — an operation that finishes
/// with nothing to report still finishes.
#[test]
fn the_header_carries_work_in_flight_and_lets_go_when_it_ends() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    model.set_busy_notice("delivering all".to_owned());
    model.tick = a_tick_showing_the_fullest_frame();
    let (rows, _) = tab_strip(&model);
    assert!(
        rows[0].contains(&format!(
            "{} delivering all",
            agent_activity_frame(model.tick)
        )),
        "{rows:?}"
    );

    model.clear_busy_notice();
    let (settled, _) = tab_strip(&model);
    assert!(
        !settled[0].contains("delivering all"),
        "the hint goes with the work: {settled:?}"
    );
    assert!(
        !settled[0].contains(&agent_activity_frame(model.tick)),
        "and so does the spinner: {settled:?}"
    );
}

/// Outcomes are never drawn in the strip. The header is two words
/// about what is happening now; a message about what happened is a
/// toast, which arrives whether or not the reader is looking here and
/// does not have to be chosen over the next one.
#[test]
fn an_outcome_is_never_drawn_in_the_header() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    model.raise_toast(
        crate::ui::widget::ToastKind::Done,
        "merged → main",
        "fix-auth-redirect",
        None,
    );

    let (rows, _) = tab_strip(&model);
    assert!(
        !rows.iter().any(|row| row.contains("merged")),
        "the strip says nothing about it: {rows:?}"
    );
    assert_eq!(model.toast_stack().len(), 1, "the toast has it");
}

/// The operator's own tree is where a pull or a push is due, so the
/// header's git zone carries what each would move for the checkout in
/// front — an arrow for the direction and the count, red for what is
/// to pull and green for what is to push, with nothing between them —
/// and only the halves that have a count. The sidebar says none of it.
#[test]
fn the_header_says_what_a_pull_and_a_push_would_move() {
    let mut model = agent_session_in("/repo");
    model
        .remembered
        .upstream_syncs
        .insert(PathBuf::from("/repo"), UpstreamSync { pull: 1, push: 12 });
    let header = |model: &WorkspaceModel| {
        let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render_tab_strip(
                    frame,
                    frame.area(),
                    model,
                    &identities_fixture(),
                    LeadingSlot::AgentTabs,
                    &mut hits,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let (row, cells) = (0..buffer.area.height)
            .map(|y| {
                let cells: Vec<_> = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].clone())
                    .collect();
                (
                    cells.iter().map(|cell| cell.symbol()).collect::<String>(),
                    cells,
                )
            })
            .find(|(row, _)| row.contains("code"))
            .expect("the header row");
        let hue = |glyph: &str| {
            cells
                .iter()
                .find(|cell| cell.symbol() == glyph)
                .map(|cell| cell.fg)
        };
        (row.clone(), hue("\u{2193}"), hue("\u{2191}"))
    };

    let (row, pull, push) = header(&model);
    assert!(
        row.contains("\u{2193}1\u{2191}12"),
        "pull and push read as one, with no gap: {row:?}"
    );
    assert_eq!(pull, Some(theme::color(Token::StateDanger)));
    assert_eq!(push, Some(theme::color(Token::StateSuccess)));
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(
        rows.iter()
            .all(|row| !row.contains('\u{2193}') && !row.contains('\u{2191}')),
        "the sidebar leaves it to the header: {rows:?}"
    );

    model
        .remembered
        .upstream_syncs
        .insert(PathBuf::from("/repo"), UpstreamSync { pull: 0, push: 3 });
    let (row, ..) = header(&model);
    assert!(
        !row.contains('\u{2193}') && row.contains("\u{2191}3"),
        "nothing to pull, three to push: {row:?}"
    );

    model
        .remembered
        .upstream_syncs
        .insert(PathBuf::from("/repo"), UpstreamSync::default());
    let (row, ..) = header(&model);
    assert!(
        !row.contains('\u{2191}') && !row.contains('\u{2193}'),
        "in sync says nothing: {row:?}"
    );
}

/// What a pull would bring is the pull: the count is a target that
/// says so under the pointer and starts it on a click, once, with the
/// header saying it runs. What a push would send is only reported.
#[test]
fn clicking_what_a_pull_would_bring_pulls() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-pull-click"));
    let mut model = agent_session_in("/repo");
    model
        .remembered
        .upstream_syncs
        .insert(PathBuf::from("/repo"), UpstreamSync { pull: 9, push: 2 });
    model
        .remembered
        .targets
        .insert(PathBuf::from("/repo"), "main".to_owned());
    let mut driven = driven(model, &home);
    driven.frame();
    let pull = hit_rect(&driven.attach.model, WorkspaceHit::PullTarget);
    assert_eq!(
        pull.width, 2,
        "the pull count alone is the target: {pull:?}"
    );

    driven.attach.model.hovered = Some(WorkspaceHit::PullTarget);
    let (rows, _) = tab_strip(&driven.attach.model);
    assert!(
        rows.join("\n").contains("pull 9 · fast-forward"),
        "{rows:?}"
    );

    driven.press(pull.x, pull.y);
    assert!(
        driven.attach.model.remembered.pull_pending,
        "the pull started"
    );
    let said = driven
        .attach
        .model
        .remembered
        .notice
        .as_ref()
        .map(|notice| notice.text.clone());
    assert_eq!(said.as_deref(), Some("pulling main"));
}

/// Inside a slot the remote is the target's business, not the task's:
/// the header carries no pull or push for it, whatever the primary
/// checkout owes.
#[test]
fn a_slot_never_shows_the_primary_sync() {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    model
        .remembered
        .upstream_syncs
        .insert(PathBuf::from("/repo"), UpstreamSync { pull: 2, push: 2 });
    let (rows, _) = tab_strip(&model);
    assert!(
        rows.iter()
            .all(|row| !row.contains('\u{2193}') && !row.contains('\u{2191}')),
        "no arrow for a slot: {rows:?}"
    );
}

/// Every toast's words fit the toast. A title over [`TOAST_TITLE`] or
/// a detail over [`TOAST_DETAIL`] columns is cut with an ellipsis in a
/// pane of ordinary width, and the cut half is usually the half that
/// said what to do. Checked on the literals every `raise_toast` call
/// writes, so a long one fails here rather than on somebody's screen;
/// what a call formats at run time is the caller's to keep short.
#[test]
fn every_toasts_literal_words_fit_the_toast() {
    const TOAST_TITLE: usize = 24;
    const TOAST_DETAIL: usize = 32;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/orchestrator");
    let mut sources = vec![root.with_extension("rs")];
    let mut pending = vec![root];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && !path.components().any(|part| part.as_os_str() == "tests")
                && path.file_name().is_some_and(|name| name != "tests.rs")
            {
                sources.push(path);
            }
        }
    }
    let mut too_long = Vec::new();
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap();
        for (at, _) in text.match_indices("raise_toast(") {
            let arguments = call_arguments(&text[at + "raise_toast(".len()..]);
            for (index, limit) in [(1, TOAST_TITLE), (2, TOAST_DETAIL)] {
                let Some(words) = arguments.get(index).and_then(|arg| literal(arg)) else {
                    continue;
                };
                if words.chars().count() > limit {
                    too_long.push(format!("{}: {words:?}", path.display()));
                }
            }
        }
    }
    assert!(
        too_long.is_empty(),
        "toast words over the limit:\n{}",
        too_long.join("\n")
    );
}

/// The top-level arguments of a call whose opening parenthesis was just
/// read, split at the commas between them.
fn call_arguments(after_paren: &str) -> Vec<String> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    let mut arguments = vec![String::new()];
    for character in after_paren.chars() {
        if quoted {
            arguments.last_mut().unwrap().push(character);
            match (escaped, character) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => quoted = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => break,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                arguments.push(String::new());
                continue;
            }
            _ => {}
        }
        arguments.last_mut().unwrap().push(character);
    }
    arguments
}

/// The words of an argument that is a plain string literal, with
/// `.to_owned()` or `.into()` allowed after it; `None` for anything
/// computed.
fn literal(argument: &str) -> Option<String> {
    let argument = argument.trim();
    let argument = argument
        .strip_suffix(".to_owned()")
        .or_else(|| argument.strip_suffix(".into()"))
        .unwrap_or(argument);
    let inner = argument.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains('"')).then(|| {
        inner
            .replace("\\\n", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// A toast ends at the column the tab strip's controls end at.
///
/// The pane is already inset a column from the frame, and so is the
/// strip's own content — insetting the toast stack again put a second
/// margin on that side and left every box a column short of the chip
/// above it, which reads as the two belonging to different screens.
#[test]
fn a_toast_lines_up_with_the_controls_above_it() {
    use crate::ui::widget::ToastKind;

    let mut model = agent_with_task(WorkStateView::Ready, 3);
    model.raise_toast(ToastKind::Done, "synced", "to main", None);

    let frame_area = Rect::new(0, 0, 120, 30);
    let layout = compute_layout(frame_area, model.sidebar_width);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render::render(
                frame,
                &model,
                &identities_fixture(),
                &mut hits,
                &mut Default::default(),
            );
        })
        .unwrap();

    // Two targets dismiss: the `✕` and the box behind it. The box is
    // the one whose edge this is about.
    let toast = hits
        .iter()
        .filter_map(|(rect, hit)| matches!(hit, WorkspaceHit::DismissToast(_)).then_some(*rect))
        .max_by_key(|rect| rect.width)
        .expect("the toast registered a target");
    let chip = hits
        .iter()
        .find_map(|(rect, hit)| matches!(hit, WorkspaceHit::OpenFiles).then_some(*rect))
        .expect("the strip drew its controls");

    assert_eq!(
        toast.right(),
        layout.pane.right(),
        "flush with the pane's own edge"
    );
    assert!(
        toast.right() >= chip.right(),
        "and no further in than the chip above it: toast {} vs chip {}",
        toast.right(),
        chip.right()
    );
    assert_eq!(
        toast.y, layout.pane.y,
        "and on the pane's first row: the strip already puts a row \
         between the two, and a second one reads as the message \
         floating rather than answering what is above it"
    );
}

/// A toast's `✕` puts it away over an open extension.
///
/// The surface resolves presses in the pane its own way, against its
/// own hits, so a press on a toast drawn over it reached the surface
/// and the message stayed.
#[test]
fn a_toast_over_an_open_extension_answers_its_own_close() {
    use crate::ui::widget::ToastKind;

    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-toast-over-extension",
    ));
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    model.code = Some(uze_extensions::code::CodeView::opening(
        PathBuf::from("/w"),
        "/w".to_owned(),
        uze_extensions::code::ContentMode::Contents,
    ));
    model.raise_toast(ToastKind::Done, "synced", "to main", None);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let close = toast_close(&driven);

    driven.press(close.x, close.y);

    assert!(
        driven.attach.model.toast_stack().is_empty(),
        "the toast went"
    );
    assert!(driven.attach.model.code.is_some(), "and the surface stayed");
}

/// A write that failed after the code surface closed has nowhere else to
/// be said, so it is said as an outcome; one that landed stays quiet.
#[test]
fn a_failed_write_whose_surface_moved_on_is_still_reported() {
    let mut model = WorkspaceModel::default();
    let answered = model.absorb_file_answer(FileResolution {
        root: PathBuf::from("/gone"),
        answer: code::FileAnswer::Saved {
            path: PathBuf::from("/gone/a.txt"),
            outcome: Err("permission denied".to_owned()),
        },
    });
    assert!(answered);
    assert_eq!(model.toast_stack().len(), 1);
    assert_eq!(model.remembered.toasts[0].text, "save failed");

    let answered = model.absorb_file_answer(FileResolution {
        root: PathBuf::from("/gone"),
        answer: code::FileAnswer::Deleted {
            path: PathBuf::from("/gone/b.txt"),
            outcome: Err("busy".to_owned()),
        },
    });
    assert!(answered);
    assert_eq!(model.toast_stack().len(), 2);

    let answered = model.absorb_file_answer(FileResolution {
        root: PathBuf::from("/gone"),
        answer: code::FileAnswer::Saved {
            path: PathBuf::from("/gone/a.txt"),
            outcome: Ok(()),
        },
    });
    assert!(!answered);
    assert_eq!(model.toast_stack().len(), 2);
}
