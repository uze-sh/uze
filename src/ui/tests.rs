//! Tests for the management client, one file per screen or concern.

// Gated again so the architecture suite, which reads `#[cfg(test)] mod`
// declarations, knows these files are test code.
#[cfg(test)]
mod extensions;
#[cfg(test)]
mod layout;
#[cfg(test)]
mod marketplaces;
#[cfg(test)]
mod navigation;
#[cfg(test)]
mod plugins;
#[cfg(test)]
mod search;
#[cfg(test)]
mod session;
#[cfg(test)]
mod support;

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use uze_application::UzeHome;
use uze_application::application::{
    AnchorKind, ContextState, MarketplaceState, OverviewMarketplace, OverviewWorkspaceSummary,
    ProjectEnvironmentState, ProjectOverview,
};
use uze_application::application::{DoctorReport, MaintenanceReport};

use super::hit::Hit;
use super::management::render;
use super::model::{
    Confirmation, Focus, Overlay, PREFERENCE_ROW_COUNT, ProfilePanel, ROUTES, RefreshData,
    Remembered, Route, Status, TuiModel,
};
use super::view::health::{Severity, actionable_alerts};
use super::worker::Intent;
use crate::ui::theme::{self, Token};
use crate::ui::widget::text;

use support::*;

/// The legend is the card's own vocabulary written out, so it names what
/// a card can wear and nothing else. It used to list three marks, one of
/// them for a state no card draws any more; the unmarked state is
/// described instead of listed, because a glyph beside it would name a
/// mark that is never on screen.
#[test]
fn the_harness_legend_names_the_words_a_card_carries() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.overlay = Overlay::HarnessHelp;
    let (terminal, _hits) = drawn_at(&model, 120, 40);
    let legend = buffer_rows(&terminal).join("\n");

    assert!(
        legend.contains("Enabled"),
        "the word a set-up card carries: {legend}"
    );
    assert!(
        legend.contains("Not configured"),
        "and the word the rest carry: {legend}"
    );
    for gone in ["Not installed", "Run uze setup"] {
        assert!(
            !legend.contains(gone),
            "{gone:?} is not a state a card has: {legend}"
        );
    }
}

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

/// A harness the operator never set up receives nothing, so nothing about
/// it is theirs to act on: neither its absence nor what a plugin's hook
/// would lose there.
#[test]
fn health_speaks_only_of_configured_harnesses() {
    use uze_application::CompatibilityRoute;
    use uze_application::application::HookHealth;
    let model = model_with_data();
    let mut doctor = model.remembered.doctor.clone().unwrap();
    let approximated_on = |harness: &str| HookHealth {
        hook: "one:guard".to_owned(),
        event: "pre_tool_use".to_owned(),
        harness: harness.to_owned(),
        route: CompatibilityRoute::Degraded,
        weakened: Some("cannot deny".to_owned()),
        delivery: None,
        artifact: None,
        state: None,
    };
    doctor.attachments[0].hooks = vec![approximated_on("claude-code"), approximated_on("codex")];

    let labels: Vec<_> = actionable_alerts(Some(&doctor))
        .into_iter()
        .map(|alert| alert.label)
        .collect();

    assert!(
        labels.contains(&"one hook one:guard is approximated on Claude Code".to_owned()),
        "{labels:?}"
    );
    assert!(
        !labels.iter().any(|label| label.contains("Codex")),
        "{labels:?}"
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

fn consumer_workspace(
    state: ProjectEnvironmentState,
    declared: usize,
    installed: usize,
    missing: &[&str],
    root: &std::path::Path,
) -> OverviewWorkspaceSummary {
    OverviewWorkspaceSummary {
        cwd: root.to_path_buf(),
        root: root.to_path_buf(),
        kind: AnchorKind::Consumer,
        agents_directory_present: true,
        project: ProjectOverview {
            drift: Default::default(),
            environment: state,
            context: ContextState::Ready,
            declared_plugins: declared,
            installed_plugins: installed,
            missing_plugins: missing.iter().map(ToString::to_string).collect(),
        },
        marketplace: None,
    }
}

fn marketplace_workspace(root: &std::path::Path) -> OverviewWorkspaceSummary {
    OverviewWorkspaceSummary {
        cwd: root.to_path_buf(),
        root: root.to_path_buf(),
        kind: AnchorKind::Marketplace,
        agents_directory_present: false,
        project: ProjectOverview {
            drift: Default::default(),
            environment: ProjectEnvironmentState::NotConfigured,
            context: ContextState::None,
            declared_plugins: 0,
            installed_plugins: 0,
            missing_plugins: Vec::new(),
        },
        marketplace: Some(OverviewMarketplace {
            name: Some("acme".to_owned()),
            package_count: 1,
            invalid_packages: 0,
            state: MarketplaceState::Valid,
        }),
    }
}

#[test]
fn installing_the_projects_environment_carries_the_workspace_root() {
    let root = std::path::PathBuf::from("/tmp/project");
    let mut model = TuiModel {
        route: Route::Overview,
        focus: Focus::Content,
        remembered: Remembered {
            workspace: Some(consumer_workspace(
                ProjectEnvironmentState::InstallRequired,
                4,
                3,
                &["flow"],
                &root,
            )),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    // Offered by the Overview's own card and by the index, with no letter
    // spent on it: `i` installs a *plugin*, and one letter names one
    // action.
    let intent = model.act(uze_keys::Action::InstallProjectEnvironment);
    assert_eq!(intent, Intent::InstallProjectEnvironment(root));
}

#[test]
fn installing_the_projects_environment_is_inert_when_it_is_ready() {
    let root = std::path::PathBuf::from("/tmp/project");
    let mut model = TuiModel {
        route: Route::Overview,
        focus: Focus::Content,
        remembered: Remembered {
            workspace: Some(consumer_workspace(
                ProjectEnvironmentState::Ready,
                2,
                2,
                &[],
                &root,
            )),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let intent = model.act(uze_keys::Action::InstallProjectEnvironment);
    assert_eq!(intent, Intent::None);
}

#[test]
fn overview_install_key_is_inert_outside_consumer_workspaces() {
    let root = std::path::PathBuf::from("/tmp/market");
    let mut model = TuiModel {
        route: Route::Overview,
        focus: Focus::Content,
        remembered: Remembered {
            workspace: Some(marketplace_workspace(&root)),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let intent = model.apply_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    assert_eq!(
        intent,
        Intent::None,
        "marketplace health must not offer `uze install`"
    );
}

#[test]
fn refreshed_updates_workspace_state() {
    let root = std::path::PathBuf::from("/tmp/project");
    let mut model = TuiModel {
        route: Route::Overview,
        ..TuiModel::default()
    };
    assert!(model.remembered.workspace.is_none());
    model.refreshed(RefreshData {
        workspace: Some(consumer_workspace(
            ProjectEnvironmentState::InstallRequired,
            4,
            3,
            &["flow"],
            &root,
        )),
        ..RefreshData::default()
    });
    assert_eq!(model.overview_install_path(), Some(root.clone()));

    model.refreshed(RefreshData {
        workspace: Some(consumer_workspace(
            ProjectEnvironmentState::Ready,
            4,
            4,
            &[],
            &root,
        )),
        ..RefreshData::default()
    });
    assert_eq!(
        model.overview_install_path(),
        None,
        "refresh must reflect a completed install"
    );
}

/// The header counts what is on the machine, not how many harnesses UZE
/// knows about. It counted the cards, so a machine carrying three of the
/// four UZE supports was told "4 installed" — and removing one changed
/// nothing, which is how the number was caught.
#[test]
fn the_catalog_counts_the_harnesses_that_are_actually_installed() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.focus = Focus::Content;
    let header = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(150, 40)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        buffer_rows(&terminal)
            .into_iter()
            .find(|row| row.contains("installed"))
            .expect("the screen header counts them")
    };

    assert!(
        header(&model).contains("2 installed"),
        "both fixtures are on the machine: {}",
        header(&model)
    );

    model
        .remembered
        .doctor
        .as_mut()
        .expect("the fixture has a report")
        .harnesses[1]
        .detection
        .present = false;

    assert!(
        header(&model).contains("1 installed"),
        "and one that is gone is not installed: {}",
        header(&model)
    );
}

/// Setting a harness up is offered for every card in the catalog, and it
/// is the same gesture wherever it starts from: UZE provisions through the
/// vendor's own official route, which installs what is not on the machine
/// and updates what is. The drawer used to offer the action only where it
/// had already been done, and nothing at all on the one card with
/// something to do.
#[test]
fn every_harness_in_the_catalog_can_be_set_up() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.focus = Focus::Content;
    // The fixture's second harness is detected and unconfigured.
    model.remembered.harness_screen.selected = 1;
    let drawn = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(150, 40)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        let offered = hits
            .iter()
            .any(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::SetupHarness));
        (offered, buffer_rows(&terminal).join("\n"))
    };

    let (offered, rows) = drawn(&model);
    assert!(offered, "a harness that is here can be set up: {rows}");

    model
        .remembered
        .doctor
        .as_mut()
        .expect("the fixture has a report")
        .harnesses[1]
        .detection
        .present = false;

    let (offered, rows) = drawn(&model);
    assert!(
        offered,
        "and so can one that is not — setting it up is what installs it: {rows}"
    );
    // Read down the drawer's own column and closed up, because the note
    // wraps: it is a sentence in a panel whose width is dragged, and a
    // row-by-row search for it finds it only at the widths it happens to
    // fit on one line.
    let drawer: String = rows
        .lines()
        .filter_map(|row| row.rsplit('\u{2502}').next())
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        drawer.contains("setting it up installs it"),
        "which is what the drawer says it will do: {rows}"
    );
}

/// A harness card says its state at its foot, beside its id, the way an
/// extension card does: "Enabled" for the one UZE set up, "Not configured"
/// for the rest. The title carries the name alone, so a narrowed card has
/// nothing to clip against it.
#[test]
fn a_harness_card_says_its_state_at_its_foot() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.focus = Focus::Content;
    // By each card's own hit rect, so this reads a card's rows rather than
    // the first mention of a name anywhere on the screen.
    let cards = |width: u16| {
        let (terminal, hits) = drawn_at(&model, width, 40);
        let rows = buffer_rows(&terminal);
        let row_of = |rect: Rect, offset: u16| -> String {
            rows[(rect.y + offset) as usize]
                .chars()
                .skip(rect.x as usize)
                .take(rect.width as usize)
                .collect()
        };
        let cards: Vec<(String, String)> = hits
            .iter()
            .filter_map(|(rect, hit)| matches!(hit, Hit::HarnessRow(_)).then_some(*rect))
            .map(|rect| (row_of(rect, 1), row_of(rect, 5)))
            .collect();
        assert!(!cards.is_empty(), "no harness card was drawn: {rows:?}");
        (cards, rows)
    };
    let card = |cards: &[(String, String)], name: &str| {
        cards
            .iter()
            .find(|(title, _)| title.contains(name))
            .unwrap_or_else(|| panic!("{name} has no card: {cards:?}"))
            .clone()
    };

    let (wide, rows) = cards(200);
    let (title, foot) = card(&wide, "Claude Code");
    assert_eq!(title.trim(), "Claude Code", "the title is the name alone");
    assert!(
        foot.contains("Enabled"),
        "the one UZE set up says so: {foot:?}"
    );
    // Pinned to the card's right edge, past its two columns of inset, so
    // every card in a row says it in the same column.
    assert!(
        foot.trim_end_matches(' ').ends_with("Enabled") && foot.ends_with("Enabled  "),
        "the state sits at the right edge: {foot:?}"
    );
    let (title, foot) = card(&wide, "Codex");
    assert_eq!(title.trim(), "Codex");
    assert!(
        foot.contains("Not configured"),
        "and the one it did not says that: {foot:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("PATH")),
        "least of all a sentence about this machine's PATH: {rows:?}"
    );

    let (narrow, _) = cards(120);
    let (title, foot) = card(&narrow, "Claude Code");
    assert_eq!(title.trim(), "Claude Code");
    assert!(
        foot.contains("Enabled"),
        "a narrowed card keeps it: {foot:?}"
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

#[test]
fn overview_does_not_render_project_context() {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let model = TuiModel {
        route: Route::Overview,
        focus: Focus::Content,
        remembered: Remembered {
            workspace: Some(consumer_workspace(
                ProjectEnvironmentState::InstallRequired,
                4,
                3,
                &["flow"],
                std::path::Path::new("/tmp/project"),
            )),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    let rows = buffer_rows(&terminal);
    for forbidden in [
        "PROJECT",
        "MARKETPLACE",
        "Environment",
        "Memory",
        "Context bridges",
        "context bridges verified",
    ] {
        assert!(
            !rows.iter().any(|row| row.contains(forbidden)),
            "Overview must not render project context: {forbidden}"
        );
    }
}

#[test]
fn overview_render_does_not_mutate_project_state() {
    let base = uze_testkit::temp::scratch("ui-overview-immutable");
    let root = base.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let lock_path = root.join("agents.lock");
    let lock_bytes = b"version: 1\nplugins: {}\n";
    std::fs::write(&lock_path, lock_bytes).unwrap();
    let manifest_bytes = br#"{"name":"m","plugins":[]}"#;
    std::fs::write(root.join("marketplace.json"), manifest_bytes).unwrap();
    let agents_md = b"# hi\n";
    std::fs::write(root.join("AGENTS.md"), agents_md).unwrap();

    let model = TuiModel {
        route: Route::Overview,
        context_root: root.clone(),
        remembered: Remembered {
            workspace: Some(consumer_workspace(
                ProjectEnvironmentState::Ready,
                2,
                2,
                &[],
                &root,
            )),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let (terminal, _hits) = drawn_at(&model, 100, 40);
    let rows = buffer_rows(&terminal);

    // The render must leave the workspace exactly as found.
    assert_eq!(std::fs::read(&lock_path).unwrap(), lock_bytes);
    assert_eq!(
        std::fs::read(root.join("marketplace.json")).unwrap(),
        manifest_bytes
    );
    assert_eq!(std::fs::read(root.join("AGENTS.md")).unwrap(), agents_md);
    // The machine dashboard still renders while leaving the project untouched.
    assert!(rows.iter().any(|row| row.contains("Overview")));
    assert!(rows.iter().any(|row| row.contains("Harnesses detected")));
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn no_workspace_render_creates_nothing() {
    let base = uze_testkit::temp::scratch("ui-noworkspace");
    let root = base.join("random");
    std::fs::create_dir_all(&root).unwrap();

    let model = TuiModel {
        route: Route::Overview,
        context_root: root.clone(),
        remembered: Remembered {
            workspace: Some(OverviewWorkspaceSummary {
                cwd: root.clone(),
                root: root.clone(),
                kind: AnchorKind::NoWorkspace,
                agents_directory_present: false,
                project: ProjectOverview {
                    drift: Default::default(),
                    environment: ProjectEnvironmentState::NotConfigured,
                    context: ContextState::None,
                    declared_plugins: 0,
                    installed_plugins: 0,
                    missing_plugins: Vec::new(),
                },
                marketplace: None,
            }),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let (terminal, _hits) = drawn_at(&model, 100, 40);
    let rows = buffer_rows(&terminal);
    assert!(rows.iter().any(|row| row.contains("Overview")));
    assert!(!rows.iter().any(|row| row.contains("PROJECT")));
    assert!(!rows.iter().any(|row| row.contains("MARKETPLACE")));

    assert!(
        !root.join("agents.lock").exists(),
        "rendering must never create a project lock"
    );
    assert!(
        !root.join("marketplace.json").exists(),
        "rendering must never create a marketplace manifest"
    );
    let entries: Vec<_> = std::fs::read_dir(&root).unwrap().collect();
    assert!(
        entries.is_empty(),
        "a NoWorkspace render must leave the directory untouched"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// The real `git` on the ambient PATH, for a test that isolates PATH but
/// still needs to clone a marketplace.
fn which_git() -> std::path::PathBuf {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .flat_map(|directory| uze_platform::executable::candidates(&directory, "git"))
        .find(|candidate| candidate.is_file())
        .expect("git must be on PATH for this test")
}

#[test]
fn overview_install_intent_reaches_install_project_environment() {
    use std::sync::mpsc;
    use std::time::Duration;

    // `dispatch` builds its application through
    // `UzeApplication::from_env_with_runner`, whose integrations read
    // process-global environment. Use the testkit-wide guard so concurrent
    // tests which need a real executable on PATH cannot observe this setup.
    let mut environment = uze_testkit::env::scope();

    let base = uze_testkit::temp::scratch("ui-install-dispatch");
    let home = base.join("home");
    let project = base.join("project");
    let market = base.join("market");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::create_dir_all(market.join("flow/skills/uze-test")).unwrap();
    std::fs::write(
        market.join("marketplace.json"),
        r#"{"name":"test","plugins":[{"name":"flow","source":"flow"}]}"#,
    )
    .unwrap();
    std::fs::write(market.join("flow/plugin.json"), r#"{"name":"flow"}"#).unwrap();
    std::fs::write(market.join("flow/skills/uze-test/SKILL.md"), "# s\n").unwrap();
    let revision = uze_testkit::git::commit_everything_in(&market);
    let lock = uze_core::project_lock::ProjectLock {
        marketplaces: std::iter::once((
            "test".to_owned(),
            uze_core::project_lock::LockedMarketplace {
                git: market.display().to_string(),
                r#ref: None,
                subdirectory: None,
                revision,
            },
        ))
        .collect(),
        plugins: std::iter::once((
            "flow".to_owned(),
            uze_core::project_lock::LockedPlugin {
                marketplace: "test".to_owned(),
                integrity: None,
            },
        ))
        .collect(),
        ..Default::default()
    };
    uze_core::project_lock::save_lock(&project, &lock).unwrap();

    environment.home(&base);
    environment.set("UZE_HOME", &home);
    // Isolate PATH to a directory with nothing on it: on a machine
    // where `uze setup claude` has ever actually run, the real
    // `~/.uze/shims/claude` sits ahead of everything else on the
    // ambient PATH this test process inherited. That shim resolves to
    // this very `uze` binary (not a vendor CLI), and it is excluded
    // from `resolve_real_executable`'s walk only by comparing against
    // *this test's* fake `shims_dir` — never the developer's real one.
    // Left unisolated, the install path below shells out to `uze`
    // itself expecting Claude Code's CLI and gets `uze`'s own `--help`
    // usage back. Every harness must read as absent here, matching a
    // clean machine.
    // Git alone, since a marketplace is a Git repository and installing
    // from one clones it. Everything else must read as absent.
    let empty_path_dir = base.join("empty-path");
    std::fs::create_dir_all(&empty_path_dir).unwrap();
    let git = which_git();
    uze_platform::fs::symlink(
        &git,
        &empty_path_dir.join(uze_platform::executable::file_name("git")),
    )
    .unwrap();
    environment.set("PATH", &empty_path_dir);

    let uze_home = UzeHome::at(&home);
    let mut model = TuiModel {
        route: Route::Overview,
        context_root: project.clone(),
        remembered: Remembered {
            workspace: Some(consumer_workspace(
                ProjectEnvironmentState::InstallRequired,
                1,
                0,
                &["flow"],
                &project,
            )),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let (sender, receiver) = mpsc::channel();
    super::worker::dispatch(
        Intent::InstallProjectEnvironment(project),
        &uze_home,
        &sender,
        &mut model,
    );
    let result = receiver.recv_timeout(Duration::from_secs(30)).unwrap();
    match result {
        super::worker::WorkerResult::Mutated(_, Ok((message, data))) => {
            assert!(
                message.contains("Installed"),
                "install must report success, got {message}"
            );
            let workspace = data.workspace.expect("refresh carries workspace state");
            let project = &workspace.project;
            assert_eq!(
                project.environment,
                ProjectEnvironmentState::Ready,
                "after install the Application must report Ready"
            );
            assert_eq!(
                (project.declared_plugins, project.installed_plugins),
                (1, 1)
            );
            assert!(project.missing_plugins.is_empty());
        }
        super::worker::WorkerResult::Mutated(_, Err(error)) => {
            panic!("expected Mutated(Ok(..)), got Mutated(Err({error}))")
        }
        super::worker::WorkerResult::TrustRequired { plugin, detail, .. } => {
            panic!(
                "expected Mutated(Ok(..)), got TrustRequired {{ plugin: {plugin}, detail: {detail} }}"
            )
        }
        _ => panic!("expected Mutated(Ok(..)), got a different WorkerResult variant"),
    }

    std::fs::remove_dir_all(&base).ok();
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

/// Health is said in one place, beside the version, on every screen of
/// the modal — and a click on it opens what it is about.
#[test]
fn the_footer_says_the_machines_health_and_opens_what_needs_attention() {
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        overlay: Overlay::None,
        ..model_with_data()
    };
    if let Some(doctor) = model.remembered.doctor.as_mut() {
        doctor.ledger_error = Some("the ledger is not JSON".to_owned());
    }
    let draw = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(150, 26)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        (buffer_rows(&terminal), hits)
    };
    let (rows, hits) = draw(&model);
    let footer = rows.last().expect("a footer row");
    let version = format!("v{}", crate::self_update::running());
    assert!(footer.contains("need attention"), "{footer}");
    assert!(
        footer.find("need attention") < footer.find(&version),
        "the status sits beside the version, before it: {footer}"
    );

    let (rect, _) = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::HealthStatus)
        .expect("the status answers a click");
    model.hits = hits.clone();
    model.click(rect.x, rect.y);
    assert_eq!(model.overlay, Overlay::Health);
    let (rows, _) = draw(&model);
    assert!(
        rows.iter()
            .any(|row| row.contains("Attachment ledger unreadable")),
        "{}",
        rows.join("\n")
    );

    // Nothing read yet is nothing said, rather than a "healthy" nobody
    // checked.
    model.overlay = Overlay::None;
    model.remembered.doctor = None;
    let (rows, hits) = draw(&model);
    assert!(!rows.last().unwrap().contains("healthy"));
    assert!(!hits.iter().any(|(_, hit)| *hit == Hit::HealthStatus));
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
