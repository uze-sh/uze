//! The Profiles screen: its panels, the preferences editor, the preview and applying a profile.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::ui::hit::Hit;
use crate::ui::model::{
    Confirmation, Focus, Overlay, PREFERENCE_ROW_COUNT, ProfilePanel, Route, Status, TuiModel,
};
use crate::ui::worker::Intent;

use super::support::*;

#[test]
fn profiles_read_only_navigation_never_produces_a_mutating_intent() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    for key in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Tab,
        KeyCode::BackTab,
    ] {
        let intent = model.apply_key(KeyEvent::new(key, KeyModifiers::NONE));
        assert_eq!(
            intent,
            Intent::None,
            "Profiles navigation must never mutate, got {intent:?}"
        );
    }
}

#[test]
fn tab_cycles_the_three_profile_panels_while_content_is_focused() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    assert_eq!(model.profile_panel, ProfilePanel::List);
    model.apply_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(model.profile_panel, ProfilePanel::Editor);
    model.apply_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(model.profile_panel, ProfilePanel::Harnesses);
    model.apply_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(model.profile_panel, ProfilePanel::List);
    model.apply_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE));
    assert_eq!(model.profile_panel, ProfilePanel::Harnesses);
}

#[test]
fn left_right_cycle_the_selected_preference_value_and_persist_it() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_panel = ProfilePanel::Editor;
    model.profile_editor_selected = 0; // autonomy
    let before = model.remembered.profiles[0].preferences.autonomy;
    let intent = model.apply_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_ne!(
        model.remembered.profiles[0].preferences.autonomy, before,
        "cycling must mutate optimistically"
    );
    assert!(matches!(intent, Intent::UpdatePreferences { .. }));
    let after_right = model.remembered.profiles[0].preferences.autonomy;
    model.apply_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(
        model.remembered.profiles[0].preferences.autonomy, before,
        "left must undo right's cycle step"
    );
    let _ = after_right;
}

#[test]
fn left_right_outside_the_editor_panel_falls_back_to_sidebar_focus() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_panel = ProfilePanel::List;
    model.apply_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(model.focus, Focus::Sidebar);
}

#[test]
fn space_toggles_harness_selection_only_in_the_harnesses_panel() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_harness_selected = 0;
    let harness_id = model.remembered.doctor.as_ref().unwrap().harnesses[0]
        .integration
        .clone();
    let was_selected = model.profile_harness_selection.contains(&harness_id);

    // No-op outside the Harnesses panel.
    model.profile_panel = ProfilePanel::List;
    model.apply_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(
        model.profile_harness_selection.contains(&harness_id),
        was_selected
    );

    model.profile_panel = ProfilePanel::Harnesses;
    model.apply_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(
        model.profile_harness_selection.contains(&harness_id),
        !was_selected
    );
}

#[test]
fn n_opens_new_profile_overlay_and_submitting_creates_it() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.apply_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(model.overlay, Overlay::NewProfile(String::new()));
    for ch in "Team Backend".chars() {
        model.apply_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let intent = model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(intent, Intent::CreateProfile("team-backend".to_owned()));
    assert_eq!(model.overlay, Overlay::None);
}

#[test]
fn clicking_new_profile_opens_the_profile_overlay() {
    let mut model = model_with_data();
    model.hits = vec![(
        Rect::new(10, 4, 5, 1),
        Hit::OfferedAction(uze_keys::Action::NewProfile),
    )];

    assert_eq!(model.click(12, 4), Intent::None);
    assert_eq!(model.overlay, Overlay::NewProfile(String::new()));
}

#[test]
fn the_drawers_delete_button_opens_the_delete_confirmation() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    let id = model.remembered.profiles[0].id.clone();
    model.hits = vec![(
        Rect::new(10, 4, 8, 1),
        Hit::OfferedAction(uze_keys::Action::DeleteProfile),
    )];

    assert_eq!(model.click(12, 4), Intent::None);
    assert!(matches!(
        &model.overlay,
        Overlay::Confirm { kind: Confirmation::DeleteProfile(confirmed_id), .. } if *confirmed_id == id
    ));
}

#[test]
fn the_drawers_apply_button_targets_checked_harnesses() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.remembered.profiles[0].active = false;
    let id = model.remembered.profiles[0].id.clone();
    model.hits = vec![(
        Rect::new(18, 4, 9, 1),
        Hit::OfferedAction(uze_keys::Action::ApplyProfile),
    )];

    let Intent::ApplyProfile {
        id: applied_id,
        preferences,
        harness_ids,
    } = model.click(19, 4)
    else {
        panic!("expected ApplyProfile");
    };
    assert_eq!(applied_id, id);
    assert_eq!(preferences, model.remembered.profiles[0].preferences);
    assert_eq!(
        harness_ids,
        vec!["claude-code".to_owned(), "codex".to_owned()]
    );
}

/// Active is not in effect: a preference edited after the last apply has
/// to be writable without first making some other profile active.
#[test]
fn the_active_profile_can_be_applied_again() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    assert!(model.remembered.profiles[0].active);
    model.profile_panel = ProfilePanel::Editor;
    model.profile_editor_selected = 2;
    model.cycle_selected_preference(true);
    let edited = model.remembered.profiles[0].preferences;

    let Intent::ApplyProfile { preferences, .. } = model.act(uze_keys::Action::ApplyProfile) else {
        panic!("expected ApplyProfile");
    };
    assert_eq!(
        preferences, edited,
        "the apply carries what is on screen, not what reached disk"
    );
}

#[test]
fn applying_with_no_harness_checked_says_why_and_writes_nothing() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.profile_harness_selection.clear();
    assert_eq!(model.act(uze_keys::Action::ApplyProfile), Intent::None);
    assert!(
        matches!(&model.status, Status::Success(message) if message.contains("harness")),
        "{:?}",
        model.status
    );
}

fn preview_fixture(model: &TuiModel) -> uze_application::application::ProfilePreview {
    use uze_application::application::HarnessPreview;
    use uze_application::{
        AxisPlan, CompatibilityRoute, KeyPlan, PlannedValue, PreferenceAxis, PreferencePlan,
    };
    let claude = PreferencePlan {
        config_path: PathBuf::from("/home/someone/.claude/settings.json"),
        axes: vec![
            AxisPlan {
                axis: PreferenceAxis::Autonomy,
                route: CompatibilityRoute::Native,
                summary: "permissions.defaultMode = auto".to_owned(),
                note: None,
                keys: vec![KeyPlan {
                    key: "permissions.defaultMode".to_owned(),
                    current: Some("\"auto\"".to_owned()),
                    planned: PlannedValue::Set("\"auto\"".to_owned()),
                }],
            },
            AxisPlan {
                axis: PreferenceAxis::Sandbox,
                route: CompatibilityRoute::Degraded,
                summary: "sandbox.enabled = true".to_owned(),
                note: Some("Claude cannot start its sandbox here without socat".to_owned()),
                keys: vec![KeyPlan {
                    key: "sandbox.enabled".to_owned(),
                    current: Some("true".to_owned()),
                    planned: PlannedValue::Set("true".to_owned()),
                }],
            },
            AxisPlan {
                axis: PreferenceAxis::Model,
                route: CompatibilityRoute::Native,
                summary: "model unset".to_owned(),
                note: None,
                keys: vec![KeyPlan {
                    key: "model".to_owned(),
                    current: Some("\"default\"".to_owned()),
                    planned: PlannedValue::Removed,
                }],
            },
        ],
    };
    uze_application::application::ProfilePreview {
        preferences: model.remembered.profiles[0].preferences,
        harnesses: vec![
            HarnessPreview {
                integration: "claude-code".to_owned(),
                plan: Ok(claude),
            },
            HarnessPreview {
                integration: "codex".to_owned(),
                plan: Err("`config.toml` is not valid TOML".to_owned()),
            },
        ],
    }
}

#[test]
fn the_profiles_screen_asks_for_its_preview_once_and_again_after_an_edit() {
    let mut model = model_with_data();
    assert_eq!(
        model.profile_preview_intent(),
        Intent::None,
        "only the Profiles screen reads it"
    );
    model.set_route(Route::Profiles);
    let Intent::PreviewProfile(question) = model.profile_preview_intent() else {
        panic!("the preview is read without being asked for");
    };
    assert_eq!(
        question.preferences,
        model.remembered.profiles[0].preferences
    );
    assert_eq!(question.harness_ids, vec!["claude-code", "codex"]);

    model.profile_preview_asked = Some(question);
    assert_eq!(model.profile_preview_intent(), Intent::None, "asked once");

    model.profile_panel = ProfilePanel::Editor;
    model.cycle_selected_preference(true);
    assert!(matches!(
        model.profile_preview_intent(),
        Intent::PreviewProfile(_)
    ));
}

fn answer_preview(model: &mut TuiModel) {
    let question = model.profile_preview_question().unwrap();
    let preview = preview_fixture(model);
    model.profile_previewed(question, Ok(preview));
}

#[test]
fn a_preview_of_other_preferences_is_never_shown_as_this_ones() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    answer_preview(&mut model);
    assert!(model.current_profile_preview().is_some());
    model.remembered.profiles_selected = 1;
    assert!(
        model.current_profile_preview().is_none(),
        "safe-mode's preferences are not dev-autonomous's"
    );
}

/// A read that started before an apply finished must never pass for one
/// made after it, even when it answers the very same preferences.
#[test]
fn a_preview_read_before_the_harnesses_changed_is_refused() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    let before = model.profile_preview_question().unwrap();
    model.profile_preview_asked = Some(before.clone());
    model.invalidate_profile_preview();
    model.profile_previewed(before, Ok(preview_fixture(&model)));
    assert!(model.current_profile_preview().is_none());
    assert!(
        matches!(model.profile_preview_intent(), Intent::PreviewProfile(_)),
        "and the screen asks again"
    );
}

#[test]
fn a_preview_that_could_not_be_read_says_so_and_is_not_retried_in_a_loop() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    let question = model.profile_preview_question().unwrap();
    model.profile_preview_asked = Some(question.clone());
    model.profile_previewed(question, Err("the store is locked".to_owned()));
    assert_eq!(model.profile_preview_intent(), Intent::None);
    let lines: Vec<String> = crate::ui::view::profiles::preview_lines(&model, 80)
        .lines
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("the store is locked")),
        "{lines:?}"
    );
    model.invalidate_profile_preview();
    assert!(
        matches!(model.profile_preview_intent(), Intent::PreviewProfile(_)),
        "a refresh is what asks again"
    );
}

#[test]
fn v_opens_the_preview_and_esc_closes_it_before_anything_else() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_panel = ProfilePanel::Editor;
    model.apply_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(model.profile_preview_open);
    model.apply_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!model.profile_preview_open);
    assert_eq!(
        model.profile_panel,
        ProfilePanel::Editor,
        "Esc closed one thing"
    );
}

/// A preference is one of several values; its row says so with steppers,
/// and the pointer changes it with them.
#[test]
fn a_preference_steps_through_its_values_from_its_arrows() {
    use uze_core::preference::Autonomy;

    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    let (terminal, hits) = drawn_at(&model, 160, 40);
    let rows = buffer_rows(&terminal);
    let next = crate::ui::theme::glyph(crate::ui::theme::Symbol::StepNext);
    let stepper_columns: Vec<usize> = ["autonomy", "sandbox", "model"]
        .iter()
        .map(|axis| {
            let row = rows
                .iter()
                .find(|row| row.contains(&format!("{axis}   ")))
                .unwrap_or_else(|| panic!("no {axis} row: {rows:#?}"));
            row.chars()
                .collect::<Vec<_>>()
                .iter()
                .rposition(|cell| next.starts_with(*cell))
                .unwrap_or_else(|| panic!("{axis} offers no next value: {row}"))
        })
        .collect();
    assert!(
        stepper_columns.windows(2).all(|pair| pair[0] == pair[1]),
        "the steppers stand in one column: {stepper_columns:?}"
    );

    model.hits = hits;
    let target = |forward: bool| {
        model
            .hits
            .iter()
            .find(|(_, hit)| *hit == Hit::StepPreference { index: 0, forward })
            .map(|(rect, _)| (rect.x, rect.y))
            .expect("each arrow is a target of its own")
    };
    let (next_x, next_y) = target(true);
    let (previous_x, previous_y) = target(false);
    assert_eq!(
        model.remembered.profiles[0].preferences.autonomy,
        Autonomy::Auto
    );
    let Intent::UpdatePreferences { preferences, .. } = model.click(next_x, next_y) else {
        panic!("the next arrow changes the value");
    };
    assert_eq!(preferences.autonomy, Autonomy::Unattended);
    assert_eq!(model.profile_panel, ProfilePanel::Editor);
    let Intent::UpdatePreferences { preferences, .. } = model.click(previous_x, previous_y) else {
        panic!("the previous arrow changes it back");
    };
    assert_eq!(preferences.autonomy, Autonomy::Auto);
}

/// A profile's actions are its drawer's buttons, as on every screen; the
/// row itself only names the profile.
#[test]
fn a_profile_row_carries_no_action_of_its_own() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    let (terminal, hits) = drawn_at(&model, 160, 40);
    let rows = buffer_rows(&terminal);
    let row = rows
        .iter()
        .find(|row| row.contains("dev-autonomous"))
        .expect("the selected profile is drawn");
    let tree = row.split("Harnesses").next().unwrap_or(row);
    assert!(
        !tree.contains("apply") && !tree.contains("remove"),
        "{tree}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("  Apply  ") && row.contains("Delete")),
        "the drawer offers both as buttons: {rows:#?}"
    );
    let offered: Vec<_> = hits
        .iter()
        .filter_map(|(_, hit)| match hit {
            Hit::OfferedAction(action) => Some(*action),
            _ => None,
        })
        .collect();
    assert!(offered.contains(&uze_keys::Action::ApplyProfile));
    assert!(offered.contains(&uze_keys::Action::DeleteProfile));
}

#[test]
fn the_preview_shows_each_key_as_it_is_and_as_it_will_be() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    answer_preview(&mut model);
    model.profile_preview_open = true;
    let (terminal, hits) = drawn_at(&model, 160, 40);
    let rows = buffer_rows(&terminal);
    let row = |needle: &str| {
        rows.iter()
            .find(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle}: {rows:#?}"))
    };
    let position = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle}: {rows:#?}"))
    };
    let heading = row("autonomy   sandbox");
    assert!(
        heading.contains("model"),
        "the table names its axes: {heading}"
    );
    let claude_row = rows
        .iter()
        .position(|row| row.contains("Claude Code") && row.contains("as asked"))
        .unwrap_or_else(|| panic!("Claude's row in the table: {rows:#?}"));
    for cell in ["partial", "1 change"] {
        assert!(
            rows[claude_row].contains(cell),
            "{cell}: {}",
            rows[claude_row]
        );
    }
    let open = crate::ui::theme::glyph(crate::ui::theme::Symbol::ChevronExpanded);
    assert!(
        row(&format!("{open} Codex ")).contains("cannot apply"),
        "a harness whose file cannot be read says so on its row"
    );
    assert!(
        position(".claude/settings.json") > claude_row,
        "a harness with something to write opens by itself"
    );
    let model_row = row("\"default\"");
    assert!(
        model_row.contains("remove  ") && model_row.contains("unset"),
        "a setting says what applying does to it: {model_row}"
    );
    assert!(
        position("without socat") < position("remove  "),
        "what keeps an axis from being honoured is read before the settings"
    );
    assert!(
        position("remove  ") < position("permissions.defaultMode"),
        "what changes is read before what already holds"
    );
    row("not valid TOML");
    row("partial: not fully honoured");
    assert!(
        row("would change").contains("1 setting in 1 harness"),
        "the preview leads with its answer"
    );
    let header = row("+ new");
    assert!(
        header.contains("  Preview  "),
        "previewing is a button beside new, not a drawer action: {header}"
    );
    let buttons = row("  Apply  ");
    assert!(buttons.contains("Delete") && !buttons.contains("Preview"));
    let preview_button = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::PreviewProfile))
        .map(|(rect, _)| *rect)
        .expect("the Preview button is a target");
    model.hits = hits;
    model.click(preview_button.x + 2, preview_button.y);
    assert!(
        !model.profile_preview_open,
        "and clicking it again closes the preview"
    );
    assert!(
        rows.iter().any(|row| row.contains("Active, not in effect")),
        "the drawer no longer claims the profile is in use: {rows:#?}"
    );
}

/// Nothing to write is one quiet row; opening it shows the file anyway,
/// and a click on the row does what Enter does.
#[test]
fn a_harness_in_the_preview_opens_and_closes() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    answer_preview(&mut model);
    model.profile_preview_open = true;
    let claude_open = |model: &TuiModel| {
        crate::ui::view::profiles::preview_lines(model, 120)
            .lines
            .iter()
            .any(|line| line.to_string().contains(".claude/settings.json"))
    };
    assert!(claude_open(&model), "open by itself: it has a change");
    model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        !claude_open(&model),
        "Enter closes the harness under the cursor"
    );
    model.hits = vec![(Rect::new(0, 5, 80, 1), Hit::PreviewHarness(0))];
    model.click(3, 5);
    assert!(claude_open(&model), "and a click on its row opens it again");
    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(model.profile_preview_cursor, 1);
    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(
        model.profile_preview_cursor, 1,
        "the cursor stops at the last harness"
    );
}

#[test]
fn new_profile_overlay_esc_cancels_without_intent() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.apply_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    for ch in "x".chars() {
        model.apply_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let intent = model.apply_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(intent, Intent::None);
    assert_eq!(model.overlay, Overlay::None);
}

#[test]
fn d_on_the_list_panel_opens_a_delete_confirmation_that_a_stray_click_cannot_confirm() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_panel = ProfilePanel::List;
    model.remembered.profiles_selected = 0;
    let id = model.remembered.profiles[0].id.clone();
    model.apply_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(matches!(
        &model.overlay,
        Overlay::Confirm { kind: Confirmation::DeleteProfile(confirmed_id), .. } if *confirmed_id == id
    ));

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
        "a stray click must never confirm delete"
    );
    assert_eq!(model.overlay, Overlay::None);
}

#[test]
fn confirming_delete_with_enter_emits_delete_profile_intent() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_panel = ProfilePanel::List;
    model.remembered.profiles_selected = 0;
    let id = model.remembered.profiles[0].id.clone();
    model.apply_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    let intent = model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(intent, Intent::DeleteProfile(id));
    assert_eq!(model.overlay, Overlay::None);
}

#[test]
fn applying_a_profile_is_offered_without_a_key() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_panel = ProfilePanel::List;
    model.remembered.profiles_selected = 1;
    let id = model.remembered.profiles[1].id.clone();
    // `s` sets up a harness and nothing else; applying a profile is
    // offered by its row's own actions and its drawer's button — and it
    // writes, rather than only marking the profile active.
    let intent = model.act(uze_keys::Action::ApplyProfile);
    assert!(matches!(intent, Intent::ApplyProfile { id: applied, .. } if applied == id));
}

#[test]
fn a_is_inert_on_the_profiles_screen() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.remembered.profiles_selected = 0;
    let intent = model.apply_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(intent, Intent::None);
}

#[test]
fn a_is_inert_with_no_harnesses_selected() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.focus = Focus::Content;
    model.profile_harness_selection.clear();
    let intent = model.apply_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(intent, Intent::None);
}

#[test]
fn editor_selection_clamps_to_the_preference_row_count() {
    let mut model = model_with_data();
    model.set_route(Route::Profiles);
    model.profile_panel = ProfilePanel::Editor;
    for _ in 0..10 {
        model.move_profile_selection(1);
    }
    assert_eq!(model.profile_editor_selected, PREFERENCE_ROW_COUNT - 1);
    for _ in 0..10 {
        model.move_profile_selection(-1);
    }
    assert_eq!(model.profile_editor_selected, 0);
}
