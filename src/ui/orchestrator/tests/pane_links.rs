//! A path a pane's program wrote: underlined under the pointer with ctrl
//! held, and opened in the code surface by ctrl+click.

use super::support::*;
use super::{WorkspaceModel, render::compute_layout};
use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
use ratatui::style::Modifier;
use std::time::{Duration, Instant};
use uze_core::UzeHome;
use uze_terminal::{Cursor, MouseMode};

const LINE: &str = "see src/main.rs or src/gone.rs now";

/// A checkout holding `src/main.rs`, and a pane standing in it showing
/// [`LINE`].
fn a_pane_naming_a_file(scratch: &str) -> (std::path::PathBuf, WorkspaceModel) {
    a_pane_naming_a_file_to(scratch, MouseMode::default())
}

/// The same, its program asking for the mouse as `mouse` says.
fn a_pane_naming_a_file_to(
    scratch: &str,
    mouse: MouseMode,
) -> (std::path::PathBuf, WorkspaceModel) {
    let root = uze_testkit::temp::scratch(scratch);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let mut model = model_of(session(&root));
    let pane = model.focused_pane();
    let columns = 80u16;
    let mut cells = vec![super::render::blank_cell(); usize::from(columns) * 24];
    for (cell, character) in cells.iter_mut().zip(LINE.chars()) {
        cell.character = character;
    }
    model.panes.insert(
        pane,
        uze_terminal::PaneSnapshot {
            pane,
            columns,
            rows: 24,
            cursor: Cursor { column: 0, row: 5 },
            alternate_screen: false,
            mouse,
            bracketed_paste: false,
            cells,
        },
    );
    (root, model)
}

/// Where `word` of [`LINE`] is drawn.
fn at(driven: &Driven<'_>, word: &str) -> (u16, u16) {
    let pane = compute_layout(
        driven.area,
        driven.attach.model.sidebar_width,
        driven.attach.model.reserved(),
    )
    .pane;
    let column = u16::try_from(LINE.find(word).unwrap()).unwrap();
    (pane.x + column + 1, pane.y)
}

/// Pumps the attach, as its loop does, until `done` or a minute passes.
fn pumped_until(driven: &mut Driven<'_>, done: impl Fn(&WorkspaceModel) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let _ = driven.pump();
        if done(&driven.attach.model) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn underlined(driven: &mut Driven<'_>, column: u16, row: u16) -> bool {
    let buffer = frame_buffer_at(&mut driven.attach.model, driven.area);
    buffer[(column, row)]
        .modifier
        .contains(Modifier::UNDERLINED)
}

#[test]
fn a_path_under_the_pointer_with_ctrl_held_is_underlined() {
    let home = UzeHome::at(uze_testkit::temp::scratch("pane-link-hover-home"));
    let (_root, model) = a_pane_naming_a_file("pane-link-hover");
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let (column, row) = at(&driven, "main.rs");

    driven.mouse(column, row, MouseEventKind::Moved);
    let _ = driven.pump();
    assert!(
        !underlined(&mut driven, column, row),
        "without ctrl it is text"
    );

    driven.mouse_holding(column, row, MouseEventKind::Moved, KeyModifiers::CONTROL);
    assert!(pumped_until(&mut driven, |model| model
        .pane_links
        .hovered
        .is_some()));
    let (start, _) = at(&driven, "src/main.rs");
    let (after, _) = at(&driven, " or");
    assert!(underlined(&mut driven, start - 1, row), "the whole path");
    assert!(underlined(&mut driven, after - 2, row));
    assert!(
        !underlined(&mut driven, after - 1, row),
        "and nothing past it"
    );

    driven.mouse(column, row, MouseEventKind::Moved);
    assert!(
        !underlined(&mut driven, column, row),
        "letting ctrl go takes it away"
    );
}

#[test]
fn a_path_that_is_not_there_is_never_underlined() {
    let home = UzeHome::at(uze_testkit::temp::scratch("pane-link-missing-home"));
    let (_root, model) = a_pane_naming_a_file("pane-link-missing");
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let (column, row) = at(&driven, "gone.rs");

    driven.mouse_holding(column, row, MouseEventKind::Moved, KeyModifiers::CONTROL);
    assert!(
        driven.attach.model.pane_links.is_asking(),
        "the disk is asked"
    );
    assert!(pumped_until(&mut driven, |model| !model
        .pane_links
        .is_asking()));
    assert!(!underlined(&mut driven, column, row));
}

#[test]
fn ctrl_clicking_a_path_opens_it_in_the_code_surface() {
    let home = UzeHome::at(uze_testkit::temp::scratch("pane-link-click-home"));
    let (root, model) = a_pane_naming_a_file("pane-link-click");
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let (column, row) = at(&driven, "main.rs");

    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        driven.mouse_holding(column, row, kind, KeyModifiers::CONTROL);
    }

    assert!(pumped_until(&mut driven, |model| {
        model.showing(uze_extensions::code::CATALOG.id)
    }));
    assert_eq!(
        code_view(&driven.attach.model).and_then(|view| view.selected()),
        Some(root.join("src/main.rs").as_path())
    );
}

#[test]
fn a_ctrl_click_on_a_word_and_a_plain_click_on_a_path_open_nothing() {
    let home = UzeHome::at(uze_testkit::temp::scratch("pane-link-word-home"));
    let (_root, model) = a_pane_naming_a_file("pane-link-word");
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();

    for (word, modifiers) in [
        ("now", KeyModifiers::CONTROL),
        ("main.rs", KeyModifiers::empty()),
    ] {
        let (column, row) = at(&driven, word);
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            driven.mouse_holding(column, row, kind, modifiers);
        }
    }
    let _ = driven.pump();

    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id)
    );
    assert!(!driven.attach.model.pane_links.is_asking());
}

/// A program that asked for the mouse (an editor, say, whose ctrl+click
/// is its own) gets a ctrl+click on a path that is not there: the click
/// was held only while the disk was asked.
#[test]
fn a_ctrl_click_on_a_path_that_is_not_there_reaches_the_program() {
    let home = UzeHome::at(uze_testkit::temp::scratch("pane-link-handback-home"));
    let (_root, model) = a_pane_naming_a_file_to(
        "pane-link-handback",
        MouseMode {
            reports_clicks: true,
            reports_drag: true,
            sgr: true,
        },
    );
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let (column, row) = at(&driven, "gone.rs");
    let _ = driven.sent();

    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        driven.mouse_holding(column, row, kind, KeyModifiers::CONTROL);
    }
    assert!(
        driven.attach.model.pane_links.is_asking(),
        "held while asked"
    );
    assert!(pumped_until(&mut driven, |model| !model
        .pane_links
        .is_asking()));

    let inputs = driven
        .sent()
        .into_iter()
        .filter(|request| matches!(request, uze_terminal::ClientRequest::Input { .. }))
        .count();
    assert_eq!(inputs, 2, "the press and the release reached the program");
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id)
    );
}
