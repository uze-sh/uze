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
        drawers: Vec::new(),
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

fn standing(condition: uze_application::ExtensionCondition) -> uze_application::ExtensionReport {
    uze_application::ExtensionReport {
        id: "gsd-band".to_owned(),
        plugin: "gsd".to_owned(),
        marketplace: "market".to_owned(),
        name: "GSD".to_owned(),
        grants: vec!["read:checkout".to_owned()],
        condition,
        new_grants: Vec::new(),
        enabled_in: Vec::new(),
        updated_at: None,
        checkout_refused: None,
        offer_declined: false,
        errors: Vec::new(),
    }
}

const PROJECT: &str = "/work/acme";

/// A declared extension that runs a script, standing as `standing` says in
/// the project the screen was opened in.
fn standing_in_a_project(standing: uze_application::ExtensionReport) -> TuiModel {
    let mut model = listing_a_declared_extension(&[]);
    model.declared_extensions[0].linked = false;
    model.extension_standings = vec![standing];
    model.extension_project = Some(std::path::PathBuf::from(PROJECT));
    model
}

fn screen_of(model: &TuiModel) -> String {
    let (terminal, _) = super::support::drawn_at(model, 220, 44);
    super::support::buffer_rows(&terminal).join("\n")
}

fn here(consent: bool, enable: bool) -> Intent {
    Intent::ExtensionHere(crate::ui::model::ExtensionHere {
        identity: declared(&[]).identity,
        name: "GSD".to_owned(),
        project: std::path::PathBuf::from(PROJECT),
        consent,
        enable,
        switch_on: false,
    })
}

#[test]
fn an_extension_that_needs_consent_says_so_and_shows_its_grants() {
    let model = standing_in_a_project(standing(uze_application::ExtensionCondition::NeedsConsent));
    let screen = screen_of(&model);
    assert!(
        screen
            .lines()
            .any(|row| row.contains("gsd-band · gsd@market") && row.contains("Needs consent")),
        "{screen}"
    );
    assert!(screen.contains("grants"), "{screen}");
    assert!(
        screen.contains("read:checkout · reads the files of the checkout"),
        "{screen}"
    );
    assert!(
        screen.contains("enter enable"),
        "Enter's hint is what it does: {screen}"
    );
}

/// Enable shows the grants in a dialog before anything is recorded, and
/// only its answer consents and enables.
#[test]
fn enabling_an_unconsented_extension_asks_with_its_grants_first() {
    let mut model =
        standing_in_a_project(standing(uze_application::ExtensionCondition::NeedsConsent));
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(model.apply_key(enter), Intent::None, "nothing recorded yet");
    let screen = screen_of(&model);
    assert!(screen.contains("Allow this extension to run?"), "{screen}");
    assert!(screen.contains("gsd-band · gsd@market"), "{screen}");
    assert!(screen.contains("in acme only"), "{screen}");
    assert!(
        screen.contains("read:checkout · reads the files of the checkout"),
        "{screen}"
    );
    assert!(screen.contains("Allow and enable"), "{screen}");
    assert_eq!(model.apply_key(enter), here(true, true));
}

/// The dialog's Allow button is its Enter.
#[test]
fn clicking_allow_in_the_grants_dialog_is_pressing_enter() {
    let mut model =
        standing_in_a_project(standing(uze_application::ExtensionCondition::NeedsConsent));
    model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let (_, hits) = super::support::drawn_at(&model, 140, 40);
    let (rect, _) = *hits
        .iter()
        .find(|(_, hit)| *hit == Hit::Answer(true))
        .expect("the dialog's Allow is drawn");
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
    assert_eq!(intent, here(true, true));
}

#[test]
fn a_held_extension_names_what_its_update_wants_and_asks_for_it() {
    let mut held = standing(uze_application::ExtensionCondition::Held);
    held.new_grants = vec!["clipboard".to_owned()];
    let mut model = standing_in_a_project(held);
    let screen = screen_of(&model);
    assert!(
        screen
            .lines()
            .any(|row| row.contains("gsd-band · gsd@market") && row.contains("Held")),
        "{screen}"
    );
    assert!(
        screen.contains("its update wants clipboard, which you have not"),
        "{screen}"
    );
    model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let screen = screen_of(&model);
    assert!(screen.contains("Allow what its update wants?"), "{screen}");
    assert!(
        screen.contains("clipboard · copies text you choose to the"),
        "{screen}"
    );
}

/// Consented on this machine, an extension is enabled here without a
/// second question, and disabled here once it is.
#[test]
fn a_consented_extension_is_enabled_and_disabled_in_this_project_alone() {
    let mut model =
        standing_in_a_project(standing(uze_application::ExtensionCondition::NeedsEnabling));
    assert!(screen_of(&model).contains("Not enabled here"));
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(model.apply_key(enter), here(false, true));

    let mut enabled = standing(uze_application::ExtensionCondition::Enabled);
    enabled.enabled_in = vec![
        std::path::PathBuf::from(PROJECT),
        std::path::PathBuf::from("/work/other"),
    ];
    enabled.updated_at = Some(1_792_972_800);
    let mut model = standing_in_a_project(enabled);
    let screen = screen_of(&model);
    assert!(screen.contains("Enabled here"), "{screen}");
    assert!(screen.contains("enabled in 1 other project"), "{screen}");
    assert!(
        screen.contains("updated 2026-10-26, same grants"),
        "{screen}"
    );
    assert_eq!(model.apply_key(enter), here(false, false));
}

/// A home directory is no checkout: the extension says why it runs
/// nothing, and Enable is not offered.
#[test]
fn a_refused_checkout_is_the_reason_the_extension_runs_nothing() {
    let mut refused = standing(uze_application::ExtensionCondition::NeedsEnabling);
    refused.checkout_refused = Some("your home directory is not a checkout".to_owned());
    let mut model = standing_in_a_project(refused);
    let screen = screen_of(&model);
    assert!(
        screen
            .lines()
            .any(|row| row.contains("gsd-band · gsd@market") && row.contains("Runs nothing here")),
        "{screen}"
    );
    assert!(
        screen.contains("runs nothing here: your home directory is not a"),
        "{screen}"
    );
    assert!(!screen.contains("enter enable"), "{screen}");
    assert_eq!(
        model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Intent::None
    );
}

/// Seen with no project in front (a shell alone in the space), an
/// extension that runs code is enabled nowhere, and its machine switch is
/// already on: Enable must say where it is enabled instead of answering
/// the click with nothing.
#[test]
fn enabling_with_no_project_in_front_says_where_it_is_enabled() {
    let mut model =
        standing_in_a_project(standing(uze_application::ExtensionCondition::NeedsConsent));
    model.extension_project = None;
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(model.apply_key(enter), Intent::None);
    assert!(
        !matches!(model.overlay, crate::ui::model::Overlay::Confirm { .. }),
        "no dialog for a project there is none of"
    );
    let screen = screen_of(&model);
    assert!(screen.contains("open an agent in one"), "{screen}");
}
