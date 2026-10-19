//! Fixtures and drawing helpers the management client's tests share.

use std::path::PathBuf;

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use uze_application::application::{
    DoctorReport, MaintenanceReport, MarketplacePluginSummary, MarketplaceSummary, PluginSummary,
};

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{Focus, PluginPane, Remembered, Route, TuiModel};

pub(super) fn plugin(id: &str) -> PluginSummary {
    PluginSummary {
        id: id.to_owned(),
        active_name: id.to_owned(),
        source: "embedded:example".to_owned(),
        store_path: PathBuf::from("/store/example"),
        commit: None,
        capability_count: 2,
        freshness: uze_application::application::Freshness::not_checked(),
        installed_at_unix: None,
        undelivered: Vec::new(),
        requirement_gaps: Vec::new(),
    }
}

/// A plugin with something newer waiting, and one already current — the
/// two states every plugin row branches on.
pub(super) fn behind() -> uze_application::application::Freshness {
    uze_application::application::Freshness {
        state: uze_application::application::FreshnessState::Behind { commits: None },
        established_at_unix: Some(0),
    }
}

pub(super) fn up_to_date() -> uze_application::application::Freshness {
    uze_application::application::Freshness {
        state: uze_application::application::FreshnessState::UpToDate,
        established_at_unix: Some(0),
    }
}

pub(super) fn model_with_plugins(ids: &[&str]) -> TuiModel {
    TuiModel {
        focus: Focus::Content,
        route: Route::Plugins,
        // On a plugin rather than on its marketplace's heading: what these
        // tests act on is the row of a plugin.
        plugin_pane: PluginPane::Plugins,
        remembered: Remembered {
            plugins: ids.iter().map(|id| plugin(id)).collect(),
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    }
}

/// A model with every route's list populated (plugins, marketplace,
/// harnesses) and a mixed-severity doctor report, so rendering each
/// route exercises its non-empty branch rather than only the
/// nothing-loaded-yet placeholder every other test leaves in place.
pub(super) fn model_with_data() -> TuiModel {
    use uze_application::application::{
        ContextMechanism, HarnessContextDelivery, HarnessContextStatus, HarnessContextSupport,
        HarnessHealth, ManagedStateSummary, PackageManagedState, Portability, ProjectContextStatus,
        StoreHealth,
    };
    use uze_core::integration::{HarnessDetection, PublicationStatus};
    use uze_core::router::HarnessCapabilities;

    let mut model = model_with_plugins(&["one", "two"]);
    model.remembered.plugins[0].freshness = behind();
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
        keywords: vec!["flow".to_owned()],
        category: None,
        installed: true,
        freshness: up_to_date(),
        installed_at_unix: None,
        is_default: true,
    }];
    // Renders the "Updated" badge branch on every route that shows plugin
    // rows, alongside the "Update available" one `plugins[0]` carries.
    model.remembered.update_badges = vec![crate::ui::model::UpdateBadge {
        plugin: "flow@uze-official".to_owned(),
        seen_at: None,
    }];
    model.remembered.doctor = Some(DoctorReport {
        uze_home: PathBuf::from("/home/uze"),
        store: StoreHealth::Ready,
        plugins: model.remembered.plugins.clone(),
        harnesses: vec![
            HarnessHealth {
                below_minimum: None,
                integration: "claude-code".to_owned(),
                display_name: "Claude Code".to_owned(),
                description: "Anthropic's official coding agent CLI".to_owned(),
                detection: HarnessDetection {
                    present: true,
                    version: Some("1.0.0".to_owned()),
                },
                setup: "configured, verified".to_owned(),
                strategy: Some("managed-user-scope-skills-dir".to_owned()),
                provisioning: None,
                publication: PublicationStatus::Published,
                capabilities: HarnessCapabilities::default(),
                runtime_shim_active: true,
                agent_context: None,
                context_support: HarnessContextSupport {
                    instructions: ContextMechanism::RuntimeShim,
                    project_skills: ContextMechanism::RuntimeShim,
                    project_agents: ContextMechanism::RuntimeShim,
                    project_rules: ContextMechanism::RulesEngine,
                },
            },
            HarnessHealth {
                below_minimum: None,
                integration: "codex".to_owned(),
                display_name: "Codex".to_owned(),
                description: "OpenAI's coding agent CLI".to_owned(),
                detection: HarnessDetection {
                    present: true,
                    version: Some("0.9.0".to_owned()),
                },
                setup: "not configured".to_owned(),
                strategy: None,
                provisioning: None,
                publication: PublicationStatus::NotApplicable,
                capabilities: HarnessCapabilities::default(),
                runtime_shim_active: true,
                agent_context: None,
                context_support: HarnessContextSupport {
                    instructions: ContextMechanism::RuntimeShim,
                    project_skills: ContextMechanism::RuntimeShim,
                    project_agents: ContextMechanism::RuntimeShim,
                    project_rules: ContextMechanism::RulesEngine,
                },
            },
        ],
        attachments: vec![PackageManagedState {
            hooks: Vec::new(),
            plugin: "one".to_owned(),
            state: ManagedStateSummary {
                matched: 1,
                missing: 1,
                drifted: 1,
                conflicts: 1,
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
    model.remembered.context_status = Some(ProjectContextStatus {
        root: PathBuf::from("/home/project"),
        canonical: PathBuf::from("/home/project/AGENTS.md"),
        sources: Vec::new(),
        retired_regions: None,
        harnesses: vec![
            HarnessContextStatus {
                integration: "claude-code".to_owned(),
                display_name: "Claude Code".to_owned(),
                delivery: HarnessContextDelivery::ReadsNatively,
            },
            HarnessContextStatus {
                integration: "codex".to_owned(),
                display_name: "Codex".to_owned(),
                delivery: HarnessContextDelivery::Native,
            },
        ],
        portability: Portability::Portable,
        warnings: vec![
            "AGENTS.md carries a region for a plugin that is no longer installed".to_owned(),
        ],
    });
    model.remembered.harness_screen.selected = 0;
    model.remembered.profiles = vec![
        uze_application::application::ProfileSummary {
            id: "dev-autonomous".to_owned(),
            description: Some("My daily autonomous coding setup.".to_owned()),
            active: true,
            preferences: uze_core::preference::Preferences {
                autonomy: uze_core::preference::Autonomy::Auto,
                sandbox: uze_core::preference::SandboxScope::WorkspaceWrite,
                model: uze_core::preference::ModelPreference::Default,
            },
        },
        uze_application::application::ProfileSummary {
            id: "safe-mode".to_owned(),
            description: None,
            active: false,
            preferences: uze_core::preference::Preferences::default(),
        },
    ];
    model.profile_harness_selection = ["claude-code".to_owned(), "codex".to_owned()]
        .into_iter()
        .collect();
    model.profile_harness_defaulted = true;
    model
}

pub(super) fn marketplace_plugin(
    marketplace: &str,
    name: &str,
    installed: bool,
) -> MarketplacePluginSummary {
    MarketplacePluginSummary {
        marketplace: marketplace.to_owned(),
        name: name.to_owned(),
        description: None,
        keywords: Vec::new(),
        category: None,
        installed,
        freshness: uze_application::application::Freshness::not_checked(),
        installed_at_unix: None,
        is_default: false,
    }
}

pub(super) fn registered(name: &str) -> MarketplaceSummary {
    MarketplaceSummary {
        name: name.to_owned(),
        source: format!("https://example.com/{name}"),
        homepage: None,
        plugin_count: 0,
        linked_to: None,
    }
}

pub(super) fn two_market_model() -> TuiModel {
    TuiModel {
        route: Route::Plugins,
        plugin_pane: PluginPane::Plugins,
        focus: Focus::Content,
        remembered: Remembered {
            marketplaces: vec![registered("uze-official"), registered("ai")],
            marketplace_plugins: vec![
                marketplace_plugin("uze-official", "uze", true),
                marketplace_plugin("ai", "git", true),
                marketplace_plugin("ai", "env", false),
            ],
            ..TuiModel::default().remembered
        },
        ..TuiModel::default()
    }
}

/// The buffer a model draws, kept whole — colour included, which
/// [`buffer_rows`] deliberately throws away.
pub(super) fn drawn(model: &TuiModel) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), model, false, &mut hits);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

/// All rows of the rendered buffer, right-trimmed — the cheap,
/// snapshot-free way to assert on what the TUI actually drew.
/// `model` drawn once into a `width`×`height` test terminal, with the hit
/// regions the frame registered.
pub(super) fn drawn_at(
    model: &TuiModel,
    width: u16,
    height: u16,
) -> (Terminal<TestBackend>, Vec<(Rect, Hit)>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), model, false, &mut hits);
        })
        .unwrap();
    (terminal, hits)
}

pub(super) fn buffer_rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    (area.y..area.y + area.height)
        .map(|row| {
            let mut line = String::new();
            for column in area.x..area.x + area.width {
                line.push_str(buffer[(column, row)].symbol());
            }
            line.trim_end().to_string()
        })
        .collect()
}

/// The keymap in force is process-wide, so the tests that replace it take
/// turns — otherwise one test's rebinding is another's flake.
pub(super) static KEYBOARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
