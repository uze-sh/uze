//! Tests for the management client, one file per screen or concern.

// Gated again so the architecture suite, which reads `#[cfg(test)] mod`
// declarations, knows these files are test code.
#[cfg(test)]
mod extensions;
#[cfg(test)]
mod harnesses;
#[cfg(test)]
mod layout;
#[cfg(test)]
mod marketplaces;
#[cfg(test)]
mod navigation;
#[cfg(test)]
mod overview;
#[cfg(test)]
mod plugins;
#[cfg(test)]
mod profiles;
#[cfg(test)]
mod search;
#[cfg(test)]
mod session;
#[cfg(test)]
mod support;

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use uze_application::application::{DoctorReport, MaintenanceReport};

use super::hit::Hit;
use super::management::render;
use super::model::{Confirmation, Focus, Overlay, ROUTES, Route, TuiModel};
use super::view::health::{Severity, actionable_alerts};
use super::worker::Intent;
use crate::ui::theme::{self, Token};
use crate::ui::widget::text;

use support::*;

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

#[test]
fn overview_alerts_classify_conflicts_as_high_and_missing_as_low() {
    use uze_application::application::{ManagedStateSummary, PackageManagedState};
    let doctor = DoctorReport {
        uze_home: PathBuf::from("/home"),
        store: uze_application::application::StoreHealth::Ready,
        plugins: Vec::new(),
        harnesses: Vec::new(),
        attachments: vec![
            PackageManagedState {
                hooks: Vec::new(),
                plugin: "acme".to_owned(),
                state: ManagedStateSummary {
                    matched: 0,
                    missing: 1,
                    drifted: 0,
                    conflicts: 1,
                    blocked: 0,
                    ledger_error: None,
                },
            },
            PackageManagedState {
                hooks: Vec::new(),
                plugin: "example".to_owned(),
                state: ManagedStateSummary {
                    matched: 0,
                    missing: 1,
                    drifted: 0,
                    conflicts: 0,
                    blocked: 0,
                    ledger_error: None,
                },
            },
        ],
        deliveries: Vec::new(),
        ledger_error: None,
        provisioning_state_error: None,
        leftovers: Default::default(),
        maintenance: MaintenanceReport::default(),
        git_found: true,
        shell_refusal: None,
        ssh_missing: false,
        machine_concerns: Vec::new(),
    };
    let alerts = actionable_alerts(Some(&doctor));
    assert_eq!(alerts[0].severity, Severity::High);
    assert!(alerts.iter().any(|alert| alert.severity == Severity::Low));
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
                super::management::render_modal(frame, frame.area(), model, false, &mut Vec::new());
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

/// A hint names a key and what it does, and it asks the keymap for both.
/// The strings this replaced were typed by hand — which is how the help
/// overlay came to omit nine of the keys it was supposed to document.
#[test]
fn a_hint_line_reads_its_keys_off_the_keymap() {
    use ratatui::text::Line;
    use uze_keys::{Action, Scope};

    let scopes = [Scope::Global, Scope::Management, Scope::Plugins];
    let line: Line<'static> = crate::ui::widget::hint::Hints::new(&scopes)
        .key(Action::UpdatePlugin)
        .key(Action::Refresh)
        .key(Action::OpenActionIndex)
        .line();
    let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let keymap = uze_keys::active();
    for action in [Action::UpdatePlugin, Action::Refresh] {
        let chord = keymap.chord_for(action, &scopes).expect("bound here");
        assert!(
            content.contains(&chord.to_string()),
            "the hint names {action} without its key: {content}"
        );
        assert!(
            content.contains(&action.label().to_lowercase()),
            "{content}"
        );
    }
    assert_eq!(line.spans[0].style, theme::fg_bold(Token::Accent));
    assert_eq!(line.spans[1].style, theme::fg(Token::TextMuted));

    // An action with no key here is skipped rather than printed keyless:
    // a hint is a list of shortcuts, and what has none is offered where a
    // pointer can reach it.
    let unbound: Line<'static> = crate::ui::widget::hint::Hints::new(&scopes)
        .key(Action::NewSpace)
        .line();
    assert!(unbound.spans.is_empty());
}

/// A name or caption in wide characters — a CJK branch, an emoji — takes
/// two cells a character. Counted as one, the row ran past its column by
/// the difference and the frame cut the pad or the caption off.
#[test]
fn a_row_of_wide_characters_fits_the_columns_it_was_given() {
    use ratatui::{style::Style, text::Span};

    use crate::ui::widget::row;

    let width = |spans: &[Span<'_>]| spans.iter().map(Span::width).sum::<usize>();

    let mut caption = vec![Span::raw("▾ Git")];
    row::push_trailing(
        &mut caption,
        20,
        "功能分支".to_owned(),
        theme::color(Token::TextMuted),
    );
    assert_eq!(width(&caption), 20);

    let title = row::title_row("提交说明", "esc", 30);
    assert_eq!(title.width(), 30);

    let name = "エージェントの作業ブランチ";
    for tick in 0..200 {
        let window = row::marquee(&[(name, Style::default())], 9, tick)
            .expect("a name wider than the room slides");
        assert_eq!(width(&window), 9, "at tick {tick}");
    }
}

/// A caption pinned to the right edge is elided to what is left of the
/// row. It used to be appended whole and cut by the frame, which is how a
/// long branch name on the Git section header lost both its ending and any
/// sign that it had one.
#[test]
fn a_trailing_caption_is_elided_to_the_room_the_row_has_left() {
    use ratatui::text::Span;

    let mut spans = vec![Span::raw("▾ Git")];
    crate::ui::widget::row::push_trailing(
        &mut spans,
        20,
        "agent/a-very-long-branch-name".to_owned(),
        theme::color(Token::TextMuted),
    );
    let row: String = spans.iter().map(|span| span.content.as_ref()).collect();
    assert!(
        row.ends_with("… "),
        "the caption says it was shortened: {row}"
    );
    assert!(
        Span::raw(&row).width() <= 20,
        "and the row still fits the column: {row}"
    );

    let mut spans = vec![Span::raw("▾ Git")];
    crate::ui::widget::row::push_trailing(
        &mut spans,
        20,
        "main".to_owned(),
        theme::color(Token::TextMuted),
    );
    let row: String = spans.iter().map(|span| span.content.as_ref()).collect();
    assert!(
        row.contains("main"),
        "a caption that fits is left alone: {row}"
    );
    assert!(!row.contains('…'), "{row}");
}

#[test]
fn clip_line_truncates_long_status_with_ellipsis() {
    use ratatui::text::Line;

    let mut line =
        Line::from("Installed plugin root: /home/user/.codex/plugins/cache/very/long/path");
    text::clip(&mut line, 20);
    let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(content, "Installed plugin ro…");
    assert_eq!(ratatui::text::Span::raw(&content).width(), 20);

    let mut line = Line::from("Installed uze");
    text::clip(&mut line, 20);
    let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(content, "Installed uze");
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

/// A row eliding text stays inside the width it was given, whatever the
/// active theme's elision marker costs.
///
/// The ASCII theme spends three cells on `...` where the default spends one
/// on `…`, and the old code reserved a hard-coded `1` — so this is the
/// property that says a theme can replace a symbol without shearing every
/// row that draws it.
#[test]
fn eliding_reserves_the_active_themes_own_marker_width() {
    let marker = theme::glyph(theme::Symbol::Ellipsis);
    let marker_width = usize::from(theme::width(theme::Symbol::Ellipsis));
    for width in (marker_width + 1)..12usize {
        let elided = text::elide("a subject line long enough to be cut", width);
        let cells = elided.chars().count() - marker.chars().count() + marker_width;
        assert!(
            cells <= width,
            "elided to {elided:?} ({cells} cells) for a width of {width}"
        );
        assert!(
            elided.ends_with(&marker),
            "an elided row has to say it was cut: {elided:?}"
        );
    }
}

// The sidebar draws the badge into the label column, beside a
// right-aligned count, and the workspace tab measures the alias against
// the width it has left — so a label that changed length or cell count on
// its way to the screen would take a column with it. Cell count holds
// because every small capital is East Asian width *neutral*: a terminal
// gives each one cell whether or not its font has the glyph.
#[test]
fn small_caps_preserves_a_labels_length_and_its_cells() {
    use ratatui::text::Span;
    for label in ["Beta", "claude", "codex", "antigravity", "Run uze setup"] {
        let drawn = crate::ui::widget::text::small_caps(label);
        assert_eq!(
            drawn.chars().count(),
            label.chars().count(),
            "{label:?} changed length as {drawn:?}"
        );
        assert_eq!(
            Span::raw(drawn.clone()).width(),
            Span::raw(label).width(),
            "{label:?} changed cell count as {drawn:?}"
        );
        assert_eq!(
            drawn.split(' ').count(),
            label.split(' ').count(),
            "{label:?} lost a word boundary as {drawn:?}"
        );
    }
}

// Mixed case has to arrive as one even run — a full-height initial next to
// small capitals is the thing this exists to avoid. `q` and `x`, which
// Unicode has no small capital for, come out lowercase rather than
// vanishing or standing up as the one full-height letter in the run.
#[test]
fn small_caps_levels_mixed_case_and_keeps_what_it_cannot_fold() {
    assert_eq!(crate::ui::widget::text::small_caps("Beta"), "ʙᴇᴛᴀ");
    assert_eq!(
        crate::ui::widget::text::small_caps("PATH shadowed"),
        "ᴘᴀᴛʜ ꜱʜᴀᴅᴏᴡᴇᴅ"
    );
    assert_eq!(crate::ui::widget::text::small_caps("Query X2"), "qᴜᴇʀʏ x2");
}

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

/// Every card of the catalogue is reachable however narrow the column
/// gets, and the last of them can be brought on screen.
///
/// The catalogue is a grid: narrow the column and the cards wrap onto
/// more lines than there are rows. It used to draw from the top and stop
/// at the foot, so everything past the last full line was invisible and
/// unreachable at once — the selection walked off the bottom and nothing
/// followed it.
#[test]
fn the_settings_catalog_follows_its_selection_down_a_narrow_column() {
    let mut model = model_with_data();
    model.set_route(Route::Settings);
    model.focus = Focus::Content;
    model.settings_themes = (0..8)
        .map(|index| uze_application::application::ThemeSummary {
            id: format!("theme-{index}"),
            active: index == 0,
            path: None,
        })
        .collect();
    model.settings_glyph_sets = (0..4)
        .map(|index| uze_application::application::GlyphSetSummary {
            id: format!("set-{index}"),
            active: false,
        })
        .collect();
    model.settle_settings_selection();

    // One card per line, and more lines than rows.
    let drawn = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        let rows = buffer_rows(&terminal).join("\n");
        let cards: Vec<usize> = hits
            .iter()
            .filter_map(|(_, hit)| match hit {
                Hit::SettingsRow(index) => Some(*index),
                _ => None,
            })
            .collect();
        (cards, rows)
    };

    let last = model.settings_rows().len() - 1;
    let (first_page, rows) = drawn(&model);
    assert!(!first_page.is_empty(), "the catalogue is drawn: {rows}");
    assert!(
        !first_page.contains(&last),
        "the last card is past the first screenful: {rows}"
    );

    model.settings_selected = last;
    let (followed, rows) = drawn(&model);
    assert!(
        followed.contains(&last),
        "and the window follows the selection to it: {rows}"
    );
    assert!(
        !followed.contains(&first_page[0]),
        "leaving the first behind rather than drawing both: {rows}"
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

/// The claim the Settings screen exists to make: each glyph set is drawn
/// in *its own* glyphs, not in the ones currently in force. Without that,
/// choosing a set is choosing a name and hoping — which is the guess the
/// whole change removes, since no terminal can be asked what font it has.
#[test]
fn each_glyph_set_is_previewed_in_its_own_glyphs() {
    let mut terminal = Terminal::new(TestBackend::new(92, 40)).unwrap();
    let model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_glyph_sets: uze_theme::glyph_sets()
            .iter()
            .map(|id| uze_application::application::GlyphSetSummary {
                id: (*id).to_owned(),
                active: false,
            })
            .collect(),
        ..TuiModel::default()
    };
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    let rows = buffer_rows(&terminal);

    // A card names the set in its frame and draws the preview two rows
    // below: the frame, the line saying what the set asks of the machine,
    // then the marks themselves.
    let row_for = |id: &str| -> String {
        let title = rows
            .iter()
            .position(|row| row.contains('\u{256d}') && row.contains(id))
            .unwrap_or_else(|| panic!("no card for the `{id}` set in {rows:#?}"));
        rows[title + 2].clone()
    };

    // ASCII's own marks, on the ASCII row, while the default theme — the
    // one actually in force here — draws none of them.
    let ascii = row_for("ascii");
    assert!(
        ascii.contains('*'),
        "the ascii set is not in ascii: {ascii:?}"
    );
    assert!(
        !ascii.contains('✓'),
        "the ascii row borrowed the active theme's glyphs: {ascii:?}"
    );

    // The default set draws its own, on its own row.
    let default = row_for("default");
    assert!(
        default.contains('✓'),
        "the default set is not in its own glyphs: {default:?}"
    );

    // And the nerd set draws Codicons, which live in the private-use area.
    let nerd = row_for("nerd");
    assert!(
        nerd.chars().any(|c| ('\u{ea60}'..='\u{ec84}').contains(&c)),
        "the nerd row carries no Codicon: {nerd:?}"
    );
}

/// The card the keyboard is on says so on its frame; the card in force says
/// so with its fill. Painting the fill over the frame rather than inside it
/// takes the frame away from every card that is in force but not selected —
/// which is every card in force, most of the time.
#[test]
fn the_fill_that_marks_what_is_in_force_leaves_the_frame_alone() {
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_glyph_sets: uze_theme::glyph_sets()
            .iter()
            .map(|id| uze_application::application::GlyphSetSummary {
                id: (*id).to_owned(),
                active: *id == "nerd",
            })
            .collect(),
        ..TuiModel::default()
    };
    model.settle_settings_selection();
    let in_force = model
        .settings_rows()
        .iter()
        .position(|row| {
            matches!(
                row,
                crate::ui::model::SettingsRow::GlyphSet { active: true, .. }
            )
        })
        .expect("a set in force to draw");
    assert_ne!(
        in_force, model.settings_selected,
        "the case this guards is the card in force that the keyboard is not on"
    );

    let (terminal, hits) = drawn_at(&model, 190, 30);

    let card = hits
        .iter()
        .find_map(|(rect, hit)| (*hit == Hit::SettingsRow(in_force)).then_some(*rect))
        .expect("the set in force drew no card");
    let buffer = terminal.backend().buffer();
    let fill = crate::ui::theme::color(uze_theme::Token::SurfaceSelected);
    assert_eq!(
        buffer[(card.x + 1, card.y + 1)].bg,
        fill,
        "the card in force is not filled"
    );
    assert_ne!(
        buffer[(card.x, card.y + 1)].bg,
        fill,
        "the fill reached the frame, which is where the selection has to show"
    );
}

#[test]
fn choosing_a_glyph_set_is_a_different_intent_from_choosing_a_theme() {
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![uze_application::application::ThemeSummary {
            id: "dracula".to_owned(),
            active: false,
            path: None,
        }],
        settings_glyph_sets: vec![uze_application::application::GlyphSetSummary {
            id: "nerd".to_owned(),
            active: false,
        }],
        ..TuiModel::default()
    };
    model.settle_settings_selection();

    // The list opens on the first *choice*, never on the heading above it.
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectTheme("dracula".to_owned())
    );

    // Walking down crosses the second heading without stopping on it.
    model.move_settings_selection(1);
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectGlyphSet("nerd".to_owned()),
        "the glyph set was reached as if it were a theme"
    );
}

#[test]
fn the_chime_is_chosen_on_the_settings_screen_and_marks_the_one_in_force() {
    use uze_application::Chime;
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![uze_application::application::ThemeSummary {
            id: "dracula".to_owned(),
            active: true,
            path: None,
        }],
        settings_chime: Chime::OutOfSight,
        ..TuiModel::default()
    };
    model.settle_settings_selection();

    let chimes: Vec<(Chime, bool)> = model
        .settings_rows()
        .into_iter()
        .filter_map(|row| match row {
            crate::ui::model::SettingsRow::Chime { chime, active } => Some((chime, active)),
            _ => None,
        })
        .collect();
    assert_eq!(
        chimes,
        vec![
            (Chime::Silent, false),
            (Chime::OutOfSight, true),
            (Chime::Always, false),
        ]
    );

    model.move_settings_selection(3);
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectChime(Chime::Always)
    );
}

/// The adaptive card is no theme of its own, so it says what it follows
/// and, in the drawer, which theme it draws in on each background.
#[test]
fn the_adaptive_card_says_it_follows_the_system_and_names_both_themes() {
    let mut model = model_with_data();
    model.set_route(Route::Settings);
    model.focus = Focus::Content;
    model.settings_themes = vec![uze_application::application::ThemeSummary {
        id: uze_application::ADAPTIVE.to_owned(),
        active: true,
        path: None,
    }];
    model.settings_adaptive = Some(("light".to_owned(), "dracula".to_owned()));
    model.settle_settings_selection();

    let mut terminal = Terminal::new(TestBackend::new(160, 30)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    let rows = buffer_rows(&terminal).join("\n");
    assert!(rows.contains("follows the system"), "{rows}");
    assert!(rows.contains("light"), "{rows}");
    assert!(rows.contains("dracula"), "{rows}");
    assert!(!rows.contains("built in"), "{rows}");
}

/// The list opens with a heading, so moving up from the first choice has
/// nowhere to go: it stays put. It used to step past the heading forever,
/// clamping back onto it at every step, and freeze the whole client.
#[test]
fn moving_up_from_the_first_settings_choice_stays_put() {
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![uze_application::application::ThemeSummary {
            id: "dracula".to_owned(),
            active: false,
            path: None,
        }],
        ..TuiModel::default()
    };
    model.settle_settings_selection();
    let (done, finished) = std::sync::mpsc::channel();
    let mover = std::thread::spawn(move || {
        model.move_settings_selection(-1);
        let _ = done.send(model.activate_settings());
    });
    let answered = finished.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(
        answered.ok(),
        Some(crate::ui::worker::Intent::SelectTheme("dracula".to_owned())),
        "moving up past the leading heading never returned"
    );
    let _ = mover.join();
}

/// Coming back to Settings builds the model afresh, so the selection has
/// to be settled again — on the theme in force, not on the first card.
#[test]
fn returning_to_settings_lands_on_the_theme_in_force() {
    let theme = |id: &str, active: bool| uze_application::application::ThemeSummary {
        id: id.to_owned(),
        active,
        path: None,
    };
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![
            theme("default", false),
            theme("dracula", false),
            theme("tokyo-night", true),
        ],
        ..TuiModel::default()
    };
    model.settle_settings_selection();
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectTheme("tokyo-night".to_owned()),
        "the selection fell back to the first card"
    );
}

#[test]
fn opening_settings_asks_for_the_lists_it_chooses_from() {
    let mut model = TuiModel::default();
    assert_eq!(
        model.set_route(Route::Settings),
        crate::ui::worker::Intent::LoadSettings
    );
    // Every other route asks for nothing on arrival, so this one is not
    // paying for a read it does not need.
    assert_eq!(
        model.set_route(Route::Keys),
        crate::ui::worker::Intent::None
    );
}

/// Arriving is the only moment Settings asks for its lists, so every way
/// of arriving has to carry the ask. A gesture that dropped it left the
/// screen showing its two headings and nothing under them — and leaving and
/// coming back was no cure, because coming back was the gesture that dropped
/// it.
#[test]
fn every_way_of_reaching_settings_carries_the_ask() {
    let landing = Route::Settings.index();

    let mut numbered = TuiModel::default();
    assert_eq!(
        numbered.act(uze_keys::Action::SelectTab(landing as u8 + 1)),
        crate::ui::worker::Intent::LoadSettings,
        "choosing Settings by its number reached it without asking for its lists"
    );

    let mut walked = TuiModel {
        focus: Focus::Sidebar,
        ..TuiModel::default()
    };
    let mut asked = None;
    for _ in 0..ROUTES.len() {
        let intent = walked.act(uze_keys::Action::SelectNext);
        if walked.route.index() == landing {
            asked = Some(intent);
            break;
        }
    }
    assert_eq!(
        asked,
        Some(crate::ui::worker::Intent::LoadSettings),
        "walking the screen tabs reached Settings without asking for its lists"
    );

    let mut clicked = TuiModel::default();
    let (_terminal, hits) = drawn_at(&clicked, 120, 40);
    clicked.hits = hits;
    let (rect, _) = clicked
        .hits
        .iter()
        .find(|(_, hit)| *hit == crate::ui::hit::Hit::Route(Route::Settings))
        .expect("Settings is reachable from its tab")
        .clone();
    assert_eq!(
        clicked.click(rect.x + 1, rect.y),
        crate::ui::worker::Intent::LoadSettings,
        "clicking into Settings reached it without asking for its lists"
    );
}

#[test]
fn clicking_a_glyph_set_chooses_it() {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_glyph_sets: vec![uze_application::application::GlyphSetSummary {
            id: "ascii".to_owned(),
            active: false,
        }],
        ..TuiModel::default()
    };
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    model.hits = hits;

    let rows = model.settings_rows();
    let index = rows
        .iter()
        .position(|row| matches!(row, crate::ui::model::SettingsRow::GlyphSet { .. }))
        .expect("a set row");
    let (rect, _) = model
        .hits
        .iter()
        .find(|(_, hit)| *hit == crate::ui::hit::Hit::SettingsRow(index))
        .expect("the set row is clickable")
        .clone();

    assert_eq!(
        model.click(rect.x + 3, rect.y),
        crate::ui::worker::Intent::SelectGlyphSet("ascii".to_owned())
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

/// A card grid longer than its area scrolls to the selected card instead
/// of drawing what fits and leaving the selection off the bottom.
#[test]
fn a_card_grid_brings_the_selected_card_into_view() {
    let scroll = std::cell::Cell::new(Default::default());
    let area = Rect::new(0, 0, 80, 16);
    let grid = crate::ui::view::catalog::cards(area, 10, 9, &scroll);
    assert!(
        grid.cards.iter().any(|(position, _)| *position == 9),
        "the selected card is not drawn: {:?}",
        grid.cards
    );
    assert!(grid.first > 0 && grid.rows == 5);
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
