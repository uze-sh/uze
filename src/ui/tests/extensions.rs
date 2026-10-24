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
    let selected = model.selected_extension().expect("a selection");
    let (id, name) = (selected.id(), selected.name().to_owned());
    let switched = |enabled| Intent::SwitchExtension {
        id: id.clone(),
        name: name.clone(),
        enabled,
    };
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(model.apply_key(enter), switched(false));
    model.disabled_extensions.insert(id.clone());
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
    let walked_to = model.selected_extension().expect("a selection").id();
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
        model.selected_extension().expect("a selection").id(),
        walked_to,
        "the wheel moved the selection"
    );
    assert!(
        model.disabled_extensions.is_empty(),
        "walking switched an extension"
    );
}

fn declared(errors: &[&str]) -> uze_application::DeclaredExtensionSummary {
    uze_application::DeclaredExtensionSummary {
        identity: uze_application::ExtensionIdentity {
            marketplace: "market".to_owned(),
            plugin: "gsd".to_owned(),
            id: "gsd-band".to_owned(),
        },
        linked: true,
        name: "GSD".to_owned(),
        description: "Phases and plans".to_owned(),
        views: vec!["Board".to_owned()],
        sections: vec!["Band".to_owned()],
        errors: errors.iter().map(|error| (*error).to_owned()).collect(),
    }
}

/// The built-ins and, after them, one extension a plugin declares, with
/// that one selected.
fn listing_a_declared_extension(errors: &[&str]) -> TuiModel {
    let mut model = TuiModel {
        route: Route::Extensions,
        focus: Focus::Content,
        extensions: uze_extensions::registry::ExtensionRegistry::builtin()
            .all()
            .to_vec(),
        declared_extensions: vec![declared(errors)],
        ..TuiModel::default()
    };
    model.remembered.extension_screen.selected = model.extensions.len();
    model
}

#[test]
fn a_declared_extension_is_listed_after_the_built_ins_with_whose_it_is() {
    let model = listing_a_declared_extension(&[]);
    let entries = model.extension_entries();
    assert!(matches!(
        entries.last(),
        Some(crate::ui::model::ExtensionEntry::Declared(_))
    ));
    let (terminal, _) = super::support::drawn_at(&model, 140, 40);
    let rows = super::support::buffer_rows(&terminal);
    let screen = rows.join("\n");
    assert!(screen.contains("1 from plugins"), "{screen}");
    assert!(
        screen.contains("gsd-band · gsd@market · linked"),
        "{screen}"
    );
    assert!(screen.contains("Board · view"), "the drawer: {screen}");
    assert!(screen.contains("Band · section"), "{screen}");
    assert!(!screen.contains("last compile"), "{screen}");
}

#[test]
fn a_declared_extension_that_failed_says_so_on_its_card_and_in_its_drawer() {
    let mut model = listing_a_declared_extension(&["board.kdl:3:5: unknown node `Stak`"]);
    let (terminal, _) = super::support::drawn_at(&model, 140, 40);
    let screen = super::support::buffer_rows(&terminal).join("\n");
    assert!(screen.contains("1 error"), "{screen}");
    assert!(screen.contains("last compile"), "{screen}");
    assert!(screen.contains("unknown node `Stak`"), "{screen}");

    // Switched off, its error no longer reaches the workspace, and the
    // card says only that it is off.
    model
        .disabled_extensions
        .insert(crate::ui::extension_switch::declared_id(
            &declared(&[]).identity,
        ));
    let (terminal, _) = super::support::drawn_at(&model, 140, 40);
    let rows = super::support::buffer_rows(&terminal);
    assert!(
        rows.iter()
            .any(|row| row.contains("gsd-band · gsd@market") && row.contains("Disabled")),
        "{rows:#?}"
    );
}

/// The switch is the built-ins' own, keyed as the workspace host keys it.
#[test]
fn enter_switches_a_declared_extension_by_its_identity() {
    let mut model = listing_a_declared_extension(&[]);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        model.apply_key(enter),
        Intent::SwitchExtension {
            id: "gsd-band · gsd@market".to_owned(),
            name: "GSD".to_owned(),
            enabled: false,
        }
    );
    model
        .disabled_extensions
        .insert("gsd-band · gsd@market".to_owned());
    assert_eq!(
        model.apply_key(enter),
        Intent::SwitchExtension {
            id: "gsd-band · gsd@market".to_owned(),
            name: "GSD".to_owned(),
            enabled: true,
        }
    );
}

#[test]
fn the_filter_finds_a_declared_extension_by_its_plugin() {
    let mut model = listing_a_declared_extension(&[]);
    model.remembered.extension_screen.selected = 0;
    model.remembered.extension_screen.filter = "gsd@market".to_owned();
    assert_eq!(
        model.extension_visible_indices(),
        vec![model.extensions.len()]
    );
    assert_eq!(
        model
            .selected_extension()
            .map(|entry| entry.name().to_owned()),
        Some("GSD".to_owned())
    );
}

/// The drawer's switch button is the key: a click on it asks what Enter
/// asks.
#[test]
fn clicking_the_drawers_switch_on_a_declared_extension_is_pressing_its_key() {
    let mut model = listing_a_declared_extension(&[]);
    let (_, hits) = super::support::drawn_at(&model, 140, 40);
    let (rect, _) = *hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::OfferedAction(_)))
        .expect("the drawer's switch is drawn");
    model.hits = hits;
    let intent = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 140, 40),
    );
    assert_eq!(
        intent,
        Intent::SwitchExtension {
            id: "gsd-band · gsd@market".to_owned(),
            name: "GSD".to_owned(),
            enabled: false,
        }
    );
}
