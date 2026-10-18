//! The sidebar's timeline: the Git reads behind it, its height and scroll,
//! and the commit popup a row opens.

use super::WorkspaceHit;
use super::support::*;
use super::{
    CommitDetailPopup, CommitDetailResolution, ExtensionHit, GitAnswer, GitResolution,
    ScrollDirection, open_commit_detail,
    render::{self, render_commit_detail, timeline_height},
    scroll_timeline, toggle_timeline,
};
use crate::ui::theme::{self, Token};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uze_core::UzeHome;
use uze_extensions::view::ViewHit;

/// Scheduling a read never answers one.
///
/// The point of the whole background path: `git status` and `git log`
/// launch processes, and the loop that calls this is the loop that
/// draws. It reserves the checkout and returns; the badge is whatever
/// it already was.
#[test]
fn scheduling_a_git_read_reserves_the_checkout_and_answers_nothing() {
    let mut model = agent_session_in("/repo");
    let (sender, receiver) = std::sync::mpsc::channel();

    model.schedule_git_read(&sender);

    assert_eq!(
        model.remembered.git_pending.as_deref(),
        Some(Path::new("/repo")),
        "the checkout is reserved while its read is out"
    );
    assert!(
        model.remembered.git_badge.is_none(),
        "nothing is read on the caller's thread"
    );
    assert!(
        receiver.try_recv().is_err() || model.remembered.git_pending.is_some(),
        "the answer arrives on the channel, not from the call"
    );

    // A reservation is what stops the next tick asking again.
    model.schedule_git_read(&sender);
    assert_eq!(
        model.remembered.git_pending.as_deref(),
        Some(Path::new("/repo"))
    );
}

/// An answer about a checkout the selection has left is released and
/// dropped — not drawn over the checkout now in front of the viewer.
#[test]
fn a_git_answer_for_another_checkout_is_released_and_dropped() {
    let mut model = agent_session_in("/repo");
    model.remembered.git_pending = Some(PathBuf::from("/elsewhere"));

    let changed = model.absorb_git_read(GitResolution {
        took: Duration::ZERO,
        cwd: PathBuf::from("/elsewhere"),
        answer: GitAnswer::Full {
            summary: None,
            timeline: Some(uze_extensions::code::Timeline {
                branch: "other".to_owned(),
                commits: Vec::new(),
            }),
        },
    });

    assert!(!changed, "nothing on screen changed");
    assert!(
        model.remembered.git_pending.is_none(),
        "the key is released whatever the answer, or the checkout is \
         never asked about again"
    );
    assert!(
        model.remembered.git_badge.is_none(),
        "no badge for a checkout nobody is on"
    );
}

/// The two cadences are independent: a summary-only answer keeps the
/// history the badge already had, rather than blanking the timeline
/// every 750ms between the 3s reads that fill it.
#[test]
fn a_summary_only_answer_keeps_the_history_already_read() {
    let mut model = session_with_timeline(&["landed"]);
    let read_at = model
        .remembered
        .git_badge
        .as_ref()
        .map(|badge| badge.timeline_checked_at);

    let changed = model.absorb_git_read(GitResolution {
        took: Duration::ZERO,
        cwd: PathBuf::from("/repo"),
        answer: GitAnswer::Summary(Some(uze_extensions::code::ChangeSummary {
            additions: 2,
            deletions: 1,
        })),
    });

    assert!(changed);
    let badge = model.remembered.git_badge.as_ref().expect("a badge");
    assert_eq!(
        badge
            .timeline
            .as_ref()
            .map(|timeline| timeline.commits.len()),
        Some(1),
        "the timeline survives a summary-only read"
    );
    assert_eq!(
        Some(badge.timeline_checked_at),
        read_at,
        "and keeps its own read time, so its own cadence still governs it"
    );
    assert!(badge.summary.is_some());
}

/// A commit account is asked for, not read inline, and an answer
/// nobody is waiting for never opens over them.
#[test]
fn a_commit_account_arrives_only_for_the_row_last_clicked() {
    let mut model = session_with_timeline(&["newest", "older"]);
    let (sender, _receiver) = std::sync::mpsc::channel();

    open_commit_detail(&mut model, 1, Rect::new(0, 0, 10, 1), &sender);
    assert!(
        model.commit_detail.is_none(),
        "the popup opens when the read lands, never from the click"
    );
    let asked = model.commit_detail_pending.clone().expect("a pending hash");

    let stale = model.absorb_commit_detail(CommitDetailResolution {
        hash: "deadbee".to_owned(),
        anchor: Rect::new(0, 0, 10, 1),
        target: None,
        detail: None,
    });
    assert!(!stale, "an answer for another commit is dropped");
    assert_eq!(model.commit_detail_pending.as_deref(), Some(asked.as_str()));

    // Dismissing while the read is still out cancels it, so it cannot
    // open behind the viewer's back when it lands.
    model.dismiss_commit_detail();
    assert!(model.commit_detail_pending.is_none());
    assert!(!model.commit_detail_open());
}

/// Everything the timeline puts on screen is the extension's, and it
/// says so in the extension's own vocabulary.
///
/// The section used to be drawn by hand from `git::Timeline` with
/// three `WorkspaceHit` variants of its own, which made it half an
/// extension: the palette, the eliding and the hit rectangles were
/// all decided on the host's side of a boundary whose whole point is
/// that they are not.
#[test]
fn the_timeline_speaks_only_the_extensions_vocabulary() {
    let model = session_with_timeline(&["feat: newest", "chore: older"]);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());

    let header = timeline_hit(&hits).expect("the header folds the section");

    let commits: Vec<usize> = hits
        .iter()
        .filter_map(|(_, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::SelectItem(index))) => {
                Some(*index)
            }
            _ => None,
        })
        .collect();
    assert_eq!(commits, vec![0, 1], "one hit per commit, in order");

    // Nothing in the section reaches the host's own hit vocabulary.
    let timeline_top = header.y;
    assert!(
        hits.iter()
            .filter(|(rect, _)| rect.y >= timeline_top)
            .all(|(_, hit)| matches!(hit, WorkspaceHit::Extension(ExtensionHit::CodeTimeline(_)))),
        "a host hit escaped into the extension's section: {hits:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("feat: newest")),
        "and the rows are actually drawn: {rows:?}"
    );
}

/// Dragged, the section shows the rows asked for — no fewer than one,
/// no more than the history has, and never into the tree's own
/// minimum — where left alone it stops at half the column.
#[test]
fn dragging_the_timeline_sets_how_many_commits_show() {
    let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
    let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
    let mut model = session_with_timeline(&subjects);

    model.timeline_rows = Some(2);
    let rows = sidebar(&model, &identities_fixture()).rows;
    let drawn = rows.iter().filter(|row| row.contains("commit ")).count();
    assert_eq!(drawn, 2, "{rows:?}");

    model.timeline_rows = None;
    let rows = sidebar(&model, &identities_fixture()).rows;
    let default = rows.iter().filter(|row| row.contains("commit ")).count();

    model.timeline_rows = Some(u16::MAX);
    let rows = sidebar(&model, &identities_fixture()).rows;
    let drawn = rows.iter().filter(|row| row.contains("commit ")).count();
    assert!(drawn > default, "past the half-column default: {rows:?}");
    assert!(
        rows.iter().any(|row| row.contains("Agent")),
        "the tree keeps its rows: {rows:?}"
    );

    let timeline = model
        .remembered
        .git_badge
        .as_ref()
        .unwrap()
        .timeline
        .as_ref()
        .unwrap();
    assert_eq!(
        timeline_height(timeline, false, Some(0), 24),
        2,
        "never fewer than one"
    );
    assert_eq!(
        timeline_height(timeline, true, Some(9), 24),
        1,
        "folded is the header alone"
    );
}

/// The foot is one container: pulling the first steps' header pulls
/// the timeline below them the way the timeline's own header does,
/// since the timeline is the one section whose height is not its
/// content's. A press let go where it was is still the header's
/// click, and with the timeline folded there is nothing to pull, so
/// the press folds at once.
#[test]
fn any_header_at_the_foot_resizes_the_timeline() {
    let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
    let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
    let mut model = session_with_timeline(&subjects);
    model.first_steps_collapsed = false;
    model.timeline_rows = Some(2);
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-foot-resize"));
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    let steps_header = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == WorkspaceHit::ToggleFirstSteps).then_some(*rect))
            .expect("the steps' header is drawn")
    };
    driven.frame();
    let header = steps_header(&driven);

    driven.press(header.x + 4, header.y);
    driven.mouse(
        header.x + 4,
        header.y - 3,
        MouseEventKind::Drag(MouseButton::Left),
    );
    driven.mouse(
        header.x + 4,
        header.y - 3,
        MouseEventKind::Up(MouseButton::Left),
    );
    assert_eq!(
        driven.attach.model.timeline_rows,
        Some(5),
        "three rows up is three more commits"
    );
    assert!(
        !driven.attach.model.first_steps_collapsed,
        "a drag is not a fold"
    );

    driven.frame();
    let header = steps_header(&driven);
    driven.press(header.x + 4, header.y);
    assert!(
        !driven.attach.model.first_steps_collapsed,
        "held, it has not folded yet"
    );
    driven.mouse(
        header.x + 4,
        header.y,
        MouseEventKind::Up(MouseButton::Left),
    );
    assert!(
        driven.attach.model.first_steps_collapsed,
        "let go where it was, it is a click"
    );

    driven.attach.model.timeline_collapsed = true;
    driven.frame();
    let header = steps_header(&driven);
    driven.press(header.x + 4, header.y);
    assert!(
        !driven.attach.model.first_steps_collapsed,
        "nothing to pull while the timeline is folded, so it folds at once"
    );
    assert!(driven.attach.model.timeline_grab.is_none());
}

/// The wheel moves the section a row at a time and never past the
/// page that ends on the oldest commit; every row drawn is a target
/// for the commit it shows, by its place in the history.
#[test]
fn the_timeline_scrolls_by_rows_within_its_history() {
    let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
    let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
    let mut model = session_with_timeline(&subjects);

    model.timeline_scroll = 5;
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let drawn: Vec<&String> = rows.iter().filter(|row| row.contains("commit ")).collect();
    assert!(drawn[0].contains("● commit 5"), "{rows:?}");
    assert!(
        rows.iter().all(|row| !row.contains('◉')),
        "HEAD scrolled off: {rows:?}"
    );
    let targets: Vec<usize> = hits
        .iter()
        .filter_map(|(_, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::SelectItem(index))) => {
                Some(*index)
            }
            _ => None,
        })
        .collect();
    assert_eq!(targets[0], 5);
    assert_eq!(targets.len(), drawn.len());

    model.timeline_scroll = 100;
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(rows.last().unwrap().contains("commit 19"), "{rows:?}");

    model.timeline_scroll = 0;
    model.hits = hits;
    let shown = model.timeline_rows_shown();
    for _ in 0..50 {
        scroll_timeline(&mut model, ScrollDirection::Down);
    }
    assert_eq!(model.timeline_scroll, 20 - shown);
    for _ in 0..50 {
        scroll_timeline(&mut model, ScrollDirection::Up);
    }
    assert_eq!(model.timeline_scroll, 0);
}

fn commit_popup(anchor: Rect) -> CommitDetailPopup {
    CommitDetailPopup {
        detail: uze_extensions::code::CommitDetail {
            hash: "0ebf3b8000000000000000000000000000000000".to_owned(),
            short_hash: "0ebf3b8".to_owned(),
            author: "Ada".to_owned(),
            age: "14 minutes ago".to_owned(),
            date: "2026-09-03 19:39".to_owned(),
            refs: vec![
                "agent/task".to_owned(),
                "main".to_owned(),
                "origin/main".to_owned(),
            ],
            subject: "docs(openspec): archive five completed changes".to_owned(),
            body: "Every task done.\n\nThree decisions cleared the ADR bar.".to_owned(),
            files_changed: 8,
            insertions: 350,
            deletions: 4,
        },
        target: Some("main".to_owned()),
        anchor,
        scroll: 0,
    }
}

fn popup_rows(width: u16, height: u16, popup: &CommitDetailPopup) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| render_commit_detail(frame, frame.area(), popup))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect()
}

/// The popup is the commit's account — who, when, what it said, what
/// it touched, and what stands at it — beside the row it opened from,
/// in the pane's columns.
#[test]
fn a_commit_popup_stands_beside_its_row_and_gives_its_account() {
    let anchor = Rect::new(1, 20, 38, 1);
    let rows = popup_rows(120, 30, &commit_popup(anchor));
    let text = rows.join("\n");

    assert!(text.contains("commit"), "{text}");
    assert!(
        text.contains("Ada · 14 minutes ago · 2026-09-03 19:39"),
        "{text}"
    );
    assert!(
        text.contains("docs(openspec): archive five completed changes"),
        "{text}"
    );
    assert!(
        text.contains("Three decisions cleared the ADR bar."),
        "{text}"
    );
    assert!(text.contains("8 files changed  +350  −4"), "{text}");
    assert!(text.contains(" agent/task   main   origin/main "), "{text}");
    assert!(text.contains("0ebf3b8"), "{text}");
    let border_row = rows
        .iter()
        .position(|row| row.contains('╭') || row.contains('┌'))
        .expect("the popup has a frame");
    let left = rows[border_row]
        .chars()
        .position(|c| c == '╭' || c == '┌')
        .unwrap();
    assert_eq!(
        left as u16,
        anchor.right() + 1,
        "beside the sidebar's divider"
    );
    assert!(
        border_row <= 20,
        "level with its row, or pulled up to fit: {border_row}"
    );
}

/// Among the refs at a commit, the delivery target and its
/// remote-tracking twin wear the target's gold; any other branch is
/// blue, the way the timeline colours a commit still ahead.
#[test]
fn the_target_ref_wears_gold_and_the_others_blue() {
    let popup = commit_popup(Rect::new(1, 2, 38, 1));
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| render_commit_detail(frame, frame.area(), &popup))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let rows = popup_rows(120, 30, &popup);
    let row = rows
        .iter()
        .position(|row| row.contains(" agent/task "))
        .unwrap();
    let bg_of = |needle: &str| {
        let column = rows[row].find(needle).unwrap() + 1;
        buffer[(column as u16, row as u16)].bg
    };
    assert_eq!(bg_of("agent/task"), theme::color(Token::StateInfo));
    assert_eq!(bg_of(" main "), theme::color(Token::StateWarning));
    assert_eq!(bg_of("origin/main"), theme::color(Token::StateWarning));
}

/// A body Git hard-wrapped near 72 columns is reflowed to the popup's
/// own width, not wrapped a second time with a word stranded on each
/// row, and what it quotes in backticks loses its backticks.
#[test]
fn a_commit_body_is_reflowed_and_read_as_markdown() {
    let mut popup = commit_popup(Rect::new(1, 2, 38, 1));
    popup.detail.body = "A hint's target covered only its key, one column for an arrow, so a\n\
                         click on the words beside it did nothing. A run (`1-5 section`) is\n\
                         no target at all."
        .to_owned();
    let rows = popup_rows(120, 30, &popup);
    let body: Vec<&str> = rows
        .iter()
        .map(|row| row.trim_matches(|c: char| c == ' ' || c == '│'))
        .skip_while(|row| !row.starts_with("A hint's"))
        .take_while(|row| !row.is_empty())
        .collect();
    assert!(
        body.iter().all(|row| row.split_whitespace().count() > 1),
        "no word stranded on a row of its own: {body:?}"
    );
    let text = body.join(" ");
    assert!(text.contains("A run (1-5 section) is no target"), "{text}");
    assert!(!text.contains('`'), "{text}");
}

/// A long message scrolls inside the popup rather than growing it
/// over the pane, and the wheel stops where the text does.
#[test]
fn a_long_commit_message_scrolls_inside_a_bounded_popup() {
    let mut popup = commit_popup(Rect::new(1, 2, 38, 1));
    popup.detail.body = (0..40)
        .map(|index| format!("paragraph {index}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let area = Rect::new(0, 0, 120, 60);
    let layout = render::commit_detail_layout(area, &popup);
    assert_eq!(layout.rect.height, 20, "no taller than a hover card");
    assert!(layout.scroll_limit() > 0);
    assert_eq!(
        layout.scroll_limit() + layout.inner.height,
        layout.content_rows
    );

    let rows = popup_rows(120, 60, &popup);
    assert!(rows.join("\n").contains("paragraph 0"));
    assert!(!rows.join("\n").contains("paragraph 39"), "{rows:?}");
    popup.scroll = u16::MAX;
    let rows = popup_rows(120, 60, &popup);
    let text = rows.join("\n");
    assert!(
        text.contains("0ebf3b8"),
        "held to the end of the text: {text}"
    );
    assert_eq!(
        rows.iter().filter(|row| row.contains('│')).count(),
        18,
        "the frame keeps its height: {rows:?}"
    );
}

/// A frame with no columns at all — a terminal being resized through
/// nothing — lays the popup out without arithmetic going below zero.
#[test]
fn a_frame_with_no_width_lays_the_commit_popup_out() {
    let popup = commit_popup(Rect::new(0, 0, 0, 1));
    let layout = render::commit_detail_layout(Rect::new(0, 0, 0, 10), &popup);
    assert_eq!(layout.rect.x, 0);
}

/// A frame with no room beside the sidebar puts the popup over the
/// pane, inset, rather than clipping it against the edge.
#[test]
fn a_narrow_frame_centres_the_commit_popup() {
    let rows = popup_rows(60, 30, &commit_popup(Rect::new(1, 5, 38, 1)));
    let border_row = rows
        .iter()
        .position(|row| row.contains('╭') || row.contains('┌'))
        .expect("the popup has a frame");
    let left = rows[border_row]
        .chars()
        .position(|c| c == '╭' || c == '┌')
        .unwrap();
    assert_eq!(left, 2, "{:?}", rows[border_row]);
    assert!(rows.join("\n").contains("0ebf3b8"));
}

/// While a commit is open the wheel scrolls its text, not the pane
/// underneath, and any click or key puts it away.
#[test]
fn an_open_commit_is_a_modal_like_the_support_dropdown() {
    let mut model = agent_session_in("/repo");
    assert!(model.no_modal_open());
    model.commit_detail = Some(commit_popup(Rect::new(1, 5, 38, 1)));
    assert!(!model.no_modal_open());
}

/// A commit's dot says where it stands: blue while it is still ahead
/// of the base, the target's gold once it has landed there. The ring
/// says `HEAD`, whichever colour it wears.
#[test]
fn a_commits_dot_wears_its_standing() {
    let mut model = session_with_timeline(&["feat: ahead", "fix: also ahead", "chore: landed"]);
    let commits = &mut model
        .remembered
        .git_badge
        .as_mut()
        .unwrap()
        .timeline
        .as_mut()
        .unwrap()
        .commits;
    commits[0].ahead = true;
    commits[1].ahead = true;

    let buffer = sidebar(&model, &identities_fixture()).buffer;
    let rows = sidebar(&model, &identities_fixture()).rows;
    let dot_of = |needle: &str| {
        let row = rows.iter().position(|row| row.contains(needle)).unwrap();
        let column = rows[row]
            .chars()
            .position(|c| c == '◉' || c == '●')
            .unwrap();
        (
            rows[row].chars().nth(column).unwrap(),
            buffer[(column as u16, row as u16)].fg,
        )
    };
    assert_eq!(dot_of("feat: ahead"), ('◉', theme::color(Token::StateInfo)));
    assert_eq!(
        dot_of("fix: also ahead"),
        ('●', theme::color(Token::StateInfo))
    );
    assert_eq!(
        dot_of("chore: landed"),
        ('●', theme::color(Token::StateWarning))
    );
}

/// The header is the section's one heading: filled and bold, where the
/// commit rows under it are plain.
#[test]
fn the_timeline_header_stands_out_from_its_rows() {
    let model = session_with_timeline(&["feat: only"]);
    let buffer = sidebar(&model, &identities_fixture()).buffer;
    let rows = sidebar(&model, &identities_fixture()).rows;
    let header = rows
        .iter()
        .position(|row| row.contains("timeline"))
        .expect("the header is drawn");
    let column = rows[header].chars().position(|c| c == 't').unwrap() as u16;

    let cell = &buffer[(column, header as u16)];
    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
    assert_eq!(cell.bg, theme::color(Token::SurfaceRaisedSubtle));
    let commit = &buffer[(column, header as u16 + 1)];
    assert_ne!(commit.bg, theme::color(Token::SurfaceRaisedSubtle));
}

/// The timeline keeps the foot of the column, under the spaces, with
/// its header naming the branch and `HEAD` ringed at the top of the
/// list — wherever the tree above happens to end.
#[test]
fn the_timeline_keeps_the_foot_of_the_column() {
    let model = session_with_timeline(&["feat: third", "fix: second", "chore: first"]);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let last = rows.len() - 1;

    assert!(rows[last].contains("● chore: first"), "{rows:?}");
    assert!(rows[last - 1].contains("● fix: second"), "{rows:?}");
    assert!(rows[last - 2].contains("◉ feat: third"), "{rows:?}");
    assert!(inside(&rows[last - 2]).ends_with("3h"), "{rows:?}");
    let header = &rows[last - 3];
    assert!(header.contains("▾ timeline"), "{header}");
    assert!(inside(header).ends_with("main"), "{header}");
    assert_eq!(
        timeline_hit(&hits).map(|rect| rect.y),
        Some((last - 3) as u16)
    );
    let space_row = rows
        .iter()
        .position(|row| row.contains("Agent"))
        .expect("the agent stays in the tree above");
    assert!(space_row < last - 3, "{rows:?}");
}

/// Folded, the section is its divider and header alone — still at the
/// foot, still the one target that opens it back up.
#[test]
fn folding_the_timeline_keeps_only_its_header() {
    let mut model = session_with_timeline(&["feat: third", "fix: second"]);
    model.timeline_collapsed = true;
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let last = rows.len() - 1;

    assert!(rows[last].contains("▸ timeline"), "{rows:?}");
    assert!(
        rows.iter()
            .all(|row| !row.contains("feat:") && !row.contains("fix:")),
        "{rows:?}"
    );
    let hit = timeline_hit(&hits).expect("the header folds and unfolds");
    assert_eq!((hit.y, hit.height), (last as u16, 1));
}

/// The spaces are what the sidebar is for: however long the history,
/// the section takes at most half of what the column has left, and
/// the newest commits are the ones that fit.
#[test]
fn the_timeline_takes_at_most_half_the_column() {
    let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
    let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
    let model = session_with_timeline(&subjects);
    let rows = sidebar(&model, &identities_fixture()).rows;

    let drawn: Vec<&String> = rows.iter().filter(|row| row.contains("commit ")).collect();
    assert!(drawn.len() < 20, "{rows:?}");
    assert!(drawn.len() * 2 <= rows.len(), "{rows:?}");
    assert!(drawn[0].contains("commit 0"), "newest first: {rows:?}");
    assert_eq!(
        timeline_height(
            model
                .remembered
                .git_badge
                .as_ref()
                .unwrap()
                .timeline
                .as_ref()
                .unwrap(),
            false,
            None,
            1
        ),
        0,
        "a column too short for the header shows nothing"
    );
}

/// The subject gives way before the age, so the column that says when
/// stays a column however long the commit message runs.
#[test]
fn a_long_subject_gives_way_before_its_age() {
    let model = session_with_timeline(&[
        "feat(tui): a subject long enough to run past the sidebar's width",
    ]);
    let rows = sidebar(&model, &identities_fixture()).rows;
    let row = rows
        .iter()
        .find(|row| row.contains("◉"))
        .expect("the commit is drawn");

    assert!(row.contains('…'), "{row}");
    assert!(inside(row).ends_with("3h"), "{row:?}");
    assert!(row.contains("feat(tui): a subject"), "{row}");
}

/// Folding the timeline is a preference, not a transient: the next
/// run is told, rather than opening the section again over the spaces.
#[test]
fn folding_the_timeline_is_kept_for_the_next_run() {
    let (recorder, recorded) = std::sync::mpsc::channel();
    let mut model = session_with_timeline(&["feat: newest"]);
    model.layout_recorder = Some(recorder);
    model.timeline_collapsed = false;
    model.timeline_rows = Some(4);

    toggle_timeline(&mut model);

    let shape = recorded.try_recv().expect("the fold is recorded");
    assert!(shape.workspace.timeline_collapsed);
    assert_eq!(
        shape.workspace.timeline_rows,
        Some(4),
        "the height it was left at"
    );
    assert_eq!(
        shape.sidebar.width, model.sidebar_width,
        "the whole column's shape, not the one field that changed"
    );
}
