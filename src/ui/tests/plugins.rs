//! The plugin rows: their drawer, their resources, their updates and their removal.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use uze_application::application::{DoctorReport, MaintenanceReport, MarketplacePluginSummary};

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{
    Confirmation, Focus, ListScreen, Overlay, PluginPane, Remembered, Route, TrustedRetry, TuiModel,
};
use crate::ui::theme::{self, Token};
use crate::ui::worker::{Intent, TrustGrant};

use super::support::*;

/// The drawer opens by default on whichever row is selected, and the
/// list itself lands from a background refresh — so the first selection
/// is never "navigated to", and nothing else would ask for its detail.
#[test]
fn an_open_drawer_asks_for_the_detail_it_is_missing_exactly_once() {
    let mut model = model_with_plugins(&["one", "two"]);
    let wanted = Intent::InspectPlugin("one".to_owned());
    assert_eq!(model.drawer_inspect_intent(), wanted, "nothing fetched yet");

    model.inspection_in_flight = Some(wanted.clone());
    assert_eq!(
        model.drawer_inspect_intent(),
        Intent::None,
        "the same fetch is not queued again while it runs"
    );

    model.apply_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(
        model.drawer_inspect_intent(),
        Intent::InspectPlugin("two".to_owned()),
        "moving the selection wants the new row's detail even mid-flight"
    );

    model.route = Route::Overview;
    assert_eq!(
        model.drawer_inspect_intent(),
        Intent::None,
        "nor does another screen"
    );
}

#[test]
fn remove_confirmation_flow() {
    let mut model = model_with_plugins(&["one"]);
    model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        matches!(model.overlay, Overlay::Confirm { kind: Confirmation::RemovePlugin(ref id), .. } if id == "one")
    );
    // No letter answers a question: neither `y` nor `n` is heard.
    for letter in ['y', 'n'] {
        let intent = model.apply_key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
        assert_eq!(intent, Intent::None);
        assert!(matches!(model.overlay, Overlay::Confirm { .. }), "{letter}");
    }
    let intent = model.apply_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(intent, Intent::None);
    assert_eq!(model.overlay, Overlay::None);
    assert_eq!(model.focus, Focus::Content);
}

#[test]
fn remove_confirmed_emits_remove_intent() {
    let mut model = model_with_plugins(&["one"]);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    model.apply_key(enter);
    let intent = model.apply_key(enter);
    assert_eq!(intent, Intent::Remove("one".to_owned()));
}

#[test]
fn update_only_offered_when_available() {
    let mut model = model_with_plugins(&["one"]);
    model.apply_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
    assert_eq!(
        model.overlay,
        Overlay::None,
        "no update available, no overlay"
    );
    model.remembered.plugins[0].freshness = behind();
    model.apply_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
    assert!(
        matches!(model.overlay, Overlay::Confirm { kind: Confirmation::UpdatePlugin(ref id), .. } if id == "one")
    );
}

#[test]
fn an_auto_updated_plugin_badges_until_the_plugins_screen_has_shown_it() {
    use crate::ui::model::{RefreshData, UPDATE_BADGE_TTL, UpdateBadge};

    let mut model = model_with_plugins(&["one"]);
    model.route = Route::Overview;
    model.refreshed(RefreshData {
        auto_updated: vec!["one".to_owned()],
        ..RefreshData::default()
    });
    assert!(model.was_just_updated("one"));

    // Off the Plugins screen the badge never starts its countdown — an
    // operator who has not looked at it yet has not been told anything.
    for _ in 0..3 {
        model.expire_update_badges();
    }
    assert!(
        model.remembered.update_badges[0].seen_at.is_none(),
        "the countdown starts on sight, not on the update"
    );
    assert!(model.was_just_updated("one"));

    model.route = Route::Plugins;
    model.expire_update_badges();
    assert!(model.remembered.update_badges[0].seen_at.is_some());

    // Once seen, it comes down on its own.
    model.remembered.update_badges[0].seen_at = Some(std::time::Instant::now() - UPDATE_BADGE_TTL);
    model.expire_update_badges();
    assert!(
        !model.was_just_updated("one"),
        "the badge expires after its TTL"
    );

    // An ordinary refresh reports no auto-updates and must not re-raise
    // a badge that already had its moment.
    model.remembered.update_badges.push(UpdateBadge {
        plugin: "two".to_owned(),
        seen_at: None,
    });
    model.refreshed(RefreshData::default());
    assert!(
        model.was_just_updated("two"),
        "a live badge survives a plain refresh"
    );
    assert!(!model.was_just_updated("one"));
}

#[test]
fn trust_required_overlay_confirm_regrants_with_trust() {
    let mut model = TuiModel {
        overlay: Overlay::Confirm {
            kind: Confirmation::Trust {
                plugin: "acme".to_owned(),
                detail: "acme -> mcp-server".to_owned(),
                retry: TrustedRetry::Install {
                    name: "acme".to_owned(),
                    marketplace: "uze-official".to_owned(),
                },
            },
            focus: None,
        },
        ..TuiModel::default()
    };
    let intent = model.apply_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        intent,
        Intent::Install {
            name: "acme".to_owned(),
            marketplace: "uze-official".to_owned(),
            grant: TrustGrant::Granted,
        }
    );
    assert_eq!(model.overlay, Overlay::None);
}

/// The wheel moves the page, not the selection, and never mutates: the
/// keyboard's row and the detail it asked for stay where they were.
#[test]
fn the_wheel_scrolls_the_plugins_without_moving_or_mutating() {
    let mut model = model_with_plugins(&["one", "two", "three"]);
    let intent = model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 100, 40),
    );
    assert_eq!(intent, Intent::None);
    assert_eq!(model.remembered.plugin_screen.selected, 0);
    assert_eq!(model.remembered.plugin_screen.scroll.get().first, 1);
    assert!(!model.selection_settling(std::time::Instant::now()));
}

#[test]
fn attachment_health_is_never_unknown_after_a_refresh() {
    use uze_application::application::{ManagedStateSummary, PackageManagedState};
    // Every refresh carries the full doctor with attachments (served by
    // the inspection cache), so the Plugins drawer's status line derives
    // real health from it instead of the masked "unknown" placeholder.
    let mut model = model_with_plugins(&["one"]);
    model.route = Route::Plugins;
    model.remembered.doctor = Some(DoctorReport {
        uze_home: PathBuf::from("/home"),
        store: uze_application::application::StoreHealth::Ready,
        plugins: vec![plugin("one")],
        harnesses: Vec::new(),
        attachments: vec![PackageManagedState {
            hooks: Vec::new(),
            plugin: "one".to_owned(),
            state: ManagedStateSummary {
                matched: 2,
                missing: 0,
                drifted: 0,
                conflicts: 0,
                blocked: 0,
                ledger_error: None,
            },
        }],
        deliveries: Vec::new(),
        ledger_error: None,
        provisioning_state_error: None,
        leftovers: Default::default(),
        maintenance: MaintenanceReport::default(),
        git_found: true,
        shell_refusal: None,
        ssh_missing: false,
        machine_concerns: Vec::new(),
    });
    let (terminal, _hits) = drawn_at(&model, 100, 40);
    let rows = buffer_rows(&terminal);
    assert!(
        rows.iter().any(|row| row.to_lowercase().contains("ready")),
        "a refreshed report must render real health, got:\n{rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("unknown")),
        "attachment health must never read 'unknown' after a refresh"
    );
}

/// The drawer's buttons are the plugin's offers, clickable where the
/// plugin is described — and only what can run now.
#[test]
fn the_drawer_offers_what_can_be_done_as_buttons() {
    let summary = MarketplacePluginSummary {
        marketplace: "team".to_owned(),
        name: "kit".to_owned(),
        description: None,
        keywords: Vec::new(),
        category: None,
        installed: true,
        freshness: behind(),
        installed_at_unix: None,
        is_default: false,
    };
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        plugin_pane: PluginPane::Plugins,
        remembered: Remembered {
            plugin_screen: ListScreen::default(),
            marketplace_plugins: vec![summary],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let (mut terminal, hits) = drawn_at(&model, 120, 40);
    model.hits = hits;
    let buttons: Vec<_> = model
        .hits
        .iter()
        .filter_map(|(rect, hit)| match hit {
            // The first-steps section shares the hit; its entries are not
            // this plugin's.
            Hit::OfferedAction(
                action @ (uze_keys::Action::InstallPlugin
                | uze_keys::Action::UpdatePlugin
                | uze_keys::Action::RemovePlugin),
            ) => Some((*rect, *action)),
            _ => None,
        })
        .collect();
    assert_eq!(
        buttons
            .iter()
            .map(|(_, action)| *action)
            .collect::<Vec<_>>(),
        [
            uze_keys::Action::UpdatePlugin,
            uze_keys::Action::RemovePlugin
        ],
        "what builds first, what destroys last, and nothing that cannot run"
    );
    assert!(
        buttons[0].0.y == buttons[1].0.y && buttons[0].0.x < buttons[1].0.x,
        "the update stands in the row of actions, not beside its note: {buttons:?}"
    );
    assert_eq!(
        model
            .selected_marketplace_plugin()
            .and_then(|plugin| crate::ui::view::plugins::primary_offer(&plugin.offers())),
        Some(uze_keys::Action::UpdatePlugin),
        "and Enter takes the update rather than the removal"
    );

    // Soft at rest, the full hue under the pointer.
    let (remove, _) = buttons[1];
    let resting = terminal.backend().buffer()[(remove.x, remove.y)].bg;
    assert_ne!(
        theme::token_of(resting),
        Some(Token::StateDanger),
        "a resting button is not the full hue"
    );
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: remove.x,
            row: remove.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 120, 40),
    );
    assert_eq!(model.hovered_offer, Some(uze_keys::Action::RemovePlugin));
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    assert_eq!(
        theme::token_of(terminal.backend().buffer()[(remove.x, remove.y)].bg),
        Some(Token::StateDanger),
        "and the one under the pointer is"
    );

    let (update, _) = buttons[0];
    model.click(update.x, update.y);
    assert!(
        matches!(model.overlay, Overlay::Confirm { kind: Confirmation::UpdatePlugin(ref id), .. } if id.contains("kit")),
        "the button asks, the same way the menu entry does: {:?}",
        model.overlay
    );
}

/// An unfolded plugin's resources are grouped by kind, each kind headed
/// with how many there are, so a skill never reads as a hook because the
/// two shared one line.
#[test]
fn an_unfolded_plugin_groups_its_resources_by_kind() {
    use uze_application::CapabilityKind;
    use uze_application::application::PluginCapability;

    let capability = |name: &str, kind| PluginCapability {
        identity: name.to_owned(),
        name: name.to_owned(),
        kind,
        preview: Default::default(),
    };
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        remembered: Remembered {
            marketplace_plugins: vec![MarketplacePluginSummary {
                category: Some("productivity".to_owned()),
                ..marketplace_plugin("team", "kit", false)
            }],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    model.expanded_plugins.insert("kit@team".to_owned());
    model.plugin_resources.insert(
        "kit@team".to_owned(),
        vec![
            capability("review", CapabilityKind::AgentSkill),
            capability("guard", CapabilityKind::Hook),
            capability("plan", CapabilityKind::AgentSkill),
        ],
    );
    let (terminal, _hits) = drawn_at(&model, 140, 40);
    let rows = buffer_rows(&terminal);
    let tree = |row: &String| row.split('│').next().unwrap_or_default().to_owned();

    let skills = rows
        .iter()
        .position(|row| tree(row).contains("skills"))
        .unwrap_or_else(|| panic!("the skills branch: {rows:#?}"));
    assert!(tree(&rows[skills]).contains(" 2 "), "{rows:#?}");
    assert!(tree(&rows[skills + 1]).contains("review") && tree(&rows[skills + 2]).contains("plan"));
    assert!(
        tree(&rows[skills + 3]).contains("hooks") && tree(&rows[skills + 4]).contains("guard"),
        "hooks under a heading of their own: {rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| tree(row).contains("kit") && tree(row).contains("productivity")),
        "the plugin's row says what kind of work it is for: {rows:#?}"
    );
}

/// Inspecting a resource keeps its plugin's actions, labelled the way the
/// plugin's own row labels them rather than repeating the plugin's name.
#[test]
fn a_selected_resource_offers_its_plugins_action_by_its_plain_label() {
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        remembered: Remembered {
            marketplace_plugins: vec![marketplace_plugin("team", "kit", false)],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    model.expanded_plugins.insert("kit@team".to_owned());
    model.plugin_resources.insert(
        "kit@team".to_owned(),
        vec![uze_application::application::PluginCapability {
            identity: "review".to_owned(),
            name: "review".to_owned(),
            kind: uze_application::CapabilityKind::AgentSkill,
            preview: Default::default(),
        }],
    );
    model.select_plugin_row(0, Some("review".to_owned()));
    assert!(model.selected_resource().is_some());

    let (terminal, hits) = drawn_at(&model, 140, 40);
    let (rect, _) = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::InstallPlugin))
        .unwrap_or_else(|| panic!("the install button: {hits:?}"));
    let row = &buffer_rows(&terminal)[rect.y as usize];
    let label: String = row
        .chars()
        .skip(rect.x as usize)
        .take(rect.width as usize)
        .collect();
    assert_eq!(
        label.trim(),
        uze_keys::Action::InstallPlugin.label(),
        "{row}"
    );
}

fn resource(
    identity: &str,
    kind: uze_application::CapabilityKind,
    path: &str,
    text: &str,
) -> uze_application::application::PluginCapability {
    uze_application::application::PluginCapability {
        identity: identity.to_owned(),
        name: identity.to_owned(),
        kind,
        preview: uze_application::application::CapabilityPreview {
            path: path.to_owned(),
            text: text.to_owned(),
        },
    }
}

/// `git` unfolded, with a skill and an MCP server under it.
fn unfolded_git() -> TuiModel {
    use uze_application::CapabilityKind;
    let mut model = two_market_model();
    model.expanded_plugins.insert("git@ai".to_owned());
    model.plugin_resources.insert(
        "git@ai".to_owned(),
        vec![
            resource(
                "term",
                CapabilityKind::Mcp,
                "mcp.json",
                "{\n  \"command\": \"npx\"\n}",
            ),
            resource(
                "commit",
                CapabilityKind::AgentSkill,
                "skills/commit/SKILL.md",
                "---\nname: commit\ndescription: Conventional commit messages.\n---\n\n# Commit\n\nWrite one from the staged diff.\n",
            ),
        ],
    );
    model
}

/// The arrows walk an unfolded plugin's resources in the order the tree
/// draws them — skills before MCP servers, whatever order they arrived in
/// — and the step back from a resource lands on its plugin.
#[test]
fn the_arrows_walk_an_unfolded_plugins_resources_in_drawn_order() {
    let mut model = unfolded_git();
    model.select_plugin_row(1, None);

    model.act(uze_keys::Action::SelectNext);
    assert_eq!(
        model.selected_resource().map(|r| r.name).as_deref(),
        Some("commit")
    );
    model.act(uze_keys::Action::SelectNext);
    assert_eq!(
        model.selected_resource().map(|r| r.name).as_deref(),
        Some("term")
    );
    model.act(uze_keys::Action::SelectNext);
    assert_eq!(
        model.remembered.plugin_screen.selected, 2,
        "then the next plugin"
    );
    assert!(model.selected_resource().is_none());

    model.act(uze_keys::Action::SelectPrevious);
    assert_eq!(
        model.selected_resource().map(|r| r.name).as_deref(),
        Some("term")
    );
    model.act(uze_keys::Action::FocusSidebar);
    assert!(
        model.selected_resource().is_none(),
        "left returns to the plugin"
    );
    assert_eq!(model.remembered.plugin_screen.selected, 1);
    assert!(
        model.expanded_plugins.contains("git@ai"),
        "which stays unfolded"
    );
}

/// A resource the keyboard is on is previewed in the drawer: what it is,
/// where it sits, and its text rendered — the frontmatter as a key/value
/// table rather than read as a rule, the body as Markdown.
#[test]
fn a_selected_resource_is_previewed_in_the_drawer() {
    let mut model = unfolded_git();
    model.select_plugin_row(1, Some("commit".to_owned()));
    let (terminal, hits) = drawn_at(&model, 150, 30);
    let rows = buffer_rows(&terminal);
    // The drawer is right of the list's rule; the frontmatter's own table
    // draws rules inside it, so the cut is at the first one past the list.
    let drawer: Vec<String> = rows
        .iter()
        .map(|row| row.split_once('│').map_or("", |(_, rest)| rest).to_owned())
        .collect();

    assert!(
        drawer
            .iter()
            .any(|row| row.contains("│ description ") && row.contains("Conventional commit")),
        "the frontmatter is a table: {drawer:#?}"
    );
    for expected in [
        "SKILL",
        "skills/commit/SKILL.md",
        "Write one from the staged diff.",
    ] {
        assert!(
            drawer.iter().any(|row| row.contains(expected)),
            "{expected:?} in the drawer: {drawer:#?}"
        );
    }
    assert!(
        !drawer.iter().any(|row| row.trim() == "---"),
        "the frontmatter's fences are not drawn as a rule: {drawer:#?}"
    );
    assert!(hits.iter().any(|(_, hit)| *hit == Hit::ResourcePreview));
}

/// A long preview scrolls to where its last row meets the drawer's bottom
/// and no further, so the way back up costs no presses spent past its end.
#[test]
fn a_resource_preview_scrolls_to_its_end_and_stops() {
    let mut model = unfolded_git();
    let long: String = (0..80).map(|n| format!("Line {n}.\n\n")).collect();
    model.plugin_resources.get_mut("git@ai").unwrap()[1]
        .preview
        .text = long;
    model.select_plugin_row(1, Some("commit".to_owned()));
    let mut terminal = Terminal::new(TestBackend::new(150, 30)).unwrap();
    let mut draw = |model: &TuiModel| {
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
    };
    draw(&model);

    for _ in 0..40 {
        model.act(uze_keys::Action::ScrollPageDown);
    }
    let limit = crate::ui::view::plugins::preview_scroll_limit();
    assert!(limit > 0, "the preview is longer than the drawer");
    assert_eq!(model.resource_scroll, limit);

    model.act(uze_keys::Action::ScrollPageUp);
    assert!(
        model.resource_scroll < limit,
        "one page back is a page back"
    );
    draw(&model);
}

/// The whole row answers, not the chevron alone: a click puts the keyboard
/// on a plugin, and a click on the plugin it is already on opens or folds
/// its resources.
#[test]
fn a_click_on_the_selected_plugin_row_opens_and_folds_it() {
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Sidebar,
        overlay: Overlay::None,
        ..model_with_data()
    };
    let click_row = |model: &mut TuiModel, index: usize| {
        let mut terminal = Terminal::new(TestBackend::new(150, 26)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        let rect = hits
            .iter()
            .find(|(_, hit)| *hit == Hit::MarketplaceRow(index))
            .map(|(rect, _)| *rect)
            .expect("the row is a target");
        model.hits = hits;
        // Past the chevron, on the name.
        model.click(rect.x + 6, rect.y);
    };
    let expanded = |model: &TuiModel| {
        let plugin = model.selected_marketplace_plugin().expect("a plugin");
        model
            .expanded_plugins
            .contains(&model.marketplace_plugin_id(&plugin))
    };

    click_row(&mut model, 1);
    assert_eq!(model.remembered.plugin_screen.selected, 1);
    assert!(
        !expanded(&model),
        "the first click selects and nothing more"
    );
    click_row(&mut model, 1);
    assert!(expanded(&model), "a click on the selected row opens it");
    click_row(&mut model, 1);
    assert!(!expanded(&model), "and another folds it");
}

/// The group the keyboard is in stands on a recessed ground, heading and
/// all, so the reader does not lose which marketplace they are looking
/// into; the row the keyboard is on takes the selection's ground and the
/// cursor ahead of it.
#[test]
fn the_active_group_keeps_its_ground_while_a_plugin_is_selected() {
    let mut model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        overlay: Overlay::None,
        ..model_with_data()
    };
    let rows = model.marketplace_rows();
    let one = model
        .visible_indices_in(&rows)
        .iter()
        .position(|&raw| rows[raw].name == "one")
        .expect("the local plugin is listed");
    model.select_plugin_row(one, None);
    let (terminal, hits) = drawn_at(&model, 150, 30);
    let rows = buffer_rows(&terminal);
    let heading = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::PluginMarket(Some("local".to_owned())))
        .map(|(rect, _)| *rect)
        .expect("the heading is a target");
    let buffer = terminal.backend().buffer();
    for x in [heading.x, heading.right() - 1] {
        assert_eq!(
            buffer[(x, heading.y)].bg,
            theme::color(Token::SurfaceRecessed),
            "the group's ground spans its heading at column {x}"
        );
    }
    let other = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::PluginMarket(Some("uze-official".to_owned())))
        .map(|(rect, _)| *rect)
        .expect("the other heading is a target");
    assert_ne!(
        buffer[(other.x + 2, other.y)].bg,
        theme::color(Token::SurfaceRecessed),
        "a group the keyboard is not in keeps the surface's ground"
    );
    let selected = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::MarketplaceRow(one))
        .map(|(rect, _)| *rect)
        .expect("the plugin is a target");
    assert_eq!(
        buffer[(selected.x + 4, selected.y)].bg,
        theme::color(Token::SurfaceSelected)
    );
    let cursor = format!(
        "{} {}",
        theme::glyph(theme::Symbol::Prompt),
        theme::glyph(theme::Symbol::ChevronCollapsed)
    );
    assert!(
        rows[selected.y as usize].contains(&cursor),
        "the cursor stands just ahead of the plugin: {:?}",
        rows[selected.y as usize]
    );
}

/// The updated column says when each plugin was installed or last updated
/// here — not when uze last checked for updates — and nothing for one that
/// is not installed; its marketplace says its most recent.
#[test]
fn the_updated_column_is_when_a_plugin_was_installed() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut installed = marketplace_plugin("team", "kit", true);
    installed.installed_at_unix = Some(now - 3 * 86_400);
    installed.freshness.established_at_unix = Some(now - 600);
    let mut newer = marketplace_plugin("team", "lint", true);
    newer.installed_at_unix = Some(now - 2 * 3_600);
    let available = marketplace_plugin("team", "draft", false);
    let model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        remembered: Remembered {
            marketplace_plugins: vec![installed, newer, available],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let (terminal, _hits) = drawn_at(&model, 150, 30);
    let rows = buffer_rows(&terminal);
    let tree = |needle: &str| {
        rows.iter()
            .map(|row| row.split('│').next().unwrap_or_default().to_owned())
            .find(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not drawn: {rows:#?}"))
    };
    assert!(tree("▸ kit").contains("3d ago"), "{}", tree("▸ kit"));
    assert!(!tree("▸ kit").contains("10m ago"), "not the update check");
    assert!(tree("▸ lint").contains("2h ago"));
    assert!(!tree("▸ draft").contains("ago"), "{}", tree("▸ draft"));
    assert!(tree("team  ").contains("2h ago"), "its most recent");
}

/// The selection lights the container it is: the whole marketplace card
/// while its heading is selected, and a plugin with every resource under
/// it while the keyboard is anywhere inside that plugin — siblings keep
/// the group's ground, and the cursor alone says which row it is.
#[test]
fn the_selection_lights_its_whole_container() {
    use uze_application::CapabilityKind;
    use uze_application::application::PluginCapability;

    let mut model = two_market_model();
    model.expanded_plugins.insert("git@ai".to_owned());
    model.plugin_resources.insert(
        "git@ai".to_owned(),
        vec![PluginCapability {
            identity: "commit".to_owned(),
            name: "commit".to_owned(),
            kind: CapabilityKind::AgentSkill,
            preview: Default::default(),
        }],
    );
    let selected = theme::color(Token::SurfaceSelected);
    let ground_of = |model: &TuiModel, needle: &str| {
        let (terminal, hits) = drawn_at(model, 150, 30);
        let rows = buffer_rows(&terminal);
        let y = rows
            .iter()
            .position(|row| {
                row.split('│')
                    .next()
                    .is_some_and(|tree| tree.contains(needle))
            })
            .unwrap_or_else(|| panic!("{needle} not drawn: {rows:#?}"));
        let x = hits
            .iter()
            .find_map(|(rect, hit)| matches!(hit, Hit::PluginMarket(_)).then_some(rect.x + 1))
            .expect("a group is drawn");
        terminal.backend().buffer()[(x, y as u16)].bg
    };

    model.select_plugin_market(Some("ai".to_owned()));
    for row in ["ai ", "git", "commit", "env"] {
        assert_eq!(
            ground_of(&model, row),
            selected,
            "{row} is inside the selected card"
        );
    }
    assert_ne!(
        ground_of(&model, "uze-official"),
        selected,
        "the other card is not"
    );

    let rows = model.marketplace_rows();
    let git = model
        .visible_indices_in(&rows)
        .iter()
        .position(|&raw| rows[raw].name == "git")
        .expect("git is listed");
    model.select_plugin_row(git, Some("commit".to_owned()));
    assert_eq!(
        ground_of(&model, "git"),
        selected,
        "the plugin the cursor is in"
    );
    assert_eq!(ground_of(&model, "commit"), selected, "and its resources");
    assert_eq!(
        ground_of(&model, "env"),
        theme::color(Token::SurfaceRecessed),
        "a sibling keeps the group's ground"
    );
}

/// The panel's action stands a row apart from what is above it, so a long
/// preview scrolling in the body never runs into the button.
#[test]
fn the_panel_action_has_a_row_of_air_above_it() {
    let model = TuiModel {
        route: Route::Plugins,
        focus: Focus::Content,
        plugin_pane: PluginPane::Plugins,
        remembered: Remembered {
            marketplace_plugins: vec![MarketplacePluginSummary {
                description: Some("word ".repeat(400)),
                ..marketplace_plugin("team", "kit", false)
            }],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    };
    let (terminal, hits) = drawn_at(&model, 150, 30);
    let button = hits
        .iter()
        .find(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::InstallPlugin))
        .map(|(rect, _)| *rect)
        .expect("the panel offers to install");
    let above: String = buffer_rows(&terminal)[button.y as usize - 1]
        .chars()
        .skip(button.x as usize)
        .take(20)
        .collect();
    assert!(
        above.trim().is_empty(),
        "the row above the button: {above:?}"
    );
}

/// What the machine lacks for an installed plugin reads on its row and in
/// its panel, and the panel's line hands the command that installs it to
/// a shell rather than running it; a gap with no known installer says to
/// install it by hand and hands nothing over.
#[test]
fn a_missing_requirement_reads_on_the_plugin_and_hands_its_install_to_a_shell() {
    use uze_application::application::{RequirementLine, RequirementStatus};
    let gap = |executable: &str, install: Option<&str>, denies: &[&str]| RequirementLine {
        executable: executable.to_owned(),
        minimum: None,
        purpose: None,
        needed_by: vec!["hook `secret-guard`".to_owned()],
        denies_while_unmet: denies.iter().map(|hook| (*hook).to_owned()).collect(),
        status: RequirementStatus::Missing,
        install_command: install.map(str::to_owned),
    };
    let mut model = model_with_plugins(&["flow@uze-official"]);
    model.remembered.plugins[0].requirement_gaps = vec![
        gap(
            "python3",
            Some("sudo apt-get install -y python3"),
            &["secret-guard"],
        ),
        gap("uze-lab-tool", None, &[]),
    ];
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
    model.select_plugin_row(0, None);
    let (terminal, hits) = drawn_at(&model, 200, 40);
    model.hits = hits;
    let rows = buffer_rows(&terminal);

    assert!(
        rows.iter().any(|row| row.contains("needs python3")),
        "the row's status names what is missing: {rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("python3 missing") && row.contains("`secret-guard` denies")),
        "the panel says what the gap costs: {rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("uze-lab-tool missing"))
            && rows.iter().any(|row| row.contains("by hand")),
        "{rows:#?}"
    );
    let rect = model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::TypeInShell(_)))
        .map(|(rect, _)| *rect)
        .expect("the gap with an installer is a target");
    assert_eq!(
        model.click(rect.x, rect.y),
        Intent::TypeInShell("sudo apt-get install -y python3".to_owned()),
    );
    assert_eq!(
        model
            .hits
            .iter()
            .filter(|(_, hit)| matches!(hit, Hit::TypeInShell(_)))
            .count(),
        1,
        "a gap installed by hand hands nothing over"
    );
}
