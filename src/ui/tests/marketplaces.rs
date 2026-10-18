//! The marketplace groups of the plugin tree: walking them, adding and removing one, and its source link.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use uze_application::application::{MarketplacePluginSummary, MarketplaceSummary};

use crate::ui::hit::Hit;
use crate::ui::model::{Confirmation, Focus, Overlay, PluginPane, Remembered, Route, TuiModel};
use crate::ui::theme::{self, Token};
use crate::ui::worker::Intent;

use super::support::*;

/// Up and down walk one tree across every marketplace: a group's heading,
/// its plugins, the next group's heading — stopping at either end.
#[test]
fn up_and_down_walk_every_group_in_one_tree() {
    let mut model = two_market_model();
    assert_eq!(model.list_len(model.route), 3, "every plugin is listed");

    model.act(uze_keys::Action::SelectNext);
    assert_eq!(model.plugin_pane, PluginPane::Markets);
    assert_eq!(model.plugin_market.as_deref(), Some("ai"));

    model.act(uze_keys::Action::SelectNext);
    assert_eq!(model.plugin_pane, PluginPane::Plugins);
    assert_eq!(
        model
            .selected_marketplace_plugin()
            .map(|plugin| plugin.name),
        Some("git".to_owned())
    );
    assert_eq!(model.plugin_market.as_deref(), Some("ai"));

    for _ in 0..4 {
        model.act(uze_keys::Action::SelectPrevious);
    }
    assert_eq!(model.plugin_pane, PluginPane::Markets);
    assert_eq!(
        model.plugin_market.as_deref(),
        Some("uze-official"),
        "the first heading is as far up as it goes"
    );
}

/// Left and right walk the tree in steps: from a marketplace's heading
/// onto its first plugin, unfolding it, then back out the same way — the
/// fold, the heading, and the screen tabs.
#[test]
fn left_and_right_walk_the_tree_and_a_plugins_resources() {
    let mut model = two_market_model();
    model.select_plugin_market(Some("uze-official".to_owned()));

    model.act(uze_keys::Action::FocusContent);
    assert_eq!(model.plugin_pane, PluginPane::Plugins);
    model.act(uze_keys::Action::FocusContent);
    assert!(model.expanded_plugins.contains("uze@uze-official"));

    model.act(uze_keys::Action::FocusSidebar);
    assert!(
        model.expanded_plugins.is_empty(),
        "the first step back folds"
    );
    model.act(uze_keys::Action::FocusSidebar);
    assert_eq!(model.plugin_pane, PluginPane::Markets);
    assert_eq!(model.plugin_market.as_deref(), Some("uze-official"));
    model.act(uze_keys::Action::FocusSidebar);
    assert_eq!(model.focus, Focus::Sidebar);
}

/// On the rail the removal key removes the marketplace, after asking,
/// and the one that ships inside uze is refused with a reason.
#[test]
fn removing_on_the_rail_asks_about_the_marketplace() {
    let mut model = two_market_model();
    model.plugin_pane = PluginPane::Markets;
    model.plugin_market = Some("ai".to_owned());
    model.act(uze_keys::Action::RemovePlugin);
    assert!(matches!(
        &model.overlay,
        Overlay::Confirm { kind: Confirmation::RemoveMarketplace(name), .. } if name == "ai"
    ));
    assert_eq!(
        model.act(uze_keys::Action::Activate),
        Intent::RemoveMarketplace("ai".to_owned())
    );

    model.close_overlay();
    model.plugin_market = Some("uze-official".to_owned());
    model.act(uze_keys::Action::RemovePlugin);
    assert_eq!(model.overlay, Overlay::None);
}

#[test]
fn add_marketplace_overlay_types_and_submits() {
    let mut model = TuiModel {
        focus: Focus::Content,
        route: Route::Plugins,
        ..TuiModel::default()
    };
    let intent = model.apply_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    assert_eq!(intent, Intent::None);
    assert!(matches!(model.overlay, Overlay::AddMarketplace(ref s) if s.is_empty()));

    for c in "/tmp/mp".chars() {
        model.apply_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    assert!(matches!(model.overlay, Overlay::AddMarketplace(ref s) if s == "/tmp/mp"));

    let intent = model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(intent, Intent::AddMarketplace("/tmp/mp".to_owned()));
    assert_eq!(model.overlay, Overlay::None);
}

#[test]
fn add_marketplace_overlay_esc_cancels_without_intent() {
    let mut model = TuiModel {
        overlay: Overlay::AddMarketplace("abc".to_owned()),
        ..TuiModel::default()
    };
    let intent = model.apply_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(intent, Intent::None);
    assert_eq!(model.overlay, Overlay::None);
}

/// `r` used to remove a plugin on one screen and refresh the machine on
/// every other one — the collision that made the help overlay need an
/// aside column to explain itself. A letter now names one action.
#[test]
fn a_letter_names_one_action_and_refreshing_has_its_own() {
    let mut model = TuiModel {
        focus: Focus::Content,
        route: Route::Overview,
        ..TuiModel::default()
    };
    assert_eq!(
        model.apply_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
        Intent::Refresh,
        "refreshing is not a letter: it is not something done to a row"
    );
    assert_eq!(
        model_with_plugins(&["one"]).apply_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
        Intent::Refresh,
        "and it means the same thing on every screen"
    );

    // Enter already removes an installed plugin, after asking, so no
    // letter says it a second time.
    let mut plugins_model = model_with_plugins(&["one"]);
    plugins_model.apply_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(plugins_model.overlay, Overlay::None);
}

/// A marketplace's panel names where it actually lives, and the address
/// itself is what opens it — a whole row of target, not a one-column
/// glyph you miss by moving the mouse one cell.
#[test]
fn the_source_card_shows_the_marketplace_link_and_offers_to_open_it() {
    let mut model = model_with_plugins(&["one"]);
    model.route = Route::Plugins;
    model.remembered.marketplaces = vec![MarketplaceSummary {
        name: "uze-official".to_owned(),
        source: "embedded:uze-official".to_owned(),
        homepage: Some("https://github.com/uze-sh/uze".to_owned()),
        plugin_count: 1,
        linked_to: None,
    }];
    model.remembered.marketplace_plugins = vec![MarketplacePluginSummary {
        marketplace: "uze-official".to_owned(),
        name: "flow".to_owned(),
        description: Some("A flow plugin".to_owned()),
        keywords: Vec::new(),
        category: None,
        installed: true,
        freshness: up_to_date(),
        installed_at_unix: None,
        is_default: true,
    }];

    model.select_plugin_market(Some("uze-official".to_owned()));
    let (terminal, hits) = drawn_at(&model, 160, 40);
    model.hits = hits;
    let rows = buffer_rows(&terminal);
    assert!(
        rows.iter()
            .any(|row| row.contains("https://github.com/uze-sh/uze")),
        "the address reads on the card: {rows:#?}"
    );

    let rect = model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::OpenLink(name) if name == "uze-official"))
        .map(|(rect, _)| *rect)
        .expect("the address is a target of its own");
    assert!(
        rect.width > 20,
        "and the whole row of it, not one column: {rect:?}"
    );
    for column in [rect.x, rect.x + rect.width / 2, rect.right() - 1] {
        assert_eq!(
            model.click(column, rect.y),
            Intent::OpenLink("https://github.com/uze-sh/uze".to_owned()),
            "clicking anywhere along it hands the address over"
        );
    }
}

/// Rows folded above the address push it down, and its target has to move
/// with it: a link drawn one row from where it answers reads as a link
/// that answers nothing.
#[test]
fn the_source_link_is_clickable_on_the_row_it_is_drawn_on() {
    let mut model = model_with_plugins(&["one"]);
    model.route = Route::Plugins;
    model.remembered.marketplaces = vec![MarketplaceSummary {
        name: "uze-official".to_owned(),
        source: "embedded:uze-official".to_owned(),
        homepage: Some("https://github.com/uze-sh/uze".to_owned()),
        plugin_count: 1,
        linked_to: None,
    }];
    model.remembered.marketplace_plugins = vec![MarketplacePluginSummary {
        marketplace: "uze-official".to_owned(),
        name: "flow".to_owned(),
        description: Some(
            "Makes this project's instructions portable across every harness \
             uze knows about, so switching agents never costs the context \
             the project already wrote down."
                .to_owned(),
        ),
        keywords: vec!["context".to_owned(), "portability".to_owned()],
        category: None,
        installed: true,
        freshness: up_to_date(),
        installed_at_unix: None,
        is_default: true,
    }];

    model.select_plugin_market(Some("uze-official".to_owned()));
    let (terminal, hits) = drawn_at(&model, 160, 40);
    model.hits = hits;

    let rows = buffer_rows(&terminal);
    let drawn = rows
        .iter()
        .position(|row| row.contains("https://github.com/uze-sh/uze"))
        .expect("the address reads on the card") as u16;
    let rect = model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::OpenLink(name) if name == "uze-official"))
        .map(|(rect, _)| *rect)
        .expect("the address is a target of its own");
    assert_eq!(
        rect.y, drawn,
        "the target sits on the row the address is drawn on: {rows:#?}"
    );
    assert_eq!(
        model.click(rect.x + 1, drawn),
        Intent::OpenLink("https://github.com/uze-sh/uze".to_owned()),
    );
}

/// The address answers the pointer: underlined while it is on it. Hover
/// and click read the same hit list, so a row that lights up is a row
/// that answers.
#[test]
fn the_source_link_lights_up_only_under_the_pointer() {
    let mut model = model_with_plugins(&["one"]);
    model.route = Route::Plugins;
    model.remembered.marketplaces = vec![MarketplaceSummary {
        name: "uze-official".to_owned(),
        source: "embedded:uze-official".to_owned(),
        homepage: Some("https://github.com/uze-sh/uze".to_owned()),
        plugin_count: 1,
        linked_to: None,
    }];
    model.remembered.marketplace_plugins = vec![MarketplacePluginSummary {
        marketplace: "uze-official".to_owned(),
        name: "flow".to_owned(),
        description: Some("A flow plugin".to_owned()),
        keywords: Vec::new(),
        category: None,
        installed: true,
        freshness: up_to_date(),
        installed_at_unix: None,
        is_default: true,
    }];

    model.select_plugin_market(Some("uze-official".to_owned()));
    let (_terminal, hits) = drawn_at(&model, 160, 40);
    model.hits = hits;
    let rect = model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::OpenLink(name) if name == "uze-official"))
        .map(|(rect, _)| *rect)
        .expect("the address is a target of its own");

    assert!(
        !model.source_link_hovered,
        "muted until the pointer arrives"
    );
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(
        model.source_link_hovered,
        "and lit while the pointer is on it"
    );
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: rect.x + 1,
            row: rect.y + 1,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(!model.source_link_hovered, "muted again once it leaves");
}

/// A marketplace heads its group: its name and where it comes from, how
/// many plugins it offers, and the one thing worth knowing about it —
/// what asks for action, or how much of it is installed. Adding another
/// is the key on the screen's own header, the last thing on its row.
#[test]
fn a_marketplace_heads_its_group_with_what_needs_saying() {
    let model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        overlay: Overlay::None,
        ..model_with_data()
    };
    let (terminal, hits) = drawn_at(&model, 150, 30);
    let rows = buffer_rows(&terminal);
    let text = rows.join("\n");
    let row_of = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not drawn:\n{text}"))
    };
    let heading = row_of("category");
    assert!(
        rows[heading].contains("name") && rows[heading].contains("status"),
        "{text}"
    );
    let uze = row_of(&format!(
        "uze  {} Official",
        theme::glyph(theme::Symbol::MarkOfficial)
    ));
    assert!(rows[uze].contains("1 plugin"), "{text}");
    assert!(rows[uze].contains("1/1 installed"), "{text}");
    let local = row_of("local  installed directly");
    assert!(rows[local].contains("1 update"), "{text}");

    let market = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::PluginMarket(Some("local".to_owned())))
        .map(|(rect, _)| *rect)
        .expect("the heading is a target");
    assert_eq!(market.y as usize, local, "{text}");
    assert_eq!(market.height, 1);

    let offer = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::AddMarketplace))
        .map(|(rect, _)| *rect)
        .expect("adding a marketplace is a target");
    assert!(
        rows[offer.y as usize].contains("Marketplace")
            && rows[offer.y as usize].contains("marketplaces ·"),
        "on the screen's header row: {text}"
    );
    assert!(
        !hits.iter().any(|(rect, hit)| rect.y == offer.y
            && rect.x > offer.x
            && !matches!(hit, Hit::ResizePanel(_))
            && rect.x < offer.right() + 2),
        "it ends the tree's header row"
    );
}

/// A marketplace's card answers a click anywhere on it: its rows of air
/// pick the marketplace the way its heading does, rather than leaving
/// stripes of the card that do nothing.
#[test]
fn every_row_of_a_marketplace_card_answers_a_click() {
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        overlay: Overlay::None,
        ..model_with_data()
    };
    let (_terminal, hits) = drawn_at(&model, 150, 30);
    let heading = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::PluginMarket(Some("local".to_owned())))
        .map(|(rect, _)| *rect)
        .expect("the heading is a target");
    let column = heading.x + heading.width / 2;
    // The heading, a row of air, its two plugins and the row closing it.
    for row in heading.y..heading.y + 5 {
        let answers = hits
            .iter()
            .find(|(rect, _)| rect.contains(ratatui::layout::Position::new(column, row)))
            .map(|(_, hit)| hit.clone());
        assert!(
            matches!(answers, Some(Hit::PluginMarket(_) | Hit::MarketplaceRow(_))),
            "row {row} of the card answers nothing: {answers:?}"
        );
    }
    model.hits = hits;
    model.click(column, heading.y + 1);
    assert_eq!(model.plugin_pane, PluginPane::Markets);
    assert_eq!(model.plugin_market.as_deref(), Some("local"));
}

/// A first look at the Marketplace starts on its first marketplace, not
/// on a plugin inside it — and a return visit lands wherever the last one
/// left the keyboard.
#[test]
fn the_marketplace_opens_on_its_first_marketplace_until_something_is_chosen() {
    let mut first = TuiModel::recall(
        Some(Remembered {
            marketplaces: vec![registered("uze-official"), registered("ai")],
            marketplace_plugins: vec![
                marketplace_plugin("uze-official", "uze", true),
                marketplace_plugin("ai", "git", true),
            ],
            ..TuiModel::default().remembered
        }),
        &uze_application::ManagementLayout::default(),
    );
    first.route = Route::Plugins;
    first.focus = Focus::Content;
    assert_eq!(
        first.plugin_tree_cursor(),
        crate::ui::model::PluginTreeRow::Market("uze-official".to_owned()),
        "nothing chosen yet: the first marketplace"
    );
    assert!(first.selected_market_summary().is_some(), "and its panel");

    first.act(uze_keys::Action::FocusContent);
    assert_eq!(first.plugin_pane, PluginPane::Plugins);
    let layout = first.management_layout();
    let again = TuiModel::recall(Some(first.remember()), &layout);
    assert_eq!(
        again.plugin_pane,
        PluginPane::Plugins,
        "a plugin chosen last time is where the next visit opens"
    );
}

/// A marketplace's blank lines answer for its heading when clicked, but
/// the pointer on the heading lights the heading alone — never every line
/// that shares its target.
#[test]
fn hovering_a_marketplace_lights_only_the_line_under_the_pointer() {
    let mut model = model_with_plugins(&["one", "two", "three"]);
    let (_terminal, hits) = drawn_at(&model, 140, 30);
    model.hits = hits;
    let market_lines: Vec<Rect> = model
        .hits
        .iter()
        .filter(|(_, hit)| matches!(hit, Hit::PluginMarket(Some(_))))
        .map(|(rect, _)| *rect)
        .collect();
    assert!(market_lines.len() > 1, "the group has lines of its own");
    let heading = market_lines[0];
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: heading.x + 1,
            row: heading.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 140, 30),
    );
    let (terminal, _) = drawn_at(&model, 140, 30);
    let lit: Vec<u16> = market_lines
        .iter()
        .filter(|rect| {
            terminal.backend().buffer()[(rect.x + 1, rect.y)].bg
                == theme::color(Token::SurfaceHover)
        })
        .map(|rect| rect.y)
        .collect();
    assert_eq!(lit, vec![heading.y], "the heading alone should light up");
}
