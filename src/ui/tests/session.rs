//! What one visit leaves for the next: remembered screens and resolutions across visits and runs.

use ratatui::layout::Rect;

use crate::ui::hit::Hit;
use crate::ui::model::{Confirmation, Focus, Overlay, Route, Status, TuiModel};
use crate::ui::worker::Intent;

use super::support::*;

#[test]
fn a_return_visit_draws_what_the_last_one_resolved() {
    let mut model = model_with_plugins(&["one", "two"]);
    model.remembered.resolved_at = Some(std::time::Instant::now());
    model.remembered.plugin_screen.selected = 1;
    model.drawer_width = Some(46);
    // What one visit ends holding — including work it was in the middle
    // of, which the next visit must not inherit.
    model.status = Status::Working("Inspecting one…".to_owned());
    model.overlay = Overlay::Confirm {
        kind: Confirmation::RemovePlugin("one".to_owned()),
        focus: Some(0),
    };
    model.maintenance_in_flight = true;
    model.inspection_in_flight = Some(Intent::InspectPlugin("one".to_owned()));
    model.hits = vec![(Rect::new(0, 0, 1, 1), Hit::Route(Route::Plugins))];

    let layout = model.management_layout();
    let model = TuiModel::recall(Some(model.remember()), &layout);

    assert_eq!(
        model.remembered.plugins.len(),
        2,
        "the machine state the last visit resolved is still the truth about the machine"
    );
    assert!(
        model.remembered.resolved_at.is_some(),
        "and so is when it was resolved — the next visit decides on it"
    );
    assert_eq!(model.route, Route::Plugins);
    assert_eq!(model.remembered.plugin_screen.selected, 1);
    assert_eq!(
        model.drawer_width,
        Some(46),
        "a drawer stays the width it was dragged to"
    );
    assert!(matches!(model.status, Status::Idle));
    assert!(matches!(model.overlay, Overlay::None));
    assert_eq!(model.focus, Focus::Sidebar);
    assert!(
        !model.maintenance_in_flight && model.inspection_in_flight.is_none(),
        "work in flight belonged to a visit that ended, and its channel with it"
    );
    assert!(model.hits.is_empty());
}

#[test]
fn a_resolution_the_session_just_made_is_not_asked_for_again() {
    use crate::ui::management::{RESOLUTION_STANDS_FOR, opening_re_resolves};
    use std::time::Instant;

    assert!(
        opening_re_resolves(None),
        "nothing resolved yet is not an answer to stand on"
    );
    assert!(
        !opening_re_resolves(Some(Instant::now())),
        "the session's own warm-up answered a moment ago; opening the screen shows it"
    );
    assert!(
        opening_re_resolves(Some(Instant::now() - RESOLUTION_STANDS_FOR)),
        "past the window, opening the screen is a claim about the machine now"
    );
}

#[test]
fn a_first_visit_starts_from_the_default_model() {
    let model = TuiModel::recall(None, &uze_application::ManagementLayout::default());
    assert!(model.remembered.plugins.is_empty());
    assert_eq!(model.route, Route::Overview);
    assert!(
        model.drawer_width.is_none(),
        "the width a drawer opens with is stated once, by Default"
    );
}

/// Which screen was open, and how its drawers were left, outlive the
/// process: the next run opens where the last one was, not on Overview.
#[test]
fn the_next_run_opens_on_the_screen_the_last_one_left() {
    let mut model = TuiModel::default();
    model.set_route(Route::Profiles);
    model.drawer_width = Some(38);
    model.profile_columns_width = Some(28);
    model.plugin_market = Some("uze-official".to_owned());

    let layout = model.management_layout();
    assert_eq!(layout.route.as_deref(), Some("profiles"));

    let model = TuiModel::recall(None, &layout);
    assert_eq!(model.route, Route::Profiles);
    assert_eq!(
        model.drawer_width,
        Some(38),
        "a drawer stays the width it was dragged to"
    );
    assert_eq!(model.profile_columns_width, Some(28));
    assert_eq!(model.plugin_market.as_deref(), Some("uze-official"));

    let unknown = uze_application::ManagementLayout {
        route: Some("a screen this build does not have".to_owned()),
        ..uze_application::ManagementLayout::default()
    };
    assert_eq!(
        TuiModel::recall(None, &unknown).route,
        Route::Overview,
        "a screen the client no longer recognizes opens the default, not nothing"
    );
}

/// The management client reopens on the screen it was left on without
/// passing through a route change — and Settings, which reads its lists
/// on arrival, used to open empty until it was clicked again.
#[test]
fn settings_reopened_where_it_was_left_still_reads_its_lists() {
    let layout = uze_application::ManagementLayout {
        route: Some(Route::Settings.id().to_owned()),
        ..uze_application::ManagementLayout::default()
    };
    let mut model = TuiModel::recall(None, &layout);
    assert_eq!(model.route, Route::Settings);
    assert_eq!(model.settings_intent(), Intent::LoadSettings);
    model.settings_read = true;
    assert_eq!(
        model.settings_intent(),
        Intent::None,
        "read once, not every frame — an empty machine included"
    );
    model.set_route(Route::Overview);
    assert_eq!(
        model.settings_intent(),
        Intent::None,
        "only that screen reads them"
    );
}
