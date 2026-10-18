//! The management modal: opening and closing it, and the keys inside it.

use super::WorkspaceHit;
use super::support::*;
use super::{
    RootPicker, WorkStateView, WorkspaceModel,
    render::{self},
};
use crate::ui::theme::{self, Token};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;
use uze_core::UzeHome;

// --- The management modal --------------------------------------------

fn manage_chord() -> uze_keys::Chord {
    uze_keys::active()
        .chord_for(
            uze_keys::Action::SwitchMode,
            &[uze_keys::Scope::Global, uze_keys::Scope::Workspace],
        )
        .expect("the modal is reachable from the keyboard")
}

/// The management surface is a modal over the workspace, not a mode
/// beside it: the action that opens it closes it again, the frame
/// draws it over everything, and the client behind it stays attached.
#[test]
fn the_manage_action_opens_the_modal_and_closes_it_again() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-toggle"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);

    driven.press_key(key_event(manage_chord()));
    assert!(driven.attach.model.manage.is_some(), "the modal opened");
    driven.frame();
    let chrome = driven
        .attach
        .model
        .manage_chrome
        .expect("the frame drew the modal");
    assert!(
        driven.attach.model.hits[..2]
            .iter()
            .any(|(rect, hit)| *hit == WorkspaceHit::ManageSurface && *rect == chrome.area),
        "the modal answers for its own rectangle ahead of everything under it"
    );
    assert!(
        driven.attach.model.session.is_some(),
        "the workspace behind it is still attached"
    );

    driven.press_key(key_event(manage_chord()));
    assert!(
        driven.attach.model.manage.is_none(),
        "the same key closes it"
    );
    driven.frame();
    assert!(driven.attach.model.manage_chrome.is_none());
}

/// The header's trailing control opens the modal; a click inside it
/// is the modal's own, a click beside it closes it.
///
/// On a terminal with room for a margin. Below `management::ROOMY_*`
/// the modal takes the whole frame and there is no beside to click —
/// the mark on its title and the chord still close it, which is the
/// trade the fill is: a small screen spends its columns on the
/// screens in front rather than on proving the workspace is behind.
#[test]
fn the_header_control_opens_the_modal_and_a_click_beside_it_closes_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-click"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
    driven.frame();
    let more = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::OpenManage).then_some(*rect))
        .expect("the header offers the modal");
    driven.press(more.x, more.y);
    assert!(
        driven.attach.model.manage.is_some(),
        "the control opened it"
    );
    assert_eq!(manage_route(&driven), crate::ui::model::Route::Overview);

    driven.frame();
    let plugins = driven
        .attach
        .model
        .manage
        .as_ref()
        .expect("open")
        .hits
        .iter()
        .find_map(|(rect, hit)| {
            matches!(
                hit,
                crate::ui::hit::Hit::Route(crate::ui::model::Route::Plugins)
            )
            .then_some(*rect)
        })
        .expect("the modal's menu lists Plugins");
    driven.press(plugins.x, plugins.y);
    assert_eq!(
        manage_route(&driven),
        crate::ui::model::Route::Plugins,
        "a click inside the modal reaches the modal"
    );

    let chrome = driven.attach.model.manage_chrome.expect("drawn");
    assert!(
        chrome.area.x > 0,
        "the modal leaves the workspace visible beside it"
    );
    driven.press(0, 0);
    assert!(
        driven.attach.model.manage.is_none(),
        "a click beside the modal closes it"
    );
    assert!(
        driven.attach.model.root_picker.is_none(),
        "and reaches nothing underneath"
    );
}

/// The mark on the modal's title closes it.
#[test]
fn the_close_mark_on_the_modal_closes_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-close-mark"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
    driven.press_key(key_event(manage_chord()));
    driven.frame();
    let close = driven.attach.model.manage_chrome.expect("drawn").close;
    driven.press(close.x, close.y);
    assert!(driven.attach.model.manage.is_none());
}

/// Inside the modal the keyboard is the modal's: a key that moves its
/// screens moves them, and one that backs out of everything backs out
/// of the modal when nothing inside it is open.
#[test]
fn keys_inside_the_modal_are_the_modals() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-keys"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
    driven.press_key(key_event(manage_chord()));
    let keymap = uze_keys::active();
    let next = keymap
        .chord_for(
            uze_keys::Action::SelectTab(2),
            &[uze_keys::Scope::Global, uze_keys::Scope::Management],
        )
        .expect("screens are walked from the keyboard");
    driven.press_key(key_event(next));
    assert_ne!(
        manage_route(&driven),
        crate::ui::model::Route::Overview,
        "the modal's own screen moved"
    );
    assert!(
        driven.attach.model.action_index.is_none()
            && driven.attach.model.agent_picker.is_none()
            && driven.attach.model.root_picker.is_none(),
        "nothing of the workspace answered"
    );

    // Esc leaves the modal. A screen's detail drawer is a column of
    // it, not a layer over it, so there is nothing for Esc to back out
    // of first — which is what makes one press enough.
    let esc = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    driven.press_key(esc);
    assert!(
        driven.attach.model.manage.is_none(),
        "Esc closes the modal from the screen it was on"
    );
}

/// The modal is about the project the operator is standing in.
///
/// The process's own directory is not that project: a shell opens at
/// home and the work is in a repository, so the Overview read its
/// prompt history — and its context status, and the project's
/// plugins — against a directory nothing had been recorded for, and
/// said "no history yet" over a full file.
#[test]
fn the_modal_is_about_the_space_it_was_opened_over() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-root"));
    let mut driven = driven(agent_session_in("/repo"), &home);
    driven.press_key(key_event(manage_chord()));

    assert_eq!(
        driven
            .attach
            .model
            .manage
            .as_ref()
            .expect("the modal opened")
            .context_root,
        PathBuf::from("/repo"),
        "the modal speaks about the space, not about where uze was started"
    );
}

/// The modal reopens on the screen it was closed on, and the layout
/// the client hands back carries that screen for the next run.
#[test]
fn the_modal_reopens_where_it_was_closed() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-memory"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
    driven.press_key(key_event(manage_chord()));
    let next = uze_keys::active()
        .chord_for(
            uze_keys::Action::SelectTab(2),
            &[uze_keys::Scope::Global, uze_keys::Scope::Management],
        )
        .expect("bound");
    driven.press_key(key_event(next));
    let moved_to = manage_route(&driven);
    driven.press_key(key_event(manage_chord()));
    assert_eq!(
        driven.attach.model.management_layout.route.as_deref(),
        Some(moved_to.id()),
        "closing keeps the screen in the layout the client owns"
    );
    assert_eq!(
        driven.attach.model.shape().management.route.as_deref(),
        Some(moved_to.id()),
        "and it is what the layout file is written from"
    );

    driven.press_key(key_event(manage_chord()));
    assert_eq!(manage_route(&driven), moved_to, "reopening lands on it");
}

/// The sidebar opens with the surface's name, the way to grow it and the
/// way into the other one, a rule between the two controls.
#[test]
fn the_sidebar_header_names_the_column_and_offers_the_modal() {
    let mut model = agent_with_task(WorkStateView::Ready, 1);
    let rows = frame_rows(&mut model);
    let header = &rows[0];
    assert!(header.contains("work"), "the surface is named: {header:?}");
    assert!(
        header.contains(&theme::glyph(crate::ui::theme::Symbol::Manage)),
        "the control that opens the modal ends the row: {header:?}"
    );
    assert!(
        rows[1]
            .trim_start()
            .starts_with(&theme::glyph(crate::ui::theme::Symbol::TreeDivider).repeat(4)),
        "a hairline closes the header: {:?}",
        rows[1]
    );
    assert!(
        header.contains("+ space"),
        "the way to grow the column rides the header: {header:?}"
    );
    let rule = header
        .find(&theme::glyph(crate::ui::theme::Symbol::TreeColumnDivider))
        .expect("a rule between the two controls");
    assert!(
        header[..rule].contains("+ space"),
        "with the rule between them: {header:?}"
    );
}

/// The word is where the prompt came from: while that prompt is open it
/// is spent, and says so by going quiet until it closes.
#[test]
fn the_way_to_grow_the_column_goes_quiet_while_its_prompt_is_open() {
    let hue_of_new = |model: &mut WorkspaceModel| {
        let area = Rect::new(0, 0, 80, 24);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render::render(
                    frame,
                    model,
                    &identities_fixture(),
                    &mut Vec::new(),
                    &mut render::FrameMetrics::default(),
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let header = buffer_rows(&buffer)[0].clone();
        let column = header.find("+ space").expect("the control is drawn") as u16;
        buffer[(column, 0)].fg
    };
    let mut model = agent_with_task(WorkStateView::Ready, 1);
    assert_eq!(
        hue_of_new(&mut model),
        theme::color(Token::AccentMuted),
        "held back at rest"
    );

    model.hovered = Some(WorkspaceHit::NewSpace);
    assert_eq!(
        hue_of_new(&mut model),
        theme::color(Token::Accent),
        "the pointer restores the accent"
    );

    model.root_picker = Some(RootPicker::opened_in("~", None));

    assert_eq!(
        hue_of_new(&mut model),
        theme::color(Token::TextMuted),
        "spent while the prompt it opened is open"
    );
}

/// A toast raised while the management modal is open is drawn over
/// it and put away from there, leaving the modal open. Beneath the
/// modal's scrim it could be neither read nor reached.
#[test]
fn a_toast_over_the_management_modal_is_drawn_and_answers_there() {
    use crate::ui::widget::ToastKind;

    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-toast-over-manage"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
    driven.press_key(key_event(manage_chord()));
    driven
        .attach
        .model
        .raise_toast(ToastKind::Done, "installed", "the plugin", None);
    driven.frame();
    let close = toast_close(&driven);
    assert_eq!(
        driven.attach.model.hit_at(close.x, close.y),
        Some(WorkspaceHit::DismissToast(0)),
        "nothing the modal registered stands over the toast"
    );

    driven.press(close.x, close.y);

    assert!(
        driven.attach.model.toast_stack().is_empty(),
        "the toast went"
    );
    assert!(driven.attach.model.manage.is_some(), "and the modal stayed");
}
