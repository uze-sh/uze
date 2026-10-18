//! The Overview: machine health and the project the client was opened in.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use uze_application::UzeHome;
use uze_application::application::{
    AnchorKind, ContextState, MarketplaceState, OverviewMarketplace, OverviewWorkspaceSummary,
    ProjectEnvironmentState, ProjectOverview,
};

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{Focus, Overlay, RefreshData, Remembered, Route, TuiModel};
use crate::ui::view::health::actionable_alerts;
use crate::ui::worker::Intent;

use super::support::*;

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
    crate::ui::worker::dispatch(
        Intent::InstallProjectEnvironment(project),
        &uze_home,
        &sender,
        &mut model,
    );
    let result = receiver.recv_timeout(Duration::from_secs(30)).unwrap();
    match result {
        crate::ui::worker::WorkerResult::Mutated(_, Ok((message, data))) => {
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
        crate::ui::worker::WorkerResult::Mutated(_, Err(error)) => {
            panic!("expected Mutated(Ok(..)), got Mutated(Err({error}))")
        }
        crate::ui::worker::WorkerResult::TrustRequired { plugin, detail, .. } => {
            panic!(
                "expected Mutated(Ok(..)), got TrustRequired {{ plugin: {plugin}, detail: {detail} }}"
            )
        }
        _ => panic!("expected Mutated(Ok(..)), got a different WorkerResult variant"),
    }

    std::fs::remove_dir_all(&base).ok();
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
