//! Moving between screens and focus: the sidebar, the tabs, the routes a build offers, and the first steps.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use uze_application::application::MarketplacePluginSummary;

use crate::ui::hit::Hit;
use crate::ui::model::{Confirmation, Focus, Overlay, ROUTES, Route, TuiModel};
use crate::ui::worker::Intent;

use super::support::*;

#[test]
fn sidebar_keyboard_navigation_cycles_routes() {
    let mut model = TuiModel {
        focus: Focus::Sidebar,
        ..TuiModel::default()
    };
    // Against the sidebar's own order rather than against named routes:
    // what this proves is that the keys walk it and turn around, which is
    // still true the next time the order is argued over.
    assert_eq!(model.route, ROUTES[0]);
    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(model.route, ROUTES[1]);
    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(model.route, ROUTES[2]);
    model.apply_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(model.route, ROUTES[1]);
}

#[test]
fn tab_toggles_focus_between_sidebar_and_content() {
    let mut model = TuiModel {
        route: Route::Plugins,
        ..TuiModel::default()
    };
    assert_eq!(model.focus, Focus::Sidebar);
    model.apply_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(model.focus, Focus::Content);
    model.apply_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(model.focus, Focus::Sidebar);
}

/// Down walks the tree past the group's heading onto the next plugin, the
/// panel asks for that plugin's detail, and Enter performs what the panel
/// offers first — asked about before it happens.
#[test]
fn content_navigation_and_inspect_intent() {
    let mut model = model_with_plugins(&["one", "two"]);
    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(model.remembered.plugin_screen.selected, 1);
    assert_eq!(
        model.marketplace_inspect_intent(),
        Intent::InspectPlugin("two".to_owned())
    );
    assert_eq!(
        model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Intent::None
    );
    assert!(
        matches!(model.overlay, Overlay::Confirm { .. }),
        "an installed plugin's first offer is removing it, confirmed first"
    );
}

#[test]
fn a_route_action_key_works_from_the_sidebar_too() {
    let mut model = model_with_plugins(&["one"]);
    model.remembered.plugins[0].freshness = behind();
    model.focus = Focus::Sidebar;
    model.apply_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
    assert!(
        matches!(model.overlay, Overlay::Confirm { kind: Confirmation::UpdatePlugin(ref id), .. } if id == "one"),
        "`u` must not be swallowed just because the sidebar holds focus"
    );
}

#[test]
fn mouse_click_on_sidebar_route_switches_route_and_focus() {
    let mut model = TuiModel {
        hits: vec![(Rect::new(0, 1, 20, 1), Hit::Route(Route::Plugins))],
        ..TuiModel::default()
    };
    let intent = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 100, 40),
    );
    assert_eq!(intent, Intent::None);
    assert_eq!(model.route, Route::Plugins);
    assert_eq!(model.focus, Focus::Content);
}

#[test]
fn read_only_navigation_never_produces_a_mutating_intent() {
    let mut model = model_with_plugins(&["one", "two"]);
    model.set_route(Route::Plugins);
    model.remembered.marketplace_plugins = vec![MarketplacePluginSummary {
        marketplace: "uze".to_owned(),
        name: "uze".to_owned(),
        description: None,
        keywords: Vec::new(),
        category: None,
        installed: true,
        freshness: up_to_date(),
        installed_at_unix: None,
        is_default: true,
    }];
    for key in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::Char('j'),
        KeyCode::Char('k'),
    ] {
        let intent = model.apply_key(KeyEvent::new(key, KeyModifiers::NONE));
        // Plugins navigation may dispatch a read-only inspect fetch
        // (keeps the drawer's revision and the row's resources populated as
        // selection moves) — that's not a mutation, so only reject the
        // intents that actually write something.
        assert!(
            matches!(
                intent,
                Intent::None | Intent::InspectMarketplacePlugin { .. } | Intent::InspectPlugin(..)
            ),
            "navigation must never produce a mutating intent, got {intent:?}"
        );
    }
}

/// A step is recorded wherever it was performed from — the key, a button,
/// the index — because every action this client performs goes through one
/// place, and that is where the list learns.
#[test]
fn taking_a_step_any_way_at_all_marks_it_taken() {
    let mut model = model_with_plugins(&["flow"]);
    assert!(model.steps_taken.is_empty());
    model.act(uze_keys::Action::OpenActionIndex);
    assert!(
        model
            .steps_taken
            .contains(&uze_keys::Action::OpenActionIndex.name())
    );

    // And an action that is not a step leaves the list alone.
    let before = model.steps_taken.clone();
    model.act(uze_keys::Action::StartFilter);
    assert_eq!(model.steps_taken, before);
}

/// The property the list needs and nothing was checking: a step must be
/// takeable from wherever the list is drawn, which is every screen.
///
/// Two steps were screen-specific — asking a row what can be done to it,
/// and searching a list — so on the screen uze opens on they were rows
/// that did nothing when clicked, in a checklist that could never be
/// finished from there.
#[test]
fn every_first_step_can_be_taken_from_every_screen() {
    for route in ROUTES {
        for action in crate::ui::management::FIRST_STEPS {
            let mut model = TuiModel {
                route,
                focus: Focus::Content,
                ..model_with_data()
            };
            model.act(action);
            assert!(
                model.steps_taken.contains(&action.name()),
                "{action} did not land on {route:?} — a step the list offers \
                 everywhere has to be takeable everywhere"
            );
        }
    }
}

#[test]
fn no_tab_carries_a_count() {
    let model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..model_with_data()
    };
    let (terminal, hits) = drawn_at(&model, 190, 40);
    let header = buffer_rows(&terminal)
        .into_iter()
        .next()
        .unwrap_or_default();
    for (rect, hit) in &hits {
        let Hit::Route(route) = hit else { continue };
        let tab: String = header
            .chars()
            .skip(rect.x as usize)
            .take(rect.width as usize)
            .collect();
        let after_label = tab.split(route.label()).nth(1).unwrap_or_default();
        assert!(
            !after_label.chars().any(|glyph| glyph.is_ascii_digit()),
            "no count beside {route:?}: {tab:?}"
        );
    }
}

/// A screen is chosen by the number on its tab, from wherever the focus
/// was, and the keyboard lands in it — choosing one is wanting to be on it.
#[test]
fn a_number_chooses_the_screen_from_wherever_you_are() {
    for focus in [Focus::Content, Focus::Sidebar] {
        let mut model = TuiModel {
            route: Route::Overview,
            focus,
            ..model_with_data()
        };
        let plugins = Route::Plugins.index() + 1;
        let digit = char::from_digit(plugins as u32, 10).expect("one digit");
        model.apply_key(KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE));
        assert_eq!(model.route, Route::Plugins);
        assert_eq!(
            model.focus,
            Focus::Content,
            "and the keyboard is in the screen, not on its name"
        );
    }
}
