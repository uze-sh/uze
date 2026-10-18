//! Searching and filtering a screen's list.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use uze_application::application::MarketplacePluginSummary;

use crate::ui::model::{Focus, ListScreen, PluginPane, Remembered, Route, Status, TuiModel};

use super::support::*;

#[test]
fn marketplace_filter_narrows_visible_selection() {
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        remembered: Remembered {
            marketplace_plugins: vec![
                marketplace_plugin("ai", "std", false),
                marketplace_plugin("ai", "flow", true),
            ],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    assert_eq!(model.marketplace_visible_indices(), vec![0, 1]);

    model.apply_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert!(model.filtering);
    for c in "flow".chars() {
        model.apply_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    assert_eq!(model.marketplace_visible_indices(), vec![1]);
    assert_eq!(model.selected_marketplace_plugin().unwrap().name, "flow");

    model.apply_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!model.filtering);
    assert!(model.remembered.plugin_screen.filter.is_empty());
    assert_eq!(model.marketplace_visible_indices(), vec![0, 1]);
}

#[test]
fn extension_filter_narrows_visible_selection() {
    use uze_extensions::registry::BuiltinExtension;

    let mut model = TuiModel {
        route: Route::Extensions,
        focus: Focus::Content,
        extensions: vec![
            BuiltinExtension {
                id: "git",
                name: "Git",
                description: "Review the working tree",
                surface: "Workspace TUI",
                usage: "Open from the tab strip",
            },
            BuiltinExtension {
                id: "task-list",
                name: "Task List",
                description: "Track workspace tasks",
                surface: "Management TUI",
                usage: "Open from the sidebar",
            },
        ],
        ..TuiModel::default()
    };

    model.apply_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    for c in "task".chars() {
        model.apply_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    assert_eq!(model.extension_visible_indices(), vec![1]);
    assert_eq!(model.selected_extension().unwrap().name, "Task List");

    model.apply_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(model.remembered.extension_screen.filter.is_empty());
    assert_eq!(model.extension_visible_indices(), vec![0, 1]);
}

/// An action that belongs to a screen with a list still answers on one
/// without, rather than doing nothing — which is what a broken key looks
/// like — and is never recorded as something that happened.
#[test]
fn a_key_with_nothing_to_act_on_here_says_so() {
    let mut model = TuiModel {
        route: Route::Overview,
        focus: Focus::Content,
        ..model_with_data()
    };

    model.act(uze_keys::Action::StartFilter);
    assert!(!model.filtering, "the Overview has nothing to search");
    assert!(
        matches!(&model.status, Status::Success(said) if said.contains("search")),
        "and the key says so: {:?}",
        model.status
    );
    assert!(
        model.steps_taken.is_empty(),
        "it is not a first step, and nothing was recorded either way"
    );

    // On a screen that has them, both land.
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        ..model_with_data()
    };
    model.act(uze_keys::Action::StartFilter);
    assert!(model.filtering);
}

/// The Keys screen draws a search field and answers clicks on it, but `/`
/// did not reach it: the one screen whose whole subject is keys had a key
/// that did nothing.
#[test]
fn the_keys_screen_is_searchable_by_its_own_key() {
    let mut model = TuiModel {
        route: Route::Keys,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    model.act(uze_keys::Action::StartFilter);
    assert!(model.filtering);
    for character in "quit".chars() {
        model.type_character(character);
    }
    assert_eq!(model.key_screen.filter, "quit");
    assert!(
        !model.key_rows().is_empty(),
        "and the list narrowed to something"
    );
    assert!(
        model.key_rows().len() < TuiModel::default().key_rows().len(),
        "narrower than the whole keyboard"
    );
}

/// The search field is drawn on three screens and, until this, clicking it
/// did nothing at all — it was rendered without a hit of its own.
#[test]
fn clicking_the_search_field_starts_a_search() {
    for route in [Route::Plugins, Route::Extensions, Route::Harnesses] {
        let mut model = model_with_data();
        model.set_route(route);
        let (_terminal, hits) = drawn_at(&model, 100, 40);
        model.hits = hits;
        let (rect, _) = model
            .hits
            .iter()
            .find(|(_, hit)| *hit == crate::ui::hit::Hit::FocusFilter)
            .unwrap_or_else(|| panic!("{route:?} draws a search field nobody can click"))
            .clone();
        model.click(rect.x + 1, rect.y);
        assert!(model.filtering, "{route:?}");
    }
}

/// The category is what a catalogue is browsed by: the drawer says it,
/// and the filter finds a plugin by it as it does by a keyword.
#[test]
fn a_plugin_reads_and_filters_by_its_category() {
    use uze_application::application::MarketplacePluginDetail;

    let summary = MarketplacePluginSummary {
        category: Some("productivity".to_owned()),
        ..marketplace_plugin("ai", "notes", false)
    };
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        plugin_pane: PluginPane::Plugins,
        marketplace_detail: Some(MarketplacePluginDetail {
            revision: None,
            summary: summary.clone(),
            capabilities: Vec::new(),
        }),
        remembered: Remembered {
            plugin_screen: ListScreen::default(),
            marketplace_plugins: vec![summary, marketplace_plugin("ai", "other", false)],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let (terminal, _hits) = drawn_at(&model, 120, 40);
    let rows = buffer_rows(&terminal);
    assert!(
        rows.iter()
            .any(|row| row.contains("category") && row.contains("productivity")),
        "the drawer names the category: {rows:#?}"
    );

    model.remembered.plugin_screen.filter = "product".to_owned();
    let names: Vec<String> = model
        .marketplace_visible_indices()
        .into_iter()
        .map(|index| model.marketplace_rows()[index].name.clone())
        .collect();
    assert_eq!(names, ["notes"]);
}
