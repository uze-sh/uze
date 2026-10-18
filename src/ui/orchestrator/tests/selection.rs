//! Selecting with a drag and copying it, over a pane and over the surfaces.

use super::WorkspaceHit;
use super::support::*;
use super::{Viewport, WorkspaceModel, render::compute_layout};
use crossterm::event::{MouseButton, MouseEventKind};
use std::path::{Path, PathBuf};
use uze_core::UzeHome;
use uze_extensions::view::ViewHit;
use uze_terminal::{ClientEvent, ClientRequest, Cursor, MouseMode, SelectionGesture};

/// A pane showing `line` on its first row, whose program asked for
/// mouse reports or did not.
fn pane_showing(model: &mut WorkspaceModel, line: &str, reports_clicks: bool) {
    let pane = model.focused_pane();
    let columns = 80u16;
    let mut cells = vec![super::render::blank_cell(); usize::from(columns) * 24];
    for (cell, character) in cells.iter_mut().zip(line.chars()) {
        cell.character = character;
    }
    model.panes.insert(
        pane,
        uze_terminal::PaneSnapshot {
            pane,
            columns,
            rows: 24,
            cursor: Cursor { column: 0, row: 0 },
            alternate_screen: false,
            mouse: MouseMode {
                reports_clicks,
                reports_drag: reports_clicks,
                sgr: true,
            },
            bracketed_paste: false,
            cells,
        },
    );
}

fn drag_across_the_first_word(driven: &mut Driven<'_>, modifiers: crossterm::event::KeyModifiers) {
    driven.frame();
    let pane = compute_layout(driven.area, driven.attach.model.sidebar_width).pane;
    for (column, kind) in [
        (pane.x, MouseEventKind::Down(MouseButton::Left)),
        (pane.x + 4, MouseEventKind::Drag(MouseButton::Left)),
        (pane.x + 4, MouseEventKind::Up(MouseButton::Left)),
    ] {
        let mut event = mouse_at(column, pane.y, kind);
        event.modifiers = modifiers;
        let layout = compute_layout(driven.area, driven.attach.model.sidebar_width);
        let viewport = Viewport {
            size: ratatui::layout::Size::new(driven.area.width, driven.area.height),
            columns: layout.pane.width,
            rows: layout.pane.height,
            layout,
        };
        let _ = driven
            .attach
            .handle(crossterm::event::Event::Mouse(event), &viewport);
    }
}

/// The server's answer to the copy a release asks for.
fn copy_answered(driven: &mut Driven<'_>, text: &str) {
    let pane = driven.attach.model.focused_pane();
    driven
        .events_sender
        .as_ref()
        .expect("the runtime is still there")
        .send(ClientEvent::SelectionText {
            pane,
            text: text.to_owned(),
        })
        .unwrap();
    driven.pump();
}

/// Putting the "copied" toast away copies nothing again.
///
/// The pane's selection stays drawn after its copy, and the release of
/// the click on the toast's `✕` used to reach it — copying it once more
/// and raising the toast that click had just dismissed, on every click.
#[test]
fn dismissing_the_copied_toast_does_not_copy_again() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-toast-recopy"));
    let mut model = model_of(session("/tmp"));
    pane_showing(&mut model, "hello world", false);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    drag_across_the_first_word(&mut driven, crossterm::event::KeyModifiers::empty());
    copy_answered(&mut driven, "hello");
    assert_eq!(driven.attach.model.toast_stack().len(), 1, "it said so");
    let _ = driven.sent();

    driven.frame();
    let close = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, WorkspaceHit::DismissToast(_)))
        .map(|(rect, _)| *rect)
        .expect("the toast registered a target");
    driven.press(close.x, close.y);
    driven.mouse(close.x, close.y, MouseEventKind::Up(MouseButton::Left));

    assert!(
        driven.attach.model.toast_stack().is_empty(),
        "the toast went"
    );
    assert!(
        !driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::CopySelection { .. })),
        "and the selection under it was not copied again"
    );
}

/// The code surface answers the same gesture a pane does: press on a
/// file's text, drag, let go, and what was passed over is on the
/// clipboard.
#[test]
fn releasing_a_drag_over_the_code_surface_copies_what_it_covered() {
    use uze_extensions::{DirEntry, ExtensionHit, Unreadable, code};

    /// One file in one directory, answered from memory: what is under
    /// test is where the pointer lands, not a read.
    struct OneFile;
    impl uze_extensions::Host for OneFile {
        fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
            Err("no git here".to_owned())
        }
        fn repository_root(&self, _: &Path) -> Result<PathBuf, String> {
            Err("no git here".to_owned())
        }
        fn read_file(&self, _: &Path) -> Result<String, Unreadable> {
            Ok("hello world\nsecond\n".to_owned())
        }
        fn list_dir(&self, _: &Path) -> Result<Vec<DirEntry>, String> {
            Ok(vec![DirEntry {
                directory: false,
                name: "notes.txt".to_owned(),
            }])
        }
        fn write_file(&self, _root: &Path, _: &Path, _: &str) -> Result<(), String> {
            Err("read only".to_owned())
        }
        fn delete_file(&self, _root: &Path, _: &Path) -> Result<(), String> {
            Err("read only".to_owned())
        }
        fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
            Err("read only".to_owned())
        }
        fn syntax_theme(&self) -> String {
            String::new()
        }
    }

    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-code-copy-on-select",
    ));
    let mut model = model_of(session("/tmp"));
    let mut view = code::CodeView::opening(
        PathBuf::from("/w"),
        "/w".to_owned(),
        code::ContentMode::Contents,
    );
    let settle = |view: &mut code::CodeView| {
        while let Some(request) = view.take_request() {
            view.absorb(code::fulfill(&OneFile, request));
        }
    };
    settle(&mut view);
    let space = uze_extensions::view::Size {
        width: 60,
        height: 20,
    };
    code::handle_command(&mut view, uze_extensions::view::Command::Activate, space);
    settle(&mut view);
    model.code = Some(view);
    let mut driven = driven(model, &home);
    driven.frame();
    let first_line = driven.hit(|hit| {
        matches!(
            hit,
            WorkspaceHit::Extension(ExtensionHit::Code(ViewHit::PlaceCaret { line: 0, .. }))
        )
    });
    let text = first_line.x + driven.attach.model.code_scrollbars.content_gutter;

    driven.press(text, first_line.y);
    driven.mouse(
        text + 4,
        first_line.y,
        MouseEventKind::Drag(MouseButton::Left),
    );
    driven.mouse(
        text + 4,
        first_line.y,
        MouseEventKind::Up(MouseButton::Left),
    );

    assert_eq!(driven.attach.model.clipboard.as_deref(), Some("hello"));
    assert!(
        matches!(
            &driven.attach.model.selection,
            Some(crate::ui::selection::Selection::Text(marking)) if !marking.held()
        ),
        "the drag is over, and what was taken stays drawn"
    );

    // Typing into the file, the caret goes with the drag while it is
    // still held, so what is typed next lands where the drag ended.
    // The copy's toast is put away first: it stands over the pane's
    // top-right corner, where the file's first line now is.
    driven.attach.model.dismiss_toast(0);
    if let Some(view) = driven.attach.model.code.as_mut() {
        code::handle_command(view, uze_extensions::view::Command::Edit, space);
    }
    driven.frame();
    let row_of = |driven: &Driven<'_>, line: usize| {
        driven
            .attach
            .model
            .code_scrollbars
            .text_rows
            .iter()
            .find(|row| row.line == line)
            .cloned()
            .unwrap_or_else(|| panic!("line {line} is drawn"))
    };
    let (first, second) = (row_of(&driven, 0), row_of(&driven, 1));
    driven.press(first.glyphs[6].x, first.area.y);
    driven.mouse(
        second.glyphs[2].x,
        second.area.y,
        MouseEventKind::Drag(MouseButton::Left),
    );
    let caret = match code::view(driven.attach.model.code.as_ref().unwrap(), space).content {
        uze_extensions::view::Content::Lines { caret, .. } => caret,
        _ => None,
    };
    assert_eq!(
        caret,
        Some(uze_extensions::view::Caret { line: 1, column: 2 })
    );
    driven.mouse(
        second.glyphs[2].x,
        second.area.y,
        MouseEventKind::Up(MouseButton::Left),
    );
    assert_eq!(driven.attach.model.clipboard.as_deref(), Some("world\nsec"));
}

#[test]
fn releasing_a_drag_over_a_pane_copies_what_it_covered() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-copy-on-select"));
    let mut model = model_of(session("/tmp"));
    pane_showing(&mut model, "hello world", false);
    let mut driven = driven(model, &home);
    let pane = driven.attach.model.focused_pane();

    drag_across_the_first_word(&mut driven, crossterm::event::KeyModifiers::empty());

    assert_eq!(
        driven.sent(),
        [
            ClientRequest::Select {
                pane,
                gesture: SelectionGesture::Begin {
                    anchor: (0, 0),
                    head: (4, 0),
                },
            },
            ClientRequest::Select {
                pane,
                gesture: SelectionGesture::Release,
            },
            ClientRequest::CopySelection { pane },
        ],
        "the server holds the selection, so the view can scroll under it"
    );
    copy_answered(&mut driven, "hello");
    assert_eq!(driven.attach.model.clipboard.as_deref(), Some("hello"));
    assert!(
        driven.attach.model.selection.is_some(),
        "what was taken stays drawn after the release"
    );
    driven.press_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(
        driven.attach.model.selection.is_none(),
        "and the next key puts it away"
    );
    assert!(
        driven.sent().contains(&ClientRequest::Select {
            pane,
            gesture: SelectionGesture::Clear,
        }),
        "on the server too"
    );
}

#[test]
fn a_drag_is_a_selection_even_over_a_program_that_owns_the_mouse() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-copy-over-mouse"));
    let mut model = model_of(session("/tmp"));
    pane_showing(&mut model, "hello world", true);
    let mut driven = driven(model, &home);

    drag_across_the_first_word(&mut driven, crossterm::event::KeyModifiers::empty());

    assert!(
        forwarded_input(&mut driven).is_empty(),
        "the program is not told about a drag it did not get"
    );
}

#[test]
fn shift_hands_the_drag_to_a_program_that_owns_the_mouse() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-shift-drag"));
    let mut model = model_of(session("/tmp"));
    pane_showing(&mut model, "hello world", true);
    let mut driven = driven(model, &home);

    drag_across_the_first_word(&mut driven, crossterm::event::KeyModifiers::SHIFT);

    assert_eq!(driven.attach.model.clipboard, None);
    assert_eq!(
        forwarded_input(&mut driven).len(),
        3,
        "press, drag, release"
    );
}

#[test]
fn a_click_still_reaches_a_program_that_owns_the_mouse() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-held-click"));
    let mut model = model_of(session("/tmp"));
    pane_showing(&mut model, "hello world", true);
    let mut driven = driven(model, &home);
    driven.frame();
    let pane = compute_layout(driven.area, driven.attach.model.sidebar_width).pane;

    driven.press(pane.x + 2, pane.y);
    assert!(
        forwarded_input(&mut driven).is_empty(),
        "held until it cannot be the start of a drag"
    );
    driven.mouse(pane.x + 2, pane.y, MouseEventKind::Up(MouseButton::Left));

    assert_eq!(
        forwarded_input(&mut driven),
        vec![b"\x1b[<0;3;1M".to_vec(), b"\x1b[<0;3;1m".to_vec()]
    );
    assert_eq!(driven.attach.model.clipboard, None);
    assert!(driven.attach.model.selection.is_none());
}

#[test]
fn a_drag_over_blanks_is_not_a_click_where_it_ended() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-blank-drag"));
    let mut model = model_of(session("/tmp"));
    pane_showing(&mut model, "", true);
    let mut driven = driven(model, &home);

    drag_across_the_first_word(&mut driven, crossterm::event::KeyModifiers::empty());

    assert!(
        forwarded_input(&mut driven).is_empty(),
        "the program was not handed a click the operator never made"
    );
    copy_answered(&mut driven, "");
    assert_eq!(driven.attach.model.clipboard, None);
    assert!(
        driven.attach.model.selection.is_none(),
        "a selection of blanks is not kept drawn"
    );
}

#[test]
fn a_drag_past_the_top_of_a_full_screen_program_scrolls_it_with_the_wheel() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-drag-past-top"));
    let mut model = model_of(session("/tmp"));
    pane_showing(&mut model, "hello world", true);
    let focused = model.focused_pane();
    if let Some(snapshot) = model.panes.get_mut(&focused) {
        snapshot.alternate_screen = true;
    }
    let mut driven = driven(model, &home);
    driven.frame();
    let pane = compute_layout(driven.area, driven.attach.model.sidebar_width).pane;

    driven.press(pane.x + 2, pane.y + 1);
    driven.mouse(
        pane.x + 2,
        pane.y - 1,
        MouseEventKind::Drag(MouseButton::Left),
    );

    let sent = driven.sent();
    assert!(
        !sent
            .iter()
            .any(|request| matches!(request, ClientRequest::Scroll { .. })),
        "the alternate screen has no scrollback to move: {sent:?}"
    );
    let wheel = sent.iter().find_map(|request| match request {
        ClientRequest::Input { bytes, .. } => Some(bytes.clone()),
        _ => None,
    });
    assert_eq!(
        wheel.as_deref(),
        Some(&b"\x1b[<64;3;1M"[..]),
        "the program scrolls itself, at the edge the pointer overshot"
    );
}

fn forwarded_input(driven: &mut Driven<'_>) -> Vec<Vec<u8>> {
    driven
        .sent()
        .into_iter()
        .filter_map(|request| match request {
            ClientRequest::Input { bytes, .. } => Some(bytes),
            _ => None,
        })
        .collect()
}
