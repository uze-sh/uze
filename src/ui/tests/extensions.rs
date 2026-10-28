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
    assert!(screen.contains("Band · section"), "{screen}");
    assert!(!screen.contains("last compile"), "{screen}");
}

#[test]
fn a_declared_extension_that_failed_says_so_on_its_card_and_in_its_drawer() {
    let mut model = listing_a_declared_extension(&[
        "board.kdl:3:5: unknown node `Stak`",
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
    assert!(
        screen.contains("the workspace, in every"),
        "this screen answers for the machine: {screen}"
    );
    assert!(
        screen.contains("read:checkout · reads the files of the checkout"),
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
    assert!(grants.contains("Allow this extension to run?"), "{grants}");
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
        screen.contains("nvidia-smi · `nvidia-smi` not found"),
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
        },
        uze_application::PreferenceRow {
            name: "warn-at".to_owned(),
            label: "Warn at".to_owned(),
            kind: "number",
            value: serde_json::json!(70),
            values: Vec::new(),
        },
    ];
    standing_in_a_project(standing)
}

fn set(name: &str, label: &str, value: serde_json::Value) -> Intent {
    Intent::SetPreference {
        identity: declared(&[]).identity,
        name: name.to_owned(),
        label: label.to_owned(),
        value,
    }
}

/// The drawer lists an extension's preferences; its Preferences button
/// opens them, Enter flips a toggle, and a number is typed and set.
#[test]
fn preferences_are_listed_and_set_from_the_keyboard() {
    let mut model = with_preferences();
    let screen = screen_of(&model);
    assert!(screen.contains("preferences"), "{screen}");
    assert!(screen.contains("Show °F · off"), "{screen}");
    assert!(screen.contains("Warn at · 70"), "{screen}");
    assert!(screen.contains("Preferences"), "{screen}");
    assert_eq!(model.edit_selected_preferences(), Intent::None);
    let editor = screen_of(&model);
    assert!(editor.contains("Show °F  off"), "{editor}");
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        model.apply_key(enter),
        set("fahrenheit", "Show °F", serde_json::json!(true))
    );
    assert!(
        screen_of(&model).contains("Show °F  on"),
        "the row says what was set"
    );
    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(model.apply_key(enter), Intent::None, "a number is typed");
    for _ in 0..2 {
        model.apply_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    }
    model.apply_key(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE));
    model.apply_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE));
    assert_eq!(
        model.apply_key(enter),
        set("warn-at", "Warn at", serde_json::json!(50))
    );
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
    }];
    let mut model = standing_in_a_project(standing);
    assert!(screen_of(&model).contains("GitHub token · set"));
    model.edit_selected_preferences();
    assert!(screen_of(&model).contains("GitHub token  set"));
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(model.apply_key(enter), Intent::None, "a secret is typed");
    for ch in "ghp_abc".chars() {
        model.apply_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let typing = screen_of(&model);
    assert!(!typing.contains("ghp_abc"), "{typing}");
    assert!(typing.contains("•••••••"), "{typing}");
    assert!(typing.contains("never shown"), "{typing}");
    assert_eq!(
        model.apply_key(enter),
        set("token", "GitHub token", serde_json::json!("ghp_abc"))
    );
    let after = screen_of(&model);
    assert!(after.contains("GitHub token  set"), "{after}");
    assert!(!after.contains("ghp_abc"), "{after}");
}

/// A click on a preference's row is choosing it and pressing Enter.
#[test]
fn clicking_a_preference_row_is_pressing_enter_on_it() {
    let mut model = with_preferences();
    model.edit_selected_preferences();
    let (_, hits) = super::support::drawn_at(&model, 220, 44);
    let (row, _) = *hits
        .iter()
        .find(|(_, hit)| *hit == Hit::ExtensionPreferenceRow(0))
        .expect("the first row is clickable");
    model.hits = hits;
    let clicked = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: row.x + 1,
            row: row.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 220, 44),
    );
    assert_eq!(
        clicked,
        set("fahrenheit", "Show °F", serde_json::json!(true))
    );
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
    assert!(screen.contains("Load · status row"), "{screen}");
    assert!(screen.contains("its row in the sidebar"), "{screen}");
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
    assert!(screen.contains("Allow this extension to run?"), "{screen}");
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
