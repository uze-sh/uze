//! The Keys screen: the keymap as a list to read, scroll and rebind.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{Focus, Route, TuiModel};
use crate::ui::theme::{self, Token};
use crate::ui::worker::Intent;

use super::support::*;

/// The screen exists to be driven by the pointer: a screen about
/// rebinding that could only be worked by the bindings it is rebinding
/// would be a joke on itself.
#[test]
fn the_keys_screen_rebinds_from_a_click_and_a_keystroke() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        keyboard: crate::ui::keys::KeyboardSupport { enhanced: false },
        ..TuiModel::default()
    };
    let row = model
        .key_rows()
        .iter()
        .position(|row| row.action == uze_keys::Action::NewShellTab)
        .expect("the workspace's new-shell key is listed");
    model.key_screen.selected = row;

    let (_terminal, hits) = drawn_at(&model, 120, 40);
    model.hits = hits;
    let (rect, _) = model
        .hits
        .iter()
        .find(|(_, hit)| *hit == crate::ui::hit::Hit::OfferedAction(uze_keys::Action::ChangeKey))
        .expect("changing a key is a target, not only a keystroke")
        .clone();
    model.click(rect.x, rect.y);
    assert!(model.keys_capture, "the screen is waiting for a key");

    let intent = model.apply_key(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
    assert_eq!(
        intent,
        Intent::PersistKeymap,
        "a rebinding is remembered past this run"
    );
    assert!(!model.keys_capture);
    assert_eq!(
        uze_keys::active().chord_for(uze_keys::Action::NewShellTab, &[uze_keys::Scope::Workspace]),
        uze_keys::Chord::parse("f4").ok()
    );
    // And the screen now says it is the operator's own choice.
    assert!(
        model
            .key_rows()
            .iter()
            .any(|row| row.action == uze_keys::Action::NewShellTab && row.custom())
    );
    uze_keys::set_active(uze_keys::default_keymap().clone());
}

/// The list is long — a row per surface an action can be reached from —
/// so the window follows the selection. It did not, and every key past the
/// first screenful was invisible and unreachable at the same time.
#[test]
fn the_keys_list_follows_the_selection_past_the_fold() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let rows = model.key_rows();
    assert!(
        rows.len() > 60,
        "the premise: this list is far taller than any terminal"
    );
    let last = rows.len() - 1;
    model.key_screen.selected = last;
    let wanted = rows[last].action.label();

    let (terminal, hits) = drawn_at(&model, 140, 30);
    let drawn = buffer_rows(&terminal);
    assert!(
        drawn.iter().any(|row| row.contains(&wanted)),
        "the last key is on screen: {wanted}"
    );
    assert!(
        hits.iter()
            .any(|(_, hit)| *hit == crate::ui::hit::Hit::KeyRow(last)),
        "and it is a target, so the mouse reaches it too"
    );
    // The heading it belongs under travels with it — a key on screen under
    // no group is a key you cannot place.
    assert!(
        drawn
            .iter()
            .any(|row| row.contains(&rows[last].scope.heading().to_lowercase())),
        "{drawn:#?}"
    );
}

/// The selected key is a filled band the width of the list, the way every
/// other list in this mode marks its selection — not a brighter word
/// inside a row that otherwise looks like all the others.
#[test]
fn the_selected_key_is_a_band_across_the_list() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    model.key_screen.selected = 2;
    let (terminal, hits) = drawn_at(&model, 140, 30);

    let (rect, _) = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::KeyRow(2))
        .expect("the selected key was drawn");
    let buffer = terminal.backend().buffer();
    let filled = (rect.x..rect.right())
        .filter(|column| {
            theme::token_of(buffer[(*column, rect.y)].bg) == Some(Token::SurfaceSelected)
        })
        .count();
    assert_eq!(
        filled,
        usize::from(rect.width),
        "every column of the row, not only the words on it"
    );

    let above = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::KeyRow(1))
        .expect("its neighbour was drawn too")
        .0;
    assert_ne!(
        theme::token_of(buffer[(above.x, above.y)].bg),
        Some(Token::SurfaceSelected),
        "and only that row"
    );
}

/// The handle on the drawer's rule answers like a scrollbar: taken hold
/// of and dragged, it moves the page — never the selection — and lets go
/// when the button does.
#[test]
fn the_keys_handle_can_be_dragged() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let (_terminal, hits) = drawn_at(&model, 140, 30);
    model.hits = hits;
    let (handle, track) = model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            Hit::KeysTrack(track) => Some((*rect, *track)),
            _ => None,
        })
        .expect("the handle is a target");
    assert_eq!(handle.width, 1, "the handle, not the whole rule");

    let drag = |model: &mut TuiModel, kind, row| {
        model.apply_mouse(
            MouseEvent {
                kind,
                column: track.x,
                row,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 140, 40),
        );
    };
    let first = |model: &TuiModel| model.key_screen.scroll.get().first;
    model.click(handle.x, handle.y);
    drag(
        &mut model,
        MouseEventKind::Drag(MouseButton::Left),
        track.bottom() - 1,
    );
    let bottom = first(&model);
    assert!(bottom > 0, "dragging down scrolled nothing");
    assert_eq!(model.key_screen.selected, 0, "the drag moved the selection");
    drag(&mut model, MouseEventKind::Drag(MouseButton::Left), track.y);
    assert_eq!(first(&model), 0, "back to the top");

    drag(&mut model, MouseEventKind::Up(MouseButton::Left), track.y);
    drag(
        &mut model,
        MouseEventKind::Drag(MouseButton::Left),
        track.bottom() - 1,
    );
    assert_eq!(first(&model), 0, "the drag was let go of");
}

/// A long list that gives no sign of being long is a list nobody scrolls.
/// The track says both things at once: that there is more, and where in it
/// the window sits.
#[test]
fn a_list_taller_than_the_screen_says_where_the_window_is() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let thumb = theme::glyph(theme::Symbol::ScrollThumb);
    let column = |terminal: &Terminal<TestBackend>| -> Vec<usize> {
        buffer_rows(terminal)
            .into_iter()
            .enumerate()
            .filter(|(_, row)| row.contains(&thumb))
            .map(|(index, _)| index)
            .collect()
    };

    let (mut terminal, mut hits) = drawn_at(&model, 140, 30);
    let top = column(&terminal);
    assert!(!top.is_empty(), "the track is drawn at all");

    model.key_screen.selected = model.key_rows().len() - 1;
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    let bottom = column(&terminal);
    assert!(
        bottom.first() > top.first(),
        "and it moved down with the window: {top:?} -> {bottom:?}"
    );
    assert!(
        !bottom.is_empty() && bottom.len() < 20,
        "a fraction of the track, not all of it: {bottom:?}"
    );

    // A list that fits gets none: a scrollbar on a full view says the
    // opposite of what it is for.
    let short = TuiModel {
        route: Route::Profiles,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &short, false, &mut hits);
        })
        .unwrap();
    assert!(column(&terminal).is_empty());
}

/// The wheel moves the Keys page and leaves the selection where the keys
/// put it; the next key brings the page back to it.
#[test]
fn the_wheel_scrolls_the_keys_page_and_the_keys_bring_it_back() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let wheel = |model: &mut TuiModel, kind| {
        model.apply_mouse(
            MouseEvent {
                kind,
                column: 60,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 100, 40),
        );
    };
    let shows = |model: &TuiModel, index: usize| {
        let wanted = model.key_rows()[index].action.label();
        let (terminal, _hits) = drawn_at(model, 140, 30);
        // The list's half: the drawer names the selected action too.
        buffer_rows(&terminal)
            .iter()
            .any(|row| row.chars().take(70).collect::<String>().contains(&wanted))
    };

    drawn_at(&model, 140, 30);
    for _ in 0..40 {
        wheel(&mut model, MouseEventKind::ScrollDown);
    }
    assert_eq!(
        model.key_screen.selected, 0,
        "the wheel moved the selection"
    );
    assert!(!shows(&model, 0), "the page did not move");

    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(model.key_screen.selected, 1);
    assert!(shows(&model, 1), "the key did not bring the page back");

    // Profiles was the other screen the wheel could not move, for the same
    // reason: its selection is three panels rather than one list, and the
    // mover the wheel called knew about neither.
    let mut profiles = TuiModel {
        route: Route::Profiles,
        focus: Focus::Content,
        ..model_with_data()
    };
    assert!(
        profiles.remembered.profiles.len() > 1,
        "there is somewhere to move to"
    );
    wheel(&mut profiles, MouseEventKind::ScrollDown);
    assert_eq!(
        profiles.remembered.profiles_selected, 1,
        "the wheel moved it"
    );
}

/// A group opens with a blank line and its keys sit in from its name.
/// Without either, the headings read as rows in a different colour and the
/// whole screen reads as one block of text.
#[test]
fn a_group_of_keys_is_set_apart_from_the_one_above_it() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let (terminal, _hits) = drawn_at(&model, 140, 40);
    let drawn = buffer_rows(&terminal);
    // Columns rather than byte offsets: these rows carry the sidebar's own
    // glyphs, and half of them are more than one byte wide.
    let column_of = |row: &str, needle: &str| {
        row.find(needle)
            .map(|byte| row[..byte].chars().count())
            .unwrap_or_else(|| panic!("{needle} is not on {row:?}"))
    };
    let heading = drawn
        .iter()
        .position(|row| {
            row.split('│')
                .next()
                .is_some_and(|list| list.trim() == "management")
        })
        .expect("the second group is on screen");
    let name = column_of(&drawn[heading], "management");
    let above: String = drawn[heading - 1].chars().skip(name).take(20).collect();
    assert!(
        above.trim().is_empty(),
        "a blank line opens it: {:?}",
        drawn[heading - 1]
    );
    // Where the row's own content begins, measured from the column the
    // heading begins at — not from any one glyph, since the marker column
    // is blank on every row but the selected one.
    let inset = drawn[heading + 1]
        .chars()
        .skip(name)
        .take_while(|glyph| *glyph == ' ')
        .count();
    assert!(
        inset > 0,
        "and its keys sit in from it: {:?} / {:?}",
        drawn[heading],
        drawn[heading + 1]
    );
}

/// The list says what each action does, not only what it is called. The
/// sentence lived in the drawer alone, which made the list a column of
/// labels you had to open one at a time to read.
#[test]
fn a_key_is_listed_with_the_sentence_that_explains_it() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let (terminal, _hits) = drawn_at(&model, 160, 40);
    let drawn = buffer_rows(&terminal);
    let row = drawn
        .iter()
        .find(|row| row.contains("Manage"))
        .expect("the mode key is on screen");
    assert!(row.contains("Open or close the"), "{row:?}");

    // Narrow enough and the sentence goes rather than being cut to a stub.
    let mut narrow = Terminal::new(TestBackend::new(92, 40)).unwrap();
    let mut hits = Vec::new();
    narrow
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    let row = buffer_rows(&narrow)
        .into_iter()
        .find(|row| row.contains("Manage"))
        .expect("the mode key is still on screen");
    assert!(!row.contains("Open or close"), "{row:?}");
}

/// Everything that could be wrong with a key is said before anything is
/// written. A screen that let someone lock themselves out would be worse
/// than one with no rebinding at all.
#[test]
fn a_key_that_would_break_something_is_refused_with_the_reason() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        keyboard: crate::ui::keys::KeyboardSupport { enhanced: false },
        ..TuiModel::default()
    };
    let row = model
        .key_rows()
        .iter()
        .position(|row| row.action == uze_keys::Action::NewShellTab)
        .expect("listed");
    model.key_screen.selected = row;
    model.keys_capture = true;

    // `alt+g` already opens the changes in this same keyboard.
    model.apply_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::ALT));
    assert!(
        model
            .keys_problem
            .as_deref()
            .is_some_and(|problem| problem.contains("alt+g")),
        "{:?}",
        model.keys_problem
    );
    assert!(model.keys_capture, "still asking — nothing was written");
    assert_eq!(
        uze_keys::active().chord_for(uze_keys::Action::NewShellTab, &[uze_keys::Scope::Workspace]),
        uze_keys::Chord::parse("ctrl+t").ok()
    );

    // And whatever arrives is reported, which is the only honest answer
    // to "will this key reach uze on my terminal".
    assert!(
        model
            .keys_probe
            .as_deref()
            .is_some_and(|probe| probe.contains("alt+g")),
        "{:?}",
        model.keys_probe
    );
}

/// A chord this terminal has no way of sending is refused rather than
/// accepted and left looking alive.
#[test]
fn a_key_this_terminal_cannot_send_is_never_bound() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        keyboard: crate::ui::keys::KeyboardSupport { enhanced: false },
        ..TuiModel::default()
    };
    model.keys_capture = true;
    // Ctrl+digit has no encoding at all without the enhancement protocol.
    model.apply_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL));
    assert!(
        model
            .keys_problem
            .as_deref()
            .is_some_and(|problem| problem.contains("cannot send")),
        "{:?}",
        model.keys_problem
    );
}

/// The pointer lights the Keys row it is over and chooses nothing.
#[test]
fn hovering_a_key_lights_its_row_without_selecting_it() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    let (_terminal, hits) = drawn_at(&model, 140, 30);
    model.hits = hits;
    let (row, index) = model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            Hit::KeyRow(index) if *index == 2 => Some((*rect, *index)),
            _ => None,
        })
        .expect("the third key is drawn");
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: row.x + 2,
            row: row.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 140, 30),
    );
    assert_eq!(model.key_screen.selected, 0, "hovering selected the row");
    let (terminal, _) = drawn_at(&model, 140, 30);
    assert_eq!(
        terminal.backend().buffer()[(row.x + 1, row.y)].bg,
        theme::color(Token::SurfaceHover),
        "the row under the pointer is not lit (key {index})"
    );
}
