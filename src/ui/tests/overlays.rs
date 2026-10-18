//! Dialogs, the action index, release notes and the footer's hints.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{Confirmation, Focus, Overlay, Route, TuiModel};
use crate::ui::theme::{self, Token};
use crate::ui::worker::Intent;

use super::support::*;

/// The arrows move between a question's answers, and enter takes the one
/// the keyboard is on — the way out included.
#[test]
fn the_arrows_choose_an_answer_and_enter_takes_it() {
    let mut model = model_with_plugins(&["one"]);
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    model.apply_key(key(KeyCode::Enter));
    model.apply_key(key(KeyCode::Left));
    assert_eq!(model.apply_key(key(KeyCode::Enter)), Intent::None);
    assert_eq!(model.overlay, Overlay::None, "enter on cancel declines");

    model.apply_key(key(KeyCode::Enter));
    model.apply_key(key(KeyCode::Left));
    model.apply_key(key(KeyCode::Right));
    assert_eq!(
        model.apply_key(key(KeyCode::Enter)),
        Intent::Remove("one".to_owned())
    );
}

#[test]
fn click_outside_overlay_dismisses_without_confirming() {
    let mut model = model_with_plugins(&["one"]);
    model.overlay = Overlay::Confirm {
        kind: Confirmation::RemovePlugin("one".to_owned()),
        focus: Some(1),
    };
    let intent = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 100, 40),
    );
    assert_eq!(
        intent,
        Intent::None,
        "a stray click must never confirm a destructive action"
    );
    assert_eq!(model.overlay, Overlay::None);
}

/// One key opens the index, and it is the same key in both modes. It used
/// to be F1 in the workspace and `?` here — and since a surface prints the
/// innermost chord it can find, the key that worked in both was the one
/// never shown.
#[test]
fn the_index_opens_and_closes_on_the_one_key_both_modes_share() {
    let mut model = TuiModel::default();
    model.apply_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert!(
        matches!(model.overlay, Overlay::ActionIndex { .. }),
        "{:?}",
        model.overlay
    );
    model.apply_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(model.overlay, Overlay::None);

    // And it is what management advertises, rather than a second key of
    // its own that the workspace would not answer.
    assert_eq!(
        uze_keys::active().chord_for(
            uze_keys::Action::OpenActionIndex,
            &[uze_keys::Scope::Global, uze_keys::Scope::Management],
        ),
        uze_keys::Chord::parse("f1").ok()
    );
}

/// Nothing the index prints is written down: the words come from the
/// action and the key from the keymap. The list it replaced was typed by
/// hand and had already fallen out of step with the dispatcher for nine
/// of its bindings.
#[test]
fn the_index_prints_the_key_the_keymap_actually_binds() {
    let mut model = model_with_plugins(&["one"]);
    model.focus = Focus::Content;
    model.act(uze_keys::Action::OpenActionIndex);
    let Overlay::ActionIndex { scopes, .. } = model.overlay.clone() else {
        panic!("the index is open");
    };
    let rows = model.action_index_rows(&scopes, "");
    let keymap = uze_keys::active();
    for (action, chord) in &rows {
        assert_eq!(
            *chord,
            keymap.chord_for(*action, &scopes),
            "the index invented a key for {action}"
        );
    }
    assert!(
        rows.iter()
            .any(|(action, _)| *action == uze_keys::Action::RemovePlugin),
        "what this screen can do is on offer: {rows:?}"
    );
    assert!(
        rows.iter()
            .any(|(action, chord)| *action == uze_keys::Action::SwitchMode && chord.is_some()),
        "and so is what is reachable from everywhere"
    );
}

/// Typing narrows, and choosing a row performs it — so someone who does
/// not know the keyboard uses the index as a menu, and reads the key off
/// the row they just used.
#[test]
fn the_index_narrows_as_you_type_and_performs_what_you_choose() {
    let mut model = model_with_plugins(&["one"]);
    model.focus = Focus::Content;
    model.act(uze_keys::Action::OpenActionIndex);
    for character in "remove".chars() {
        model.apply_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    let Overlay::ActionIndex { scopes, filter, .. } = model.overlay.clone() else {
        panic!("still open");
    };
    assert_eq!(filter, "remove");
    let rows = model.action_index_rows(&scopes, &filter);
    assert!(
        rows.iter().all(
            |(action, _)| action.label().to_lowercase().contains("remove")
                || action.description().to_lowercase().contains("remove")
        ),
        "{rows:?}"
    );
    model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        matches!(
            model.overlay,
            Overlay::Confirm {
                kind: Confirmation::RemovePlugin(_),
                ..
            }
        ),
        "choosing from the index performs it: {:?}",
        model.overlay
    );
}

/// A dialog open inside the manage modal recedes the modal's own chrome
/// with the rest: its title row sits outside the surface `render` dims, and
/// was left lit above the scrim.
#[test]
fn a_dialog_in_the_manage_modal_recedes_its_title_too() {
    let title_style = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal
            .draw(|frame| {
                crate::ui::management::render_modal(
                    frame,
                    frame.area(),
                    model,
                    false,
                    &mut Vec::new(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let (x, y) = (0..buffer.area.height)
            .find_map(|y| {
                let row: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                row.find(" manage ").map(|at| (at as u16 + 1, y))
            })
            .expect("the modal's title");
        buffer[(x, y)].fg
    };
    let mut model = model_with_plugins(&["flow"]);
    let lit = title_style(&model);
    model.overlay = Overlay::ReleaseNotes(crate::ui::release_notes::ReleaseNotesModal::opening(
        "9.0.1",
    ));
    assert_ne!(
        title_style(&model),
        lit,
        "the title recedes behind the dialog"
    );
}

/// The footer's version is the way to this release's own notes: brighter
/// than the hints beside it, and a click opens them.
#[test]
fn the_footers_version_opens_this_releases_notes() {
    let mut model = model_with_plugins(&["flow"]);
    let (terminal, hits) = drawn_at(&model, 120, 40);
    let version = format!("v{}", crate::self_update::running());
    let (rect, _) = *hits
        .iter()
        .find(|(_, hit)| *hit == Hit::RunningReleaseNotes)
        .expect("the version answers a click");
    let drawn = buffer_rows(&terminal);
    assert!(
        drawn[usize::from(rect.y)].contains(&version),
        "the hit is the version itself: {:?}",
        drawn[usize::from(rect.y)]
    );

    model.hits = hits;
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 120, 40),
    );
    assert!(model.version_hovered, "it answers the pointer");
    assert_eq!(
        model.click(rect.x, rect.y),
        Intent::ReadReleaseNotes(crate::self_update::running().to_owned())
    );
    assert!(
        matches!(&model.overlay, Overlay::ReleaseNotes(modal) if modal.version == crate::self_update::running()),
        "{:?}",
        model.overlay
    );
}

/// How far a colour sits from the backdrop everything is drawn on. The
/// scrim's whole job is to make this number smaller for the screen a modal
/// interrupts, so it is the number the test asks about.
fn distance_from_the_backdrop(color: ratatui::style::Color) -> u32 {
    let ground = uze_theme::active().color(Token::SurfaceBackground);
    let ratatui::style::Color::Rgb(red, green, blue) = color else {
        panic!("every colour this TUI draws is resolved from a token: {color:?}");
    };
    u32::from(red.abs_diff(ground.0))
        + u32::from(green.abs_diff(ground.1))
        + u32::from(blue.abs_diff(ground.2))
}

/// A modal answers for the whole screen — nothing behind it responds until
/// it is dealt with — and until the scrim existed the only thing saying so
/// was the dialog's own border, which on a full screen is one hairline.
#[test]
fn a_modal_pushes_the_screen_it_interrupts_behind_it() {
    let quiet = drawn(&model_with_plugins(&["flow"]));
    let asked = drawn(&TuiModel {
        overlay: Overlay::Confirm {
            kind: Confirmation::RemovePlugin("flow".to_owned()),
            focus: Some(0),
        },
        ..model_with_plugins(&["flow"])
    });

    // A cell in the sidebar: far from any centred dialog, written in a
    // colour the theme answers for, and visibly so — an unselected
    // route's edge bar is drawn in the backdrop's own colour, and a cell
    // already on the backdrop has nowhere to recede to — so both halves of
    // the claim are about the same drawn thing rather than about whatever
    // happened to be there.
    let (column, row) = (0..40u16)
        .flat_map(|row| (0..24u16).map(move |column| (column, row)))
        .find(|position| {
            quiet[*position].symbol().trim() != ""
                && theme::token_of(quiet[*position].fg).is_some()
                && distance_from_the_backdrop(quiet[*position].fg) > 0
        })
        .expect("the sidebar drew something");

    let before = distance_from_the_backdrop(quiet[(column, row)].fg);
    let after = distance_from_the_backdrop(asked[(column, row)].fg);
    assert!(
        after < before,
        "the screen behind a question recedes: {before} -> {after}"
    );
    assert!(
        theme::token_of(asked[(column, row)].fg).is_none(),
        "and it recedes to a colour between two tokens rather than to another token"
    );

    // The question itself is untouched: it is drawn over the scrim, not
    // under it, which is the whole shape of the thing.
    let border = (0..40u16)
        .flat_map(|row| (24..100u16).map(move |column| (column, row)))
        .find(|position| theme::token_of(asked[*position].fg) == Some(Token::BorderDefault))
        .expect("the dialog drew its border at full contrast");
    assert!(
        asked[border].symbol().trim() != "",
        "and drew a border glyph there, not an empty cell"
    );
}

/// A dialog answered only by a key would be the one place in the product
/// where the keyboard is the way in rather than the accelerator.
#[test]
fn a_question_is_answered_with_the_pointer_too() {
    let mut model = model_with_plugins(&["one"]);
    model.focus = Focus::Content;
    model.overlay = Overlay::Confirm {
        kind: Confirmation::RemovePlugin("one".to_owned()),
        focus: Some(1),
    };
    let (_terminal, hits) = drawn_at(&model, 100, 40);
    model.hits = hits;

    let button = |model: &TuiModel, yes: bool| {
        model
            .hits
            .iter()
            .find(|(_, hit)| *hit == crate::ui::hit::Hit::Answer(yes))
            .map(|(rect, _)| *rect)
    };
    let cancel = button(&model, false).expect("a way out you can click");
    let confirm = button(&model, true).expect("and a way through");

    // Anywhere else declines, which is what keeps a stray click from
    // agreeing to a deletion.
    assert_eq!(model.click(0, 0), Intent::None);
    assert_eq!(model.overlay, Overlay::None);

    model.overlay = Overlay::Confirm {
        kind: Confirmation::RemovePlugin("one".to_owned()),
        focus: Some(1),
    };
    assert_eq!(model.click(cancel.x, cancel.y), Intent::None);
    assert_eq!(model.overlay, Overlay::None);

    model.overlay = Overlay::Confirm {
        kind: Confirmation::RemovePlugin("one".to_owned()),
        focus: Some(1),
    };
    assert_eq!(
        model.click(confirm.x, confirm.y),
        Intent::Remove("one".to_owned())
    );

    // A click is its own answer, wherever the keyboard happened to be.
    model.overlay = Overlay::Confirm {
        kind: Confirmation::RemovePlugin("one".to_owned()),
        focus: Some(crate::ui::widget::dialog::CANCEL),
    };
    assert_eq!(
        model.click(confirm.x, confirm.y),
        Intent::Remove("one".to_owned())
    );
}

/// A confirmation reads top to bottom as one thing: what is asked (once),
/// of what, what it means — wrapped, never cut — and the answers on the
/// right, the affirmative last, with the keys for them in the border.
#[test]
fn a_confirmation_dialog_reads_as_heading_subject_body_and_answers() {
    let mut model = model_with_data();
    model.overlay = Overlay::Confirm {
        kind: Confirmation::DeleteProfile("default".to_owned()),
        focus: Some(1),
    };
    let (terminal, hits) = drawn_at(&model, 90, 24);
    let rows = buffer_rows(&terminal);
    let position = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle} is drawn: {rows:#?}"))
    };
    assert_eq!(
        rows.iter()
            .filter(|row| row.contains("Delete profile"))
            .count(),
        1,
        "the question is asked once, not in the border and again below it"
    );
    assert_eq!(position("default"), position("Delete profile") + 1);
    assert!(
        position("configuration is touched.") > position("default"),
        "the explanation wraps inside the dialog rather than off its edge"
    );
    let answers = position("  Delete  ");
    assert!(answers > position("configuration is touched."));
    let row = &rows[answers];
    assert!(
        row.find("Cancel").unwrap() < row.find("Delete").unwrap(),
        "the affirmative is last, where reading ends: {row}"
    );
    assert!(
        position("esc cancel") > answers,
        "how to answer from the keyboard sits in the bottom border"
    );
    for yes in [true, false] {
        assert!(
            hits.iter().any(|(_, hit)| *hit == Hit::Answer(yes)),
            "each answer is a target: {yes}"
        );
    }
}

/// The index is nothing but a long list, and the wheel was guarded on
/// "no overlay" — so the one surface that most needed it was the one
/// surface it did not reach.
#[test]
fn the_wheel_walks_the_open_index() {
    let _turn = KEYBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut model = TuiModel::default();
    model.act(uze_keys::Action::OpenActionIndex);
    let rows = match &model.overlay {
        Overlay::ActionIndex { scopes, filter, .. } => {
            model.action_index_rows(scopes, filter).len()
        }
        _ => panic!("the index did not open"),
    };
    assert!(rows > 3, "the index has a list to walk");

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
    let selected = |model: &TuiModel| match &model.overlay {
        Overlay::ActionIndex { selected, .. } => *selected,
        _ => panic!("the index closed"),
    };

    wheel(&mut model, MouseEventKind::ScrollDown);
    wheel(&mut model, MouseEventKind::ScrollDown);
    assert_eq!(selected(&model), 2, "down walks forward");

    wheel(&mut model, MouseEventKind::ScrollUp);
    assert_eq!(selected(&model), 1, "up walks back");

    // The wheel is not a click: the index is still open.
    assert!(matches!(model.overlay, Overlay::ActionIndex { .. }));
}

/// A dialog asking for text is answered like every other: its own buttons
/// answer it, and a click on the field it is typed into is not an answer
/// — closing over what was typed there would lose it.
#[test]
fn a_text_prompt_is_answered_by_its_buttons_and_a_click_inside_keeps_it() {
    let mut model = two_market_model();
    model.overlay = Overlay::AddMarketplace("https://example.com/team".to_owned());
    let (_terminal, hits) = drawn_at(&model, 110, 26);
    model.hits = hits;
    let rect_of = |model: &TuiModel, wanted: Hit| {
        model
            .hits
            .iter()
            .find(|(_, hit)| *hit == wanted)
            .map(|(rect, _)| *rect)
            .unwrap_or_else(|| panic!("{wanted:?} registered"))
    };

    let body = rect_of(&model, Hit::OverlayBody);
    assert_eq!(model.click(body.x + 4, body.y + 1), Intent::None);
    assert!(
        matches!(&model.overlay, Overlay::AddMarketplace(input) if input == "https://example.com/team"),
        "a click inside keeps the dialog and what was typed"
    );

    let add = rect_of(&model, Hit::OfferedAction(uze_keys::Action::Activate));
    assert_eq!(
        model.click(add.x, add.y),
        Intent::AddMarketplace("https://example.com/team".to_owned())
    );
    assert_eq!(model.overlay, Overlay::None);
}

/// A key a footer hint names is pressed by clicking anywhere on the hint,
/// its name included: the click and the keystroke land in the same place,
/// through the same door.
#[test]
fn clicking_a_footer_hint_is_pressing_its_key() {
    use uze_keys::Action;
    let fresh = || TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        overlay: Overlay::None,
        ..model_with_data()
    };
    let draw = |model: &mut TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(150, 26)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        model.hits = hits;
    };

    let mut clicked = fresh();
    draw(&mut clicked);
    let hint = clicked
        .hits
        .iter()
        .filter(|(_, hit)| *hit == Hit::OfferedAction(Action::StartFilter))
        .map(|(rect, _)| *rect)
        .max_by_key(|rect| rect.y)
        .expect("the footer offers the key that starts a search");
    // On the last letter of its name, not on its key.
    clicked.click(hint.right() - 1, hint.y);

    let mut pressed = fresh();
    pressed.apply_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));

    assert!(clicked.filtering, "the click started the search");
    assert_eq!(clicked.filtering, pressed.filtering);
}

/// Beneath an open dialog the footer's keys are no targets: the dialog
/// takes any offered action under the pointer as its answer, and a key
/// on the screen behind it must never answer for it.
#[test]
fn a_hint_beneath_a_dialog_is_no_target() {
    let model = TuiModel {
        route: Route::Plugins,
        overlay: Overlay::HarnessHelp,
        ..model_with_data()
    };
    let mut terminal = Terminal::new(TestBackend::new(150, 26)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::StartFilter)),
        "no hint key is registered while a dialog is open"
    );
}
