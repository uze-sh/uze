//! The sections at the foot of the column: first steps, release notes, and
//! folding them.

use super::WorkspaceHit;
use super::support::*;
use super::{
    WorkspaceModel,
    render::{self},
    toggle_spec_summary, toggle_timeline,
};
use crate::ui::theme::{self, Token};
use std::time::{Duration, Instant};
use uze_core::UzeHome;

#[test]
fn the_first_steps_keep_the_foot_beside_a_space_of_agents_in_its_root() {
    let mut model = agents_in_the_root_session();
    model.first_steps_collapsed = false;
    let rows = sidebar(&model, &identities_in_the_root()).rows;
    assert!(
        rows.iter().any(|row| row.contains("first steps")),
        "the foot is budgeted from the measure: {rows:?}"
    );
}

/// A folded section's title is plain: bold is for a heading over
/// content, and folded there is none under it.
#[test]
fn a_folded_sections_title_is_not_bold() {
    let mut model = session_with_timeline(&["feat: only"]);
    let title_is_bold = |model: &WorkspaceModel| {
        let Sidebar { rows, buffer, .. } = sidebar(model, &identities_fixture());
        let header = rows
            .iter()
            .position(|row| row.contains("timeline"))
            .expect("the header is drawn");
        let column = rows[header]
            .find("timeline")
            .map(|byte| rows[header][..byte].chars().count())
            .unwrap() as u16;
        buffer[(column, header as u16)]
            .modifier
            .contains(ratatui::style::Modifier::BOLD)
    };
    model.timeline_collapsed = false;
    assert!(title_is_bold(&model), "open, it heads its rows");
    model.timeline_collapsed = true;
    assert!(!title_is_bold(&model), "folded, it is plain");
}

/// Both sections stack at the foot of the column, the steps on the
/// history, and each takes its rows before the tree is laid out — so
/// opening one pushes the other rather than being drawn over it.
#[test]
fn the_two_sections_stack_at_the_foot_and_push_each_other() {
    let mut model = session_with_timeline(&["feat: one", "fix: two", "chore: three"]);
    model.timeline_collapsed = true;
    model.first_steps_collapsed = true;
    let rows = sidebar(&model, &identities_fixture()).rows;

    let steps = rows
        .iter()
        .position(|row| row.contains("first steps"))
        .expect("the steps are at the foot");
    let timeline = rows
        .iter()
        .position(|row| row.contains("timeline"))
        .expect("and the history under them");
    assert_eq!(timeline, steps + 1, "in that order, adjacent: {rows:?}");
    assert_eq!(timeline, rows.len() - 1, "and nothing below: {rows:?}");

    // Opening the steps pushes the history down the column, never over
    // it: both headers are still on screen, still in that order.
    model.first_steps_collapsed = false;
    let rows = sidebar(&model, &identities_fixture()).rows;
    let steps = rows
        .iter()
        .position(|row| row.contains("first steps"))
        .expect("still there");
    let timeline = rows
        .iter()
        .position(|row| row.contains("timeline"))
        .expect("and so is the history");
    assert_eq!(
        timeline,
        steps + 1 + render::FIRST_STEPS.len() + 1,
        "the steps came between them, with a blank row closing them off \
         so the last one does not sit against the next header: {rows:?}"
    );
    assert!(
        inside(&rows[timeline - 1]).trim().is_empty(),
        "and that row is blank: {rows:?}"
    );
    assert_eq!(timeline, rows.len() - 1, "{rows:?}");
}

/// Each section folds on its own: opening one leaves the others as
/// they were, so more than one can be open at once.
#[test]
fn opening_one_section_leaves_the_others_open() {
    let mut model = session_with_timeline(&["feat: one"]);
    model.timeline_collapsed = true;
    model.first_steps_collapsed = false;
    model.spec_summary_open = true;

    toggle_timeline(&mut model);
    assert!(!model.timeline_collapsed);
    assert!(!model.first_steps_collapsed, "the steps stayed open");
    assert!(model.spec_summary_open, "and so did the spec");

    toggle_spec_summary(&mut model);
    assert!(!model.spec_summary_open);
    assert!(!model.timeline_collapsed, "folding one folds only it");
}

/// The release notice sits on the sections holding the foot, not under
/// them, and every part of it is whole at a real version's length.
#[test]
fn a_release_notice_sits_on_the_sections_at_the_foot() {
    let mut model = session_with_timeline(&["feat: one"]);
    model.timeline_collapsed = true;
    let hits = sidebar(&model, &identities_fixture()).hits;
    assert!(
        !hits.iter().any(|(_, hit)| matches!(
            hit,
            WorkspaceHit::OpenReleaseNotes | WorkspaceHit::DismissRelease
        )),
        "no release, no notice"
    );

    model.release = Some(crate::self_update::Notice("0.0.0-alpha.14".to_owned()));
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let (mark, _) = hits
        .iter()
        .find(|(_, hit)| matches!(hit, WorkspaceHit::DismissRelease))
        .expect("its mark puts it away");
    let y = usize::from(mark.y);
    assert!(
        rows[y].contains("v0.0.0-alpha.14")
            && rows[y].contains(&theme::glyph(theme::Symbol::MarkClose)),
        "the version, whole, with the mark on its row: {rows:?}"
    );
    assert!(rows[y + 1].contains("restart to use it"), "{rows:?}");
    assert_eq!(
        hits.iter()
            .filter(|(_, hit)| matches!(hit, WorkspaceHit::OpenReleaseNotes))
            .count(),
        2,
        "two rows, and each opens the notes"
    );
    let steps = rows
        .iter()
        .position(|line| line.contains("first steps"))
        .expect("the steps are still there");
    assert!(y < steps, "and the notice sits on them: {rows:?}");
}

/// The notes open over everything, take the keys and the pointer from
/// what is underneath, and a click on them is reading rather than a
/// way out.
#[test]
fn release_notes_open_over_the_workspace() {
    let mut model = session_with_timeline(&["feat: one"]);
    let mut modal = crate::ui::release_notes::ReleaseNotesModal::opening("0.0.0-alpha.14");
    modal.absorb(
        "0.0.0-alpha.14",
        Some(crate::self_update::ReleaseNotes {
            version: "0.0.0-alpha.14".to_owned(),
            date: None,
            body: "### Fixes\n\n- **terminal:** the fix".to_owned(),
        }),
    );
    model.release_notes = Some(modal);
    assert!(!model.no_modal_open(), "the pane gets no keys under it");

    let rows = frame_rows(&mut model);
    let drawn = rows.join("\n");
    assert!(drawn.contains("v0.0.0-alpha.14"), "{drawn}");
    assert!(drawn.contains("the fix"), "{drawn}");
    full_frame(&mut model);
    assert!(
        matches!(
            model.hits.first(),
            Some((_, WorkspaceHit::ReleaseNotesBody))
        ),
        "its area answers before anything underneath"
    );
}

/// Clicked, the notice opens the notes of the release it names — and
/// no other — read off the drawing thread; the corner mark puts the
/// modal away.
#[test]
fn the_notice_opens_the_notes_of_its_own_release() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-release-notes"));
    std::fs::create_dir_all(home.cache_dir()).unwrap();
    std::fs::write(
        home.release_notes_cache_path(),
        "# Changelog\n\n## [9.0.1](https://x) - 2026-09-23\n\n- **ui:** newer thing\n\n\
         ## [9.0.0](https://x) - 2026-09-22\n\n- **terminal:** older thing\n",
    )
    .unwrap();
    let mut model = session_with_timeline(&["feat: one"]);
    model.timeline_collapsed = true;
    model.release = Some(crate::self_update::Notice("9.0.1".to_owned()));
    let mut driven = driven(model, &home);
    driven.frame();
    let (notice, _) = *driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, WorkspaceHit::OpenReleaseNotes))
        .expect("the notice answers a click");
    driven.press(notice.x, notice.y);
    assert!(
        driven.attach.model.release_notes.is_some(),
        "opened at once"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    while !frame_rows(&mut driven.attach.model)
        .join("\n")
        .contains("newer thing")
    {
        assert!(Instant::now() < deadline, "the notes never arrived");
        driven.pump();
        std::thread::sleep(Duration::from_millis(5));
    }

    let drawn = frame_rows(&mut driven.attach.model).join("\n");
    assert!(
        !drawn.contains("older thing"),
        "only the release the notice names: {drawn}"
    );

    driven.frame();
    let (body, _) = *driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, WorkspaceHit::ReleaseNotesBody))
        .expect("the modal answers for its own area");
    driven.press(body.x + 2, body.y + 2);
    assert!(
        driven.attach.model.release_notes.is_some(),
        "a click on the notes is reading"
    );

    driven.frame();
    driven.press(body.x.saturating_sub(2), body.y);
    assert!(
        driven.attach.model.release_notes.is_none(),
        "a click outside it closes it, as it closes every dialog"
    );
}

/// The header folds it, and it stays folded: a section that came back
/// open every run would be one nobody could put away.
#[test]
fn the_first_steps_section_folds_to_its_header() {
    let mut model = session_with_timeline(&["feat: one"]);
    model.timeline_collapsed = true;
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    assert!(
        rows.iter().any(|row| row.contains("first steps")),
        "{rows:?}"
    );
    assert_eq!(
        hits.iter()
            .filter(|(_, hit)| matches!(hit, WorkspaceHit::QuickAction(_)))
            .count(),
        render::FIRST_STEPS.len(),
        "every step is a target"
    );

    model.first_steps_collapsed = true;
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    assert!(
        rows.iter().any(|row| row.contains("first steps")),
        "the header stays: {rows:?}"
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::QuickAction(_))),
        "and its steps are folded away: {rows:?}"
    );
    assert!(
        model.shape().first_steps.collapsed,
        "and the shape this client hands back says so"
    );
}

/// The list finishes. Once every step has been taken the header offers
/// a mark that puts it away for good — and only then: a list of things
/// to try that could be dismissed before trying any of them would be
/// onboarding nobody ever sees.
#[test]
fn a_finished_list_offers_to_leave() {
    let mut model = session_with_timeline(&["feat: one"]);
    model.timeline_collapsed = true;
    model.steps_taken = [render::FIRST_STEPS[0].name()].into_iter().collect();

    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| *hit == WorkspaceHit::CloseFirstSteps),
        "unfinished, so nothing to close: {rows:?}"
    );

    model.steps_taken = render::FIRST_STEPS
        .iter()
        .map(|action| action.name())
        .collect();
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    let close = hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::CloseFirstSteps).then_some(*rect))
        .expect("finished, so the header offers the way out");
    let header = rows
        .iter()
        .position(|row| row.contains("first steps"))
        .expect("the header is drawn");
    assert_eq!(usize::from(close.y), header, "on the header itself");
    // Resolved the way an ordinary click is — `hit_rect_at`, first
    // rect wins — and not by the reversed search the modal guards use.
    // Asking the wrong one is why this shipped folding instead of
    // closing.
    model.hits = hits.clone();
    assert_eq!(
        model.hit_rect_at(close.x, close.y).map(|(_, hit)| hit),
        Some(WorkspaceHit::CloseFirstSteps),
        "and the header underneath does not swallow it"
    );

    model.first_steps_closed = true;
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(
        !rows.iter().any(|row| row.contains("first steps")),
        "closed for good, header and all: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("timeline")),
        "and the history took the rows back: {rows:?}"
    );
    assert!(model.shape().first_steps.closed);
}

/// The property the list needs, in this mode too: a step must be
/// takeable from where the list is drawn, and taking it must tick it.
///
/// Moving between agents did not. Its evidence was "the selected tab
/// changed", and this client asks the server for a tab and is answered
/// frames later — so at the moment the question was asked the answer
/// was always no, however many times the step was taken.
#[test]
fn every_first_step_is_ticked_when_it_is_taken() {
    let home = UzeHome::at(uze_testkit::temp::scratch("first-steps-ticked"));
    let (model, first, _) = two_agents_with_shells();
    let mut driven = driven(model, &home);
    driven
        .attach
        .model
        .session
        .as_mut()
        .expect("session")
        .select_tab(first);

    let keymap = uze_keys::active();
    for action in render::FIRST_STEPS {
        let chord = keymap
            .chord_for(action, render::FIRST_STEP_SCOPES)
            .unwrap_or_else(|| panic!("{action} is bound in this mode"));
        driven.press_key(key_event(chord));
        assert!(
            driven.attach.model.steps_taken.contains(&action.name()),
            "{action} was taken with {chord} and never ticked"
        );
        // Whatever it opened goes away before the next one is tried.
        driven.press_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));
    }
}

/// A step is ticked once it has been taken, and the header counts them.
#[test]
fn a_step_taken_is_marked_and_counted() {
    let mut model = session_with_timeline(&["feat: one"]);
    model.timeline_collapsed = true;
    model.steps_taken = [render::FIRST_STEPS[0].name()].into_iter().collect();
    let rows = sidebar(&model, &identities_fixture()).rows;

    let header = rows
        .iter()
        .find(|row| row.contains("first steps"))
        .expect("the section names itself");
    assert!(
        header.contains(&format!("1 of {}", render::FIRST_STEPS.len())),
        "{header:?}"
    );

    let tick = theme::glyph(theme::Symbol::MarkDone);
    let taken = rows
        .iter()
        .find(|row| row.contains(&render::FIRST_STEPS[0].label()))
        .expect("the step is listed");
    assert!(taken.contains(&tick), "{taken:?}");
    let untaken = rows
        .iter()
        .find(|row| row.contains(&render::FIRST_STEPS[1].label()))
        .expect("and so is the next one");
    assert!(!untaken.contains(&tick), "{untaken:?}");
}

/// Folded, the header carries no band. A filled row across the column
/// says "this is a heading over content", and a folded section has
/// none — two of them stacked at the foot of the sidebar read as a
/// toolbar rather than as two things you can open.
#[test]
fn a_folded_section_header_carries_no_band() {
    let mut model = session_with_timeline(&["feat: only"]);
    model.first_steps_collapsed = true;

    for (collapsed, banded) in [(false, true), (true, false)] {
        model.timeline_collapsed = collapsed;
        let buffer = sidebar(&model, &identities_fixture()).buffer;
        let rows = sidebar(&model, &identities_fixture()).rows;
        let header = rows
            .iter()
            .position(|row| row.contains("timeline"))
            .expect("the header is drawn");
        let column = rows[header]
            .chars()
            .position(|glyph| glyph == 't')
            .expect("its title") as u16;
        assert_eq!(
            buffer[(column, header as u16)].bg == theme::color(Token::SurfaceRaisedSubtle),
            banded,
            "collapsed={collapsed}: {rows:?}"
        );
    }
}

/// A section at the foot folds from the column a space's block folds
/// from, one in from the edge the way its caption is held off the other
/// one, with a row of air between the tree and the foot so a tree that
/// grows to meet it still reads as two things.
#[test]
fn the_foot_sections_stand_on_the_columns_own_grid() {
    let mut model = three_spaces();
    model.first_steps_collapsed = false;
    let Sidebar { rows, .. } = sidebar(&model, &identities_fixture());
    let column = |row: &str, text: &str| row[..row.find(text).unwrap()].chars().count();
    let chevron = theme::glyph(crate::ui::theme::Symbol::ChevronExpanded);
    let steps = rows
        .iter()
        .position(|row| row.contains("first steps"))
        .expect("the steps are at the foot");
    let space = rows
        .iter()
        .position(|row| row.contains(&format!("{chevron} one")))
        .expect("a space is open above them");

    assert_eq!(
        column(&rows[steps], &chevron),
        column(&rows[space], &chevron),
        "a section folds where a space does: {rows:?}"
    );
    assert!(
        inside(&rows[steps - 1]).trim().is_empty(),
        "a row of air over the foot: {rows:?}"
    );
}
