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
        official: false,
        name: "GSD".to_owned(),
        description: "Phases and plans".to_owned(),
        views: vec!["Board".to_owned()],
        sections: vec!["Band".to_owned()],
        drawers: Vec::new(),
        statuses: Vec::new(),
        bands: Vec::new(),
        overlays: Vec::new(),
        bars: Vec::new(),
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
    assert!(screen.contains("Band · sidebar section"), "{screen}");
    assert!(!screen.contains("last compile"), "{screen}");
}

/// Two seals, as an editor's marketplace keeps them apart: what ships
/// inside uze is `Bundled`, and what uze publishes in its marketplace is
/// `Official`; a third party's wears neither.
#[test]
fn bundled_and_official_extensions_wear_their_own_seals() {
    let mut model = listing_a_declared_extension(&[]);
    let screen = |model: &TuiModel| {
        super::support::buffer_rows(&super::support::drawn_at(model, 140, 40).0).join("\n")
    };
    let third_party = screen(&model);
    assert!(third_party.contains("Bundled"), "{third_party}");
    assert!(!third_party.contains("Official"), "{third_party}");
    model.declared_extensions[0].official = true;
    let official = screen(&model);
    assert!(
        official
            .lines()
            .any(|row| row.contains("GSD") && row.contains("Official")),
        "{official}"
    );
}

#[test]
fn a_declared_extension_that_failed_says_so_on_its_card_and_in_its_drawer() {
    let mut model = listing_a_declared_extension(&[
        "board.uze:3:5: unknown node `Stak`",
        "extension.kdl:11:7: there is no grant `mcp:rtc/rtc_timesheet` (this version offers \
         `read:checkout`, `clipboard`, `open-path`, `store` and the families that end here)",
    ]);
    let (terminal, _) = super::support::drawn_at(&model, 140, 40);
    let screen = super::support::buffer_rows(&terminal).join("\n");
    assert!(screen.contains("2 errors"), "{screen}");
    assert!(screen.contains("the last compile has 2 errors"), "{screen}");
    assert!(screen.contains("unknown node `Stak`"), "{screen}");
    // A long error is folded in the drawer, never cut: its end says how
    // to fix it.
    assert!(screen.contains("that end here)"), "{screen}");

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
            id: "gsd@market/gsd-band".to_owned(),
            name: "GSD".to_owned(),
            enabled: false,
        }
    );
    model
        .disabled_extensions
        .insert("gsd@market/gsd-band".to_owned());
    assert_eq!(
        model.apply_key(enter),
        Intent::SwitchExtension {
            id: "gsd@market/gsd-band".to_owned(),
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
        .find(|(_, hit)| {
            matches!(
                hit,
                Hit::OfferedAction(
                    uze_keys::Action::EnableExtension | uze_keys::Action::DisableExtension
                )
            )
        })
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
            id: "gsd@market/gsd-band".to_owned(),
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
        enabled_everywhere: false,
        reaches_agents: false,
        updated_at: None,
        checkout_refused: None,
        offer_declined: false,
        errors: Vec::new(),
        programs: Vec::new(),
        confined: None,
        preferences: Vec::new(),
        band_rows: None,
        digest: Some("sha256:shown".to_owned()),
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
        shown: Some("sha256:shown".to_owned()),
        reaches_agents: false,
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
        screen
            .lines()
            .any(|row| row.contains("read:checkout") && row.contains("reads the checkout's files")),
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
    assert!(screen.contains("Allow GSD to run?"), "{screen}");
    assert!(screen.contains("gsd@market"), "{screen}");
    assert!(
        screen.contains("Runs a script in every project"),
        "this screen answers for the machine: {screen}"
    );
    assert!(
        screen
            .lines()
            .any(|row| row.contains("read:checkout") && row.contains("reads the checkout's files")),
        "{screen}"
    );
    assert!(screen.contains("Allow and enable"), "{screen}");
    assert_eq!(model.apply_key(enter), here(true, true));
}

/// An extension that runs programs asks about them on a step of their own,
/// after its other grants, with each pinned path; nothing is recorded
/// until both are allowed.
#[test]
fn programs_are_asked_on_their_own_step_after_the_grants() {
    let mut standing = standing(uze_application::ExtensionCondition::NeedsConsent);
    standing.grants.push("exec:nvidia-smi".to_owned());
    standing.programs = vec![uze_application::ProgramPin {
        name: "nvidia-smi".to_owned(),
        path: Some(std::path::PathBuf::from("/usr/lib/wsl/lib/nvidia-smi")),
        unresolved: None,
        runs_anything: false,
        reads: vec!["/dev/dxg".to_owned()],
        packaged: false,
    }];
    standing.confined = Some(true);
    let mut model = standing_in_a_project(standing.clone());
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(model.apply_key(enter), Intent::None);
    let grants = screen_of(&model);
    assert!(grants.contains("Allow GSD to run?"), "{grants}");
    assert!(
        !grants.contains("Allow these programs"),
        "programs wait for their step: {grants}"
    );
    assert_eq!(model.apply_key(enter), Intent::None, "nothing recorded yet");
    let programs = screen_of(&model);
    assert!(
        programs.contains("Allow these programs to run, confined?"),
        "{programs}"
    );
    assert!(
        programs.contains("nvidia-smi · /usr/lib/wsl/lib/nvidia-smi"),
        "{programs}"
    );
    assert!(programs.contains("it also reads /dev/dxg"), "{programs}");
    assert_eq!(model.apply_key(enter), here(true, true));

    standing.confined = Some(false);
    standing.grants = vec!["exec:nvidia-smi".to_owned()];
    let mut model = standing_in_a_project(standing);
    assert_eq!(model.apply_key(enter), Intent::None);
    let unconfined = screen_of(&model);
    assert!(
        unconfined.contains("Allow these programs to run as you?"),
        "{unconfined}"
    );
    assert!(
        unconfined.contains("This machine cannot confine programs."),
        "{unconfined}"
    );
    assert!(
        unconfined.contains("runs as you, with everything you can"),
        "{unconfined}"
    );
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
    assert!(screen.contains("clipboard, not allowed yet"), "{screen}");
    model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let screen = screen_of(&model);
    assert!(
        screen.contains("Allow what GSD's update wants?"),
        "{screen}"
    );
    assert!(
        screen
            .lines()
            .any(|row| row.contains("clipboard") && row.contains("copies what you choose")),
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
    assert!(screen.contains("2026-10-26, same grants"), "{screen}");
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
        screen.contains("why         your home directory is not"),
        "{screen}"
    );
    assert!(!screen.contains("enter enable"), "{screen}");
    assert_eq!(
        model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Intent::None
    );
}

/// One that reaches agents is enabled in the project in front alone,
/// even from this screen, and the question says so.
#[test]
fn enabling_one_that_reaches_agents_asks_for_this_project_alone() {
    let mut reaching = standing(uze_application::ExtensionCondition::NeedsConsent);
    reaching.reaches_agents = true;
    let mut model = standing_in_a_project(reaching);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(model.apply_key(enter), Intent::None);
    let screen = screen_of(&model);
    assert!(screen.contains("in acme only,"), "{screen}");
    assert!(screen.contains("since it reaches its"), "{screen}");
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
    assert!(screen.contains("Open an agent in a project"), "{screen}");
}

/// A consented extension's drawer lists its programs where they were
/// pinned, says whether they run confined, and offers to find a missing
/// one again; its button asks what its index row does.
#[test]
fn the_drawer_lists_pinned_programs_and_resolves_them_again_on_a_click() {
    let mut standing = standing(uze_application::ExtensionCondition::NeedsEnabling);
    standing.grants.push("exec:nvidia-smi".to_owned());
    standing.programs = vec![uze_application::ProgramPin {
        name: "nvidia-smi".to_owned(),
        path: None,
        unresolved: Some("`nvidia-smi` not found".to_owned()),
        runs_anything: false,
        reads: vec!["/dev/dxg".to_owned()],
        packaged: false,
    }];
    standing.confined = Some(true);
    let mut model = standing_in_a_project(standing);
    let screen = screen_of(&model);
    assert!(screen.contains("programs"), "{screen}");
    assert!(
        screen
            .lines()
            .any(|row| row.contains("nvidia-smi") && row.contains("`nvidia-smi` not found")),
        "{screen}"
    );
    assert!(screen.contains("reads /dev/dxg"), "{screen}");
    assert!(screen.contains("confined: reads the checkout"), "{screen}");
    assert!(screen.contains("Resolve again"), "{screen}");
    let (_, hits) = super::support::drawn_at(&model, 220, 44);
    let resolve = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::ResolvePrograms))
        .map(|(rect, _)| *rect)
        .expect("Resolve again is a button");
    model.hits = hits;
    let clicked = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: resolve.x,
            row: resolve.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 220, 44),
    );
    let expected = Intent::ResolvePrograms {
        identity: declared(&[]).identity,
        name: "GSD".to_owned(),
    };
    assert_eq!(clicked, expected);
    assert_eq!(
        model.resolve_selected_programs(),
        expected,
        "the click is the action"
    );
}

fn with_preferences() -> TuiModel {
    let mut standing = standing(uze_application::ExtensionCondition::Enabled);
    standing.preferences = vec![
        uze_application::PreferenceRow {
            name: "fahrenheit".to_owned(),
            label: "Show °F".to_owned(),
            kind: "toggle",
            value: serde_json::json!(false),
            values: Vec::new(),
            default: serde_json::json!(false),
            min: None,
            max: None,
        },
        uze_application::PreferenceRow {
            name: "warn-at".to_owned(),
            label: "Warn at".to_owned(),
            kind: "number",
            value: serde_json::json!(70),
            values: Vec::new(),
            default: serde_json::json!(80),
            min: Some(40),
            max: Some(71),
        },
    ];
    standing_in_a_project(standing)
}

/// The focus example's two preferences, as its manifest declares them.
fn focus_preferences() -> TuiModel {
    let number = |name: &str, label: &str, value: i64, default: i64, max: i64| {
        uze_application::PreferenceRow {
            name: name.to_owned(),
            label: label.to_owned(),
            kind: "number",
            value: serde_json::json!(value),
            values: Vec::new(),
            default: serde_json::json!(default),
            min: Some(1),
            max: Some(max),
        }
    };
    let mut standing = standing(uze_application::ExtensionCondition::Enabled);
    standing.name = "Focus".to_owned();
    standing.preferences = vec![
        number("minutes", "Minutes in a session", 25, 25, 90),
        number("goal", "Sessions a day", 4, 4, 12),
    ];
    let mut model = standing_in_a_project(standing);
    model.declared_extensions[0].name = "Focus".to_owned();
    model.edit_selected_preferences();
    model
}

fn set(name: &str, label: &str, value: serde_json::Value) -> Intent {
    Intent::SetPreference {
        identity: declared(&[]).identity,
        name: name.to_owned(),
        label: label.to_owned(),
        value,
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The option drawn chosen, in the accent, on the row labelled `label`.
fn chosen_on(model: &TuiModel, label: &str) -> String {
    let (terminal, _) = super::support::drawn_at(model, 220, 44);
    let buffer = terminal.backend().buffer();
    let rows = super::support::buffer_rows(&terminal);
    let y = rows
        .iter()
        .position(|row| row.contains(label))
        .unwrap_or_else(|| panic!("{label} is drawn"));
    let accent = crate::ui::theme::color(uze_theme::Token::Accent);
    (0..buffer.area.width)
        .filter(|x| buffer[(*x, y as u16)].bg == accent)
        .map(|x| buffer[(x, y as u16)].symbol().to_owned())
        .collect::<String>()
        .trim()
        .to_owned()
}

fn click(model: &mut TuiModel, target: &Hit) -> Intent {
    let (_, hits) = super::support::drawn_at(model, 220, 44);
    let (rect, _) = *hits
        .iter()
        .find(|(_, hit)| hit == target)
        .unwrap_or_else(|| panic!("{target:?} is drawn as a target"));
    model.hits = hits;
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 220, 44),
    )
}

/// The drawer lists an extension's preferences as their controls; its
/// Preferences button puts the keys on them, Enter flips a toggle, and a
/// number is typed and set, all in the drawer.
#[test]
fn preferences_are_listed_and_set_from_the_keyboard() {
    let mut model = with_preferences();
    let screen = screen_of(&model);
    assert!(screen.contains("Preferences ─"), "a ruled group: {screen}");
    let row_with = |label: &str, value: &str| {
        screen
            .lines()
            .any(|row| row.contains(label) && row.contains(value))
    };
    assert!(row_with("Show °F", "off"), "{screen}");
    assert!(row_with("Warn at", " 70 "), "{screen}");
    assert!(screen.contains("Preferences"), "{screen}");
    assert_eq!(model.edit_selected_preferences(), Intent::None);
    assert!(
        model.preferences.is_some(),
        "the keys are on the preferences"
    );
    assert_eq!(model.overlay, crate::ui::model::Overlay::None, "no dialog");
    assert_eq!(chosen_on(&model, "Show °F"), "off");
    let enter = key(KeyCode::Enter);
    assert_eq!(
        model.apply_key(enter),
        set("fahrenheit", "Show °F", serde_json::json!(true))
    );
    assert_eq!(
        chosen_on(&model, "Show °F"),
        "on",
        "the row says what was set"
    );
    model.apply_key(key(KeyCode::Down));
    assert_eq!(model.apply_key(enter), Intent::None, "a number is typed");
    let typing = screen_of(&model);
    assert!(
        typing.contains("A number from 40 to 71 · default 80"),
        "the drawer says what the value may be: {typing}"
    );
    for _ in 0..2 {
        model.apply_key(key(KeyCode::Backspace));
    }
    model.apply_key(key(KeyCode::Char('5')));
    model.apply_key(key(KeyCode::Char('0')));
    assert_eq!(
        model.apply_key(enter),
        set("warn-at", "Warn at", serde_json::json!(50))
    );
}

/// The values stand in one column against the drawer's right edge, and
/// under them the row the keys are on says its range and its default.
#[test]
fn the_drawers_preferences_align_their_values_and_say_the_range_and_default() {
    let model = focus_preferences();
    let text = screen_of(&model);
    let dialog: Vec<&str> = text.lines().collect();
    assert!(
        text.contains("A number from 1 to 90 · default 25"),
        "{text}"
    );
    let minutes = dialog
        .iter()
        .find(|row| row.contains("Minutes in a session"))
        .expect("the minutes row");
    let goal = dialog
        .iter()
        .find(|row| row.contains("Sessions a day"))
        .expect("the goal row");
    let column = |row: &str, glyph: char| row.chars().position(|ch| ch == glyph);
    let plus = crate::ui::theme::glyph(uze_theme::Symbol::StepIncrease)
        .chars()
        .next()
        .unwrap();
    assert!(column(minutes, plus).is_some(), "{minutes}");
    assert_eq!(column(minutes, plus), column(goal, plus), "{text}");
}

/// Moving between rows never moves what the drawer says under them, and
/// the detail follows the keyboard.
#[test]
fn moving_between_preferences_keeps_the_drawer_still() {
    let mut model = focus_preferences();
    let row_of = |text: &str, said: &str| text.lines().position(|row| row.contains(said));
    let before = screen_of(&model);
    model.apply_key(key(KeyCode::Down));
    let after = screen_of(&model);
    assert_eq!(
        row_of(&before, "Sessions a day"),
        row_of(&after, "Sessions a day")
    );
    assert_eq!(
        row_of(&before, "A number from 1 to 90"),
        row_of(&after, "A number from 1 to 12"),
        "the detail keeps its rows: {after}"
    );
    assert!(
        after.contains("A number from 1 to 12 · default 4"),
        "{after}"
    );
}

/// Right and left step a number by one and save it at once; at its bound
/// the step does nothing and saves nothing.
#[test]
fn the_arrows_step_a_number_within_its_bounds_and_save_it() {
    let mut model = focus_preferences();
    assert_eq!(
        model.apply_key(key(KeyCode::Right)),
        set("minutes", "Minutes in a session", serde_json::json!(26))
    );
    assert_eq!(
        model.apply_key(key(KeyCode::Left)),
        set("minutes", "Minutes in a session", serde_json::json!(25))
    );
    model.apply_key(key(KeyCode::Down));
    for _ in 4..12 {
        model.apply_key(key(KeyCode::Right));
    }
    assert_eq!(
        model.apply_key(key(KeyCode::Right)),
        Intent::None,
        "twelve is the most"
    );
    let text = screen_of(&model);
    assert!(text.contains(" 12 "), "{text}");
}

/// A click on a stepper's `+` is pressing → on its row, and on a row is
/// choosing it and nothing more.
#[test]
fn clicking_a_stepper_is_pressing_its_arrow() {
    let mut clicked = focus_preferences();
    let mut keyed = focus_preferences();
    assert_eq!(
        click(&mut clicked, &Hit::ExtensionPreferenceRow(1)),
        Intent::None,
        "a click on a row chooses it"
    );
    keyed.apply_key(key(KeyCode::Down));
    assert_eq!(clicked.preferences, keyed.preferences);
    assert_eq!(
        click(
            &mut clicked,
            &Hit::ExtensionPreferenceStep {
                index: 1,
                forward: true
            }
        ),
        keyed.apply_key(key(KeyCode::Right))
    );
    assert_eq!(
        click(
            &mut clicked,
            &Hit::ExtensionPreferenceStep {
                index: 0,
                forward: false
            }
        ),
        set("minutes", "Minutes in a session", serde_json::json!(24))
    );
    keyed.apply_key(key(KeyCode::Up));
    keyed.apply_key(key(KeyCode::Left));
    assert_eq!(clicked.preferences, keyed.preferences);
}

/// A preference's control answers a click straight from the drawer, the
/// keys then on its row; a click on a card gives them back to the cards.
#[test]
fn a_preference_is_set_from_the_drawer_without_opening_anything() {
    let mut model = focus_preferences();
    model.preferences = None;
    assert_eq!(
        click(
            &mut model,
            &Hit::ExtensionPreferenceStep {
                index: 1,
                forward: true
            }
        ),
        set("goal", "Sessions a day", serde_json::json!(5))
    );
    assert_eq!(model.overlay, crate::ui::model::Overlay::None);
    assert_eq!(
        model.preferences.as_ref().map(|editor| editor.selected),
        Some(1)
    );
    click(&mut model, &Hit::ExtensionRow(0));
    assert_eq!(model.preferences, None);
}

/// Preferences are the machine's: with no project in front, an extension
/// that is allowed still has them set from its drawer, and a click on its
/// value never reaches the screen's Enter, which would switch it off.
#[test]
fn a_preference_is_set_with_no_project_in_front() {
    let mut model = focus_preferences();
    model.preferences = None;
    model.extension_project = None;
    model.extension_standings[0].condition = uze_application::ExtensionCondition::Consented;
    assert_eq!(
        click(
            &mut model,
            &Hit::ExtensionPreferenceStep {
                index: 0,
                forward: true
            }
        ),
        set("minutes", "Minutes in a session", serde_json::json!(26))
    );
    model.preferences = None;
    assert_eq!(
        click(&mut model, &Hit::ExtensionPreferenceValue(1)),
        Intent::None,
        "a click on a number types it"
    );
    assert!(
        matches!(&model.preferences, Some(editor) if editor.typing.is_some()),
        "the value is being typed"
    );
}

/// A refresh that puts another extension under the selection takes the
/// keys off the preferences being set: they never write into one the
/// drawer no longer shows.
#[test]
fn a_refresh_that_moves_the_selection_puts_the_preferences_away() {
    let mut model = focus_preferences();
    let mut other = declared(&[]);
    other.identity.id = "gpu-meter".to_owned();
    other.name = "GPU".to_owned();
    let mut standing = model.extension_standings[0].clone();
    standing.id = "gpu-meter".to_owned();
    model.refreshed(crate::ui::model::RefreshData {
        declared_extensions: vec![other],
        extension_standings: vec![standing],
        extension_project: model.extension_project.clone(),
        ..Default::default()
    });
    assert_eq!(model.preferences, None);
    assert_eq!(model.apply_key(key(KeyCode::Right)), Intent::None);
}

/// Every key the footer names while the keys are on the preferences is a
/// target doing what the key does: enter types the number, esc gives the
/// keys back to the cards.
#[test]
fn clicking_a_preferences_footer_hint_is_pressing_its_key() {
    let mut clicked = focus_preferences();
    let mut keyed = focus_preferences();
    assert_eq!(
        click(
            &mut clicked,
            &Hit::OfferedAction(uze_keys::Action::Activate)
        ),
        keyed.apply_key(key(KeyCode::Enter))
    );
    assert_eq!(clicked.preferences, keyed.preferences);
    assert!(
        matches!(&clicked.preferences, Some(editor) if editor.typing.is_some()),
        "enter on a number types it"
    );
    let mut clicked = focus_preferences();
    let mut keyed = focus_preferences();
    click(&mut clicked, &Hit::OfferedAction(uze_keys::Action::Dismiss));
    keyed.apply_key(key(KeyCode::Esc));
    assert_eq!(clicked.preferences, None);
    assert_eq!(clicked.preferences, keyed.preferences);
}

/// The footer's keys stand where they are drawn: each target's rect holds
/// the word the key is named by.
#[test]
fn the_preferences_footer_hints_are_drawn_where_they_answer() {
    let model = focus_preferences();
    let (terminal, hits) = super::support::drawn_at(&model, 220, 44);
    let buffer = terminal.backend().buffer();
    let said = |rect: Rect| -> String {
        (rect.x..rect.right())
            .map(|x| buffer[(x, rect.y)].symbol().to_owned())
            .collect()
    };
    for (action, word) in [
        (uze_keys::Action::Activate, "enter type"),
        (uze_keys::Action::Dismiss, "esc back"),
    ] {
        let (rect, _) = hits
            .iter()
            .find(|(_, hit)| *hit == Hit::OfferedAction(action))
            .unwrap_or_else(|| panic!("{word} is a target"));
        assert_eq!(said(*rect), word);
    }
}

/// A secret is typed into a field that draws a dot per character, starts
/// empty however it was set, and is then said only to be set.
#[test]
fn a_secret_is_typed_masked_and_never_shown_again() {
    let mut standing = standing(uze_application::ExtensionCondition::Enabled);
    standing.preferences = vec![uze_application::PreferenceRow {
        name: "token".to_owned(),
        label: "GitHub token".to_owned(),
        kind: "secret",
        value: serde_json::json!(true),
        values: Vec::new(),
        default: serde_json::Value::Null,
        min: None,
        max: None,
    }];
    let mut model = standing_in_a_project(standing);
    assert!(
        screen_of(&model)
            .lines()
            .any(|row| row.contains("GitHub token") && row.contains("set"))
    );
    model.edit_selected_preferences();
    let editor = screen_of(&model);
    assert!(editor.contains("GitHub token"), "{editor}");
    let said_set = |screen: &str| {
        screen
            .lines()
            .any(|row| row.contains("GitHub token") && row.trim_end().ends_with("set"))
    };
    assert!(said_set(&editor), "{editor}");
    assert!(
        editor.contains("A secret, kept in"),
        "the detail says where it is kept: {editor}"
    );
    let enter = key(KeyCode::Enter);
    assert_eq!(model.apply_key(enter), Intent::None, "a secret is typed");
    for ch in "ghp_abc".chars() {
        model.apply_key(key(KeyCode::Char(ch)));
    }
    let typing = screen_of(&model);
    assert!(!typing.contains("ghp_abc"), "{typing}");
    assert!(typing.contains("•••••••"), "{typing}");
    assert!(typing.contains("A secret, kept in"), "{typing}");
    assert_eq!(
        model.apply_key(enter),
        set("token", "GitHub token", serde_json::json!("ghp_abc"))
    );
    let after = screen_of(&model);
    assert!(said_set(&after), "{after}");
    assert!(!after.contains("ghp_abc"), "{after}");
}

/// A toggle is `off` and `on` side by side, as a settings form draws it:
/// a click on the one not chosen sets it, on the one chosen sets nothing.
#[test]
fn clicking_a_toggles_option_chooses_it() {
    let mut model = with_preferences();
    model.edit_selected_preferences();
    let on = Hit::ExtensionPreferenceChoice {
        index: 0,
        option: 1,
    };
    assert_eq!(
        click(&mut model, &on),
        set("fahrenheit", "Show °F", serde_json::json!(true))
    );
    assert_eq!(chosen_on(&model, "Show °F"), "on");
    assert_eq!(click(&mut model, &on), Intent::None);
}

/// Two extensions drawing a row in the workspace sidebar's header, the
/// one found second selected: `gpu-meter` sorts before `gsd-band`.
fn two_status_rows() -> TuiModel {
    let mut first = declared(&[]);
    first.statuses = vec!["Load".to_owned()];
    let mut second = declared(&[]);
    second.identity.id = "gpu-meter".to_owned();
    second.name = "GPU".to_owned();
    second.statuses = vec!["GPU".to_owned()];
    let mut model = listing_a_declared_extension(&[]);
    model.declared_extensions = vec![first, second];
    model.remembered.extension_screen.selected = model.extensions.len();
    model
}

fn offered_button(model: &TuiModel, action: uze_keys::Action) -> Option<Rect> {
    let (_, hits) = super::support::drawn_at(model, 220, 44);
    hits.iter()
        .find(|(_, hit)| *hit == Hit::OfferedAction(action))
        .map(|(rect, _)| *rect)
}

/// Move up and Move down order the rows the workspace draws in its
/// sidebar's header; a click on either is the action, and the order is
/// what the layout keeps.
#[test]
fn moving_a_status_row_orders_the_sidebars_header_and_a_click_is_the_action() {
    let mut model = two_status_rows();
    let screen = screen_of(&model);
    assert!(screen.contains("Load · sidebar header row"), "{screen}");
    let selected = model.selected_extension().expect("a selection").id();
    assert_eq!(selected, "gsd@market/gsd-band");
    assert!(
        offered_button(&model, uze_keys::Action::MoveItemDown).is_none(),
        "already the last"
    );
    let up = offered_button(&model, uze_keys::Action::MoveItemUp).expect("Move up is a button");
    let (_, hits) = super::support::drawn_at(&model, 220, 44);
    model.hits = hits;
    let clicked = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: up.x,
            row: up.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 220, 44),
    );
    assert_eq!(clicked, Intent::None);
    let moved = vec![
        "gsd@market/gsd-band".to_owned(),
        "gsd@market/gpu-meter".to_owned(),
    ];
    assert_eq!(model.status_order, moved);
    assert_eq!(model.management_layout().status_order, moved, "kept");
    assert!(
        offered_button(&model, uze_keys::Action::MoveItemUp).is_none(),
        "now the first"
    );

    model.move_selected_status(false);
    assert_eq!(
        model.status_order,
        vec![
            "gsd@market/gpu-meter".to_owned(),
            "gsd@market/gsd-band".to_owned(),
        ],
        "Move down, as the key would"
    );
}

/// Allowing an extension that lays a band over each agent says what that
/// costs, beside its grants.
#[test]
fn consenting_to_a_band_says_the_rows_it_takes_over_each_agent() {
    let mut standing = standing(uze_application::ExtensionCondition::NeedsConsent);
    standing.band_rows = Some(3);
    let mut model = standing_in_a_project(standing);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(model.apply_key(enter), Intent::None);
    let screen = screen_of(&model);
    assert!(screen.contains("Allow GSD to run?"), "{screen}");
    assert!(
        screen.contains("draws 3 rows over each agent's pane"),
        "{screen}"
    );
}

#[test]
fn a_reopened_extensions_screen_still_lists_the_plugins_extensions() {
    let layout = uze_application::ManagementLayout::default();
    let mut first = TuiModel::recall(None, &layout);
    first.refreshed(crate::ui::model::RefreshData {
        declared_extensions: vec![declared(&[])],
        ..Default::default()
    });
    let again = TuiModel::recall(Some(first.remember()), &layout);
    assert_eq!(
        again.declared_extensions.len(),
        1,
        "a reopening inside the resolution's life asks nothing again, so it keeps what it read"
    );
}

/// What a click on why an extension is withheld asks for: the screen opens
/// on that extension, now when the list holds it, or as soon as one does.
#[test]
fn revealing_an_extension_selects_it_now_or_once_the_list_holds_it() {
    let mut model = two_status_rows();
    let gpu = model.declared_extensions[1].identity.clone();
    model.remembered.extension_screen.selected = 0;
    model.reveal_extension(gpu.clone());
    assert_eq!(
        model.selected_extension().expect("a selection").id(),
        "gsd@market/gpu-meter"
    );

    let held = std::mem::take(&mut model.declared_extensions);
    model.remembered.extension_screen.selected = 0;
    model.reveal_extension(gpu.clone());
    assert!(model.revealing.is_some(), "waits for a list that holds it");
    model.declared_extensions = held;
    model.reveal_pending();
    assert_eq!(
        model.selected_extension().expect("a selection").id(),
        "gsd@market/gpu-meter"
    );
}

/// The drawer names the plugin an extension comes with, and that line is
/// the way to it: hovered it lights, clicked it opens the Marketplace on
/// that plugin, unfolded on the extension, as the index's entry does.
#[test]
fn clicking_the_drawers_plugin_line_is_the_index_entry_that_shows_the_plugin() {
    let start = || {
        let mut model = listing_a_declared_extension(&[]);
        model.remembered.marketplace_plugins =
            vec![super::support::marketplace_plugin("market", "gsd", true)];
        model
    };
    let mut clicked = start();
    let (terminal, hits) = super::support::drawn_at(&clicked, 140, 40);
    let (rect, _) = *hits
        .iter()
        .find(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::ShowPlugin))
        .expect("the plugin line is a target");
    let row = &super::support::buffer_rows(&terminal)[rect.y as usize];
    assert!(row.contains("from") && row.contains("gsd@market"), "{row}");
    clicked.hits = hits;
    let at = |kind| MouseEvent {
        kind,
        column: rect.x + 1,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    };
    clicked.apply_mouse(at(MouseEventKind::Moved), Rect::new(0, 0, 140, 40));
    assert_eq!(clicked.hovered_offer, Some(uze_keys::Action::ShowPlugin));
    let lit = super::support::drawn(&clicked);
    let resting = super::support::drawn(&start());
    let cell = |buffer: &ratatui::buffer::Buffer| {
        // The drawn buffer is 100 wide; find the line there.
        let rows: Vec<String> = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect();
        let y = rows
            .iter()
            .position(|row| row.contains("from") && row.contains("gsd@market"))
            .expect("the plugin line");
        let x = rows[y].find("gsd@market").unwrap();
        let x = rows[y][..x].chars().count() as u16;
        buffer[(x, y as u16)].fg
    };
    assert_ne!(cell(&lit), cell(&resting), "the hover lights the line");
    assert_eq!(
        cell(&lit),
        crate::ui::theme::color(crate::ui::theme::Token::TextBright)
    );
    clicked.apply_mouse(
        at(MouseEventKind::Down(MouseButton::Left)),
        Rect::new(0, 0, 140, 40),
    );

    let mut pressed = start();
    let scopes = pressed.scopes();
    assert!(
        pressed
            .action_index_rows(&scopes, "")
            .iter()
            .any(|(action, _)| *action == uze_keys::Action::ShowPlugin),
        "the keyboard's way across is the index"
    );
    pressed.act(uze_keys::Action::ShowPlugin);

    for model in [&mut clicked, &mut pressed] {
        assert_eq!(model.route, Route::Plugins);
        assert_eq!(
            model
                .selected_marketplace_plugin()
                .map(|plugin| plugin.name)
                .as_deref(),
            Some("gsd")
        );
        assert!(model.expanded_plugins.contains("gsd@market"));
        // Its contents arrive, and the keyboard stands on the extension.
        model
            .plugin_extensions
            .insert("gsd@market".to_owned(), vec![declared(&[]).identity]);
        assert_eq!(
            model.plugin_tree_cursor(),
            crate::ui::model::PluginTreeRow::Extension(0, declared(&[]).identity)
        );
    }
}

/// A built-in comes with no plugin: nothing leads to the Marketplace.
#[test]
fn a_built_in_extension_links_to_no_plugin() {
    let mut model = listing_a_declared_extension(&[]);
    model.remembered.extension_screen.selected = 0;
    let (_, hits) = super::support::drawn_at(&model, 140, 40);
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::ShowPlugin))
    );
    assert_eq!(model.selected_link(), None);
}
