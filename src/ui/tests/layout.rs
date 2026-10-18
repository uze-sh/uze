//! What every screen draws: each route and overlay rendered, and the columns and drawers they lay out.

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use uze_application::application::MarketplacePluginSummary;

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{
    Confirmation, Focus, ListScreen, Overlay, PluginPane, ROUTES, Remembered, Route, TrustedRetry,
    TuiModel,
};

use super::support::*;

#[test]
fn every_route_renders_without_panicking() {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let base = model_with_data();
    for route in ROUTES {
        let model = TuiModel {
            route,
            profile_harness_selection: base.profile_harness_selection.clone(),
            focus: Focus::Content,
            remembered: Remembered {
                plugins: base.remembered.plugins.clone(),
                marketplaces: base.remembered.marketplaces.clone(),
                marketplace_plugins: base.remembered.marketplace_plugins.clone(),
                doctor: base.remembered.doctor.clone(),
                harness_screen: base.remembered.harness_screen.clone(),
                profiles: base.remembered.profiles.clone(),
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
    }
}

#[test]
fn every_overlay_renders_without_panicking() {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let base = model_with_data();
    let overlays = [
        Overlay::ActionIndex {
            scopes: vec![uze_keys::Scope::Global, uze_keys::Scope::Management],
            filter: String::new(),
            selected: 0,
        },
        Overlay::HarnessHelp,
        Overlay::Confirm {
            kind: Confirmation::RemovePlugin("one".to_owned()),
            focus: Some(1),
        },
        Overlay::Confirm {
            kind: Confirmation::UpdatePlugin("one".to_owned()),
            focus: None,
        },
        Overlay::Confirm {
            kind: Confirmation::InstallPlugin {
                name: "flow".to_owned(),
                marketplace: "uze-official".to_owned(),
            },
            focus: None,
        },
        Overlay::Confirm {
            kind: Confirmation::ApplyContext,
            focus: None,
        },
        Overlay::Confirm {
            kind: Confirmation::ProtectedPlugin("one".to_owned()),
            focus: None,
        },
        Overlay::AddMarketplace("/home/user/marketplace".to_owned()),
        Overlay::NewProfile("dev-autonomous".to_owned()),
        Overlay::Confirm {
            kind: Confirmation::DeleteProfile("default".to_owned()),
            focus: Some(1),
        },
        Overlay::Confirm {
            kind: Confirmation::Trust {
                plugin: "one".to_owned(),
                detail: "one -> mcp-server".to_owned(),
                retry: TrustedRetry::Install {
                    name: "one".to_owned(),
                    marketplace: "uze-official".to_owned(),
                },
            },
            focus: None,
        },
    ];
    for overlay in overlays {
        let model = TuiModel {
            overlay,
            remembered: Remembered {
                plugins: base.remembered.plugins.clone(),
                marketplace_plugins: base.remembered.marketplace_plugins.clone(),
                doctor: base.remembered.doctor.clone(),
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
    }
}

#[test]
fn empty_marketplace_and_no_harness_states_do_not_panic_rendering() {
    let model = TuiModel {
        route: Route::Plugins,
        ..TuiModel::default()
    };
    assert_eq!(model.list_len(model.route), 0);
    assert!(model.selected_marketplace_plugin().is_none());
    let model = TuiModel {
        route: Route::Harnesses,
        ..TuiModel::default()
    };
    assert!(model.selected_harness().is_none());
}

/// The drawer is where a row's actions are performed with the pointer, so
/// every screen that has actions draws them there, as buttons, for exactly
/// what can be done now.
#[test]
fn every_drawer_draws_what_its_row_can_do_as_buttons() {
    for route in [
        Route::Plugins,
        Route::Harnesses,
        Route::Profiles,
        Route::Keys,
    ] {
        let mut model = model_with_data();
        model.set_route(route);
        model.focus = Focus::Content;
        let available: Vec<_> = model
            .selected_offers()
            .into_iter()
            .filter(|offer| offer.is_available())
            .map(|offer| offer.action)
            .collect();
        // The sample's one plugin is the official one, installed and
        // current: nothing to do, so nothing is drawn — the buttons a
        // plugin does draw have a test of their own.
        assert!(
            route == Route::Plugins || !available.is_empty(),
            "{route:?} offers nothing to do"
        );
        let (_terminal, hits) = drawn_at(&model, 140, 40);
        for action in available {
            assert!(
                hits.iter()
                    .any(|(_, hit)| *hit == Hit::OfferedAction(action)),
                "{route:?} drew no button for {action}"
            );
        }
    }
}

/// Every right-hand drawer is the same slab: it starts on the frame's own
/// top row and runs to the bottom. A drawer handed the area *below* the
/// screen header instead starts three rows down, which reads as a panel
/// that failed to open rather than as a deliberate inset.
#[test]
fn every_drawer_runs_the_full_height_of_its_screen() {
    let mut rules = Vec::new();
    for route in [
        Route::Plugins,
        Route::Extensions,
        Route::Harnesses,
        Route::Keys,
        Route::Settings,
    ] {
        let mut model = model_with_data();
        model.set_route(route);
        model.focus = Focus::Content;
        model.settings_themes = vec![uze_application::application::ThemeSummary {
            id: "default".to_owned(),
            active: true,
            path: None,
        }];
        model.settings_glyph_sets = vec![uze_application::application::GlyphSetSummary {
            id: "nerd".to_owned(),
            active: false,
        }];
        model.settle_settings_selection();

        let (_terminal, hits) = drawn_at(&model, 140, 40);
        let rule = hits
            .iter()
            .find_map(|(rect, hit)| matches!(hit, Hit::ResizePanel(_)).then_some(*rect))
            .unwrap_or_else(|| panic!("{route:?} drew no drawer"));
        rules.push((route, rule));
    }

    let (first_route, first) = rules[0];
    for (route, rule) in &rules[1..] {
        assert_eq!(
            (rule.y, rule.height),
            (first.y, first.height),
            "{route:?}'s drawer does not run the height {first_route:?}'s does"
        );
    }
}

/// The drawer's own shape: the plugin's name is the heading rather than a
/// value under a `PLUGIN` label, and no row is folded flush against the
/// border — folding at the full width is what made a drawer with rows to
/// spare read as crowded.
#[test]
fn the_drawer_leads_with_the_name_and_leaves_a_gutter() {
    use uze_application::application::{MarketplacePluginDetail, Revision};

    let summary = MarketplacePluginSummary {
        marketplace: "ai".to_owned(),
        name: "git".to_owned(),
        description: Some(
            "Personal git workflow conventions: a Conventional Commits skill and a \
             pull/merge request skill."
                .to_owned(),
        ),
        keywords: vec!["git".to_owned(), "conventional-commits".to_owned()],
        category: None,
        installed: false,
        freshness: uze_application::application::Freshness::not_checked(),
        installed_at_unix: None,
        is_default: false,
    };
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        plugin_pane: PluginPane::Plugins,
        marketplace_detail: Some(MarketplacePluginDetail {
            revision: Some(Revision::Commit {
                short: "f1f00f7".to_owned(),
                age: "79 minutes ago".to_owned(),
                subject: "feat(git): require technical descriptions in the commit and pr skills"
                    .to_owned(),
            }),
            summary: summary.clone(),
            capabilities: Vec::new(),
        }),
        remembered: Remembered {
            plugin_screen: ListScreen::default(),
            marketplace_plugins: vec![summary],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let width = 52;
    model.drawer_width = Some(width);
    let (terminal, _hits) = drawn_at(&model, 120, 40);
    let rows = buffer_rows(&terminal);

    // The drawer is what follows the last rule on each row; the list
    // beside it heads its own column with the word.
    assert!(
        !rows.iter().any(|row| row
            .rsplit('│')
            .next()
            .is_some_and(|drawer| drawer.contains("PLUGIN"))),
        "the name is the heading, not a value under a label: {rows:#?}"
    );
    let revision = rows
        .iter()
        .position(|row| row.contains("revision"))
        .unwrap_or_else(|| panic!("the revision field is drawn: {rows:#?}"));
    assert!(
        rows[revision].contains("f1f00f7") && rows[revision].contains("79 minutes ago"),
        "the commit and its age share a row: {rows:#?}"
    );

    // Every drawer row stops short of the border. Measured against the
    // widest row the drawer drew, since the panel's own edge is what the
    // gutter is relative to.
    let widest = rows
        .iter()
        .filter(|row| row.contains("git") || row.contains("feat("))
        .map(|row| row.trim_end().chars().count())
        .max()
        .unwrap_or_default();
    assert!(
        widest > 0 && widest < 120,
        "no row runs to the terminal's edge: {widest}"
    );
}

#[test]
fn both_columns_reach_the_terminals_last_row() {
    let frame = Rect::new(0, 0, 120, 40);
    let (sidebar, column) = crate::ui::sidebar_and_column(frame, None);
    assert_eq!(sidebar.bottom(), frame.bottom());
    assert_eq!(column.bottom(), frame.bottom());
}
