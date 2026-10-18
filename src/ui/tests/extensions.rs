//! The Extensions screen: selecting and switching an extension.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::ui::hit::Hit;
use crate::ui::model::{Focus, Route, TuiModel};
use crate::ui::worker::Intent;

#[test]
fn mouse_click_on_extension_row_selects_and_opens_drawer_without_fetch() {
    // Clicking an extension row behaves like arrow-key navigation —
    // selection opens the drawer, but never an async fetch (there is
    // nothing to fetch: the catalog is static, and no "Inspecting…"
    // status flash belongs on every click).
    let mut model = TuiModel {
        focus: Focus::Content,
        ..TuiModel::default()
    };
    model.hits = vec![
        (Rect::new(0, 0, 20, 1), Hit::ExtensionRow(0)),
        (Rect::new(0, 1, 20, 1), Hit::ExtensionRow(1)),
    ];
    let intent = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 100, 40),
    );
    assert_eq!(model.remembered.extension_screen.selected, 1);
    assert_eq!(intent, Intent::None);
}

/// Enter performs a row's first offer, here as on the Marketplace: it
/// switches an extension whichever way it is not, so the screen spends no
/// pair of letters on one toggle.
#[test]
fn enter_switches_an_extension_whichever_way_it_is_not() {
    use crate::ui::worker::Intent;

    let mut model = TuiModel {
        route: Route::Extensions,
        focus: Focus::Content,
        extensions: uze_extensions::registry::ExtensionRegistry::builtin()
            .all()
            .to_vec(),
        ..TuiModel::default()
    };
    let selected = model.selected_extension().expect("a selection").to_owned();
    let switched = |enabled| Intent::SwitchExtension {
        id: selected.id.to_owned(),
        name: selected.name.to_owned(),
        enabled,
    };
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(model.apply_key(enter), switched(false));
    model.disabled_extensions.insert(selected.id.to_owned());
    assert_eq!(model.apply_key(enter), switched(true));
    assert!(
        model
            .selected_offers()
            .iter()
            .any(|offer| offer.action == uze_keys::Action::EnableExtension && offer.is_available())
    );
}

/// Walking the list — by arrow or by wheel — only moves the selection: a
/// switch is Enter's, and passing over an extension never changes it.
#[test]
fn walking_the_extensions_never_switches_one() {
    use crate::ui::worker::Intent;

    let mut model = TuiModel {
        route: Route::Extensions,
        focus: Focus::Content,
        extensions: uze_extensions::registry::ExtensionRegistry::builtin()
            .all()
            .to_vec(),
        ..TuiModel::default()
    };
    assert_eq!(
        model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        Intent::None
    );
    let walked_to = model.selected_extension().expect("a selection").id;
    let wheel = |model: &mut TuiModel, kind| {
        model.apply_mouse(
            MouseEvent {
                kind,
                column: 60,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 100, 40),
        )
    };
    assert_eq!(wheel(&mut model, MouseEventKind::ScrollUp), Intent::None);
    assert_eq!(
        model.selected_extension().expect("a selection").id,
        walked_to,
        "the wheel moved the selection"
    );
    assert!(
        model.disabled_extensions.is_empty(),
        "walking switched an extension"
    );
}
