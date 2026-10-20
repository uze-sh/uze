//! L1 contract for the read-only Context Manager surface:
//! `UzeApplication::context_inspect`/`context_plan`, and for what `uze
//! status` says about the project's `AGENTS.md`, which is the operator's
//! alone.
//!
//! The central proof this file exists for: inspection and planning are
//! genuinely zero-write, in every state a real project can be in, including
//! states UZE never created (Fase 6 — a project that already has its own
//! CLAUDE.md/AGENTS.md, written entirely by hand, long before UZE ever
//! touched it).

use std::{
    fs,
    path::{Path, PathBuf},
};

use uze_application::application::{
    ContextState, HarnessContextDelivery, Portability, ProjectContextStatus, ProjectLockStatus,
};
use uze_application::{SpaceOverview, UzeApplication};
use uze_core::PackageSource;
use uze_core::{
    Result, UzeHome,
    integration::{AttachmentReceipt, HarnessDetection, IntegrationPort},
    router::HarnessCapabilities,
};

// --- uze status: thin composition, distinct scope from doctor -------------

#[test]
fn status_reports_healthy_with_zero_issues() {
    let root = temp("status-healthy");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(project.join("AGENTS.md"), "# Mine\n").unwrap();

    let status = application.health().status(&project).unwrap();
    assert!(matches!(status.portability, Portability::Portable));
    assert_eq!(status.packages_installed, 1);
    assert!(status.issues.is_empty());
    assert!(!status.drift.stale_projection);
}

#[test]
fn status_names_a_region_an_earlier_release_left_as_owing_install() {
    let root = temp("status-retired-region");
    let application = app(&root, true);
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    left_by_an_earlier_release(&project);

    let status = application.health().status(&project).unwrap();
    assert!(status.drift.stale_projection, "{:?}", status.drift);
    assert!(status.instructions.exists);
}

/// The space overview shows what `uze status` says, never a second
/// reading of it: one project, one installed plugin it locks, one it locks
/// and the machine lacks.
#[test]
fn the_space_overview_agrees_with_status() {
    let root = temp("space-overview-agrees");
    let application = app(&root, true);
    install(&application, fixture_a());
    let installed = application.plugins().list().unwrap().remove(0);
    let (name, market) = installed.id.split_once('@').expect("a qualified id");
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(project.join("AGENTS.md"), "# Mine\n").unwrap();
    fs::write(
        project.join("agents.lock"),
        format!("version: 1\nplugins:\n  {name}:\n    marketplace: {market}\n  absent:\n    marketplace: {market}\n"),
    )
    .unwrap();

    let status = application.health().project_status(&project).unwrap();
    let SpaceOverview::Project(space) = application.workspace().space_overview(&project).unwrap()
    else {
        panic!("a project answers with its overview");
    };

    let context_of = |status: &uze_application::application::ProjectStatus| {
        serde_json::to_value((
            &status.report.instructions,
            &status.report.portability,
            &status.report.harnesses,
            &status.report.issues,
        ))
        .unwrap()
    };
    assert_eq!(context_of(&space.status), context_of(&status));
    let summary = application.project().summary(&project).unwrap();
    assert_eq!(
        summary.project.context == ContextState::Ready,
        matches!(status.report.portability, Portability::Portable)
    );

    let ProjectLockStatus::Present { plugins: locked } = &status.report.project_lock else {
        panic!("the lock reads");
    };
    assert_eq!(space.plugins.plugins.len(), locked.len());
    for (row, health) in space.plugins.plugins.iter().zip(locked) {
        assert_eq!(row.plugin, health.plugin);
        assert_eq!(row.installed, health.installed);
        assert_eq!(row.differs_from_lock, health.differs_from_lock);
        assert_eq!(row.freshness.is_some(), row.installed);
    }
    assert!(
        space
            .plugins
            .plugins
            .iter()
            .any(|row| row.package == installed.id && row.installed)
    );
    assert!(
        space
            .plugins
            .plugins
            .iter()
            .any(|row| row.plugin == "absent" && !row.installed)
    );
    assert_eq!(
        serde_json::to_value(&space.plugins.drift).unwrap(),
        serde_json::to_value(&status.report.drift).unwrap()
    );
}

#[test]
fn a_space_with_no_project_or_no_root_is_answered_as_such() {
    let root = temp("space-overview-absent");
    let application = app(&root, true);
    let bare = root.join("bare");
    fs::create_dir_all(&bare).unwrap();

    assert!(matches!(
        application.workspace().space_overview(&bare).unwrap(),
        SpaceOverview::NotAProject { .. }
    ));
    assert!(matches!(
        application
            .workspace()
            .space_overview(&root.join("gone"))
            .unwrap(),
        SpaceOverview::DirectoryGone { .. }
    ));
}

fn temp(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

fn fixture_a() -> PathBuf {
    uze_testkit::fixtures::canonical("instructions-a")
}

struct StubBridgeHarness {
    stub_id: &'static str,
    present: bool,
}

impl IntegrationPort for StubBridgeHarness {
    fn id(&self) -> &'static str {
        self.stub_id
    }

    /// The one bridged harness in v0 declares its bridge like any real
    /// integration would — the Application reads `context_delivery`, never
    /// a vendor name.
    fn context_delivery(&self) -> uze_core::integration::ContextDelivery {
        uze_core::integration::ContextDelivery::Bridge {
            file_name: "CLAUDE.md",
        }
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }
    fn detect(&self) -> HarnessDetection {
        HarnessDetection {
            present: self.present,
            version: None,
        }
    }
    fn exposure_plan(&self, _resource: &uze_core::Resource) -> uze_core::exposure::ExposurePlan {
        uze_core::exposure::ExposurePlan {
            route: uze_core::router::CompatibilityRoute::Unsupported,
            mechanism: uze_core::exposure::ExposureMechanism::Unsupported {
                rationale: "test stub attaches nothing".to_owned(),
            },
            evidence: "test stub".to_owned(),
        }
    }
    fn attach_receipt(&self, _resource: &uze_core::Resource) -> Result<Option<AttachmentReceipt>> {
        Ok(None)
    }
}

/// Claude Code is the one bridged harness in v0 (every other v0 harness
/// reads `AGENTS.md` natively); this stub stands in for it.
fn app(root: &Path, claude_present: bool) -> UzeApplication {
    UzeApplication::new(
        UzeHome::at(root.join("uze-home")),
        vec![Box::new(StubBridgeHarness {
            stub_id: "claude-code",
            present: claude_present,
        })],
    )
}

fn install(app: &UzeApplication, path: PathBuf) {
    app.plugins()
        .add(PackageSource::local(path), &uze_core::trust::AlwaysTrust)
        .expect("fixture installs cleanly");
}

/// Every byte, of every file, anywhere under `dir` — the strongest
/// "nothing changed" proof available: not just AGENTS.md, not just the
/// files this suite happens to check, but the whole directory tree.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.filter_map(std::result::Result::ok).collect();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(bytes) = fs::read(&path) {
                out.push((path.strip_prefix(root).unwrap().to_path_buf(), bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

/// What an earlier release left in `project`: a package region in
/// `AGENTS.md` beside the operator's text, and the bridge in `CLAUDE.md`.
fn left_by_an_earlier_release(project: &Path) {
    fs::write(
        project.join("AGENTS.md"),
        "# Mine\n\n<!-- uze:begin package:greet:tools:instructions -->\nSay hello.\n\
         <!-- uze:end package:greet:tools:instructions -->\n",
    )
    .unwrap();
    fs::write(
        project.join("CLAUDE.md"),
        "<!-- uze:begin instruction-bridge -->\n@AGENTS.md\n<!-- uze:end instruction-bridge -->\n",
    )
    .unwrap();
}

fn harness_delivery<'a>(
    status: &'a ProjectContextStatus,
    integration: &str,
) -> &'a HarnessContextDelivery {
    &status
        .harnesses
        .iter()
        .find(|harness| harness.integration == integration)
        .unwrap_or_else(|| panic!("no harness status for {integration}"))
        .delivery
}

// --- Fase 2: inspect/plan are genuinely zero-write -------------------------

#[test]
fn context_inspect_never_writes_anything_in_a_populated_project() {
    let root = temp("inspect-snapshot");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    left_by_an_earlier_release(&project);

    let before = snapshot(&project);
    let status = application.context().inspect(&project).unwrap();
    let after = snapshot(&project);
    assert_eq!(
        before, after,
        "context_inspect must not create, delete, or modify any file"
    );
    let retired = status
        .retired_regions
        .expect("the old regions are reported");
    assert_eq!(retired.remove, vec!["package:greet:tools:instructions"]);
    assert_eq!(retired.bridges.len(), 1);
}

#[test]
fn context_plan_never_writes_anything() {
    let root = temp("plan-snapshot");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    left_by_an_earlier_release(&project);

    // Before any reconcile at all — the state with the most to remove, and
    // therefore the state most tempting to accidentally write.
    let before = snapshot(&project);
    let plan = application.context().plan(&project).unwrap();
    let after = snapshot(&project);
    assert_eq!(
        before, after,
        "context_plan must not write anything even with everything to remove"
    );
    assert!(plan.has_changes());
}

// --- Fase 3: portability observation ---------------------------------------

#[test]
fn a_project_with_only_claude_md_is_vendor_locked() {
    let root = temp("claude-only");
    let application = app(&root, true);
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(project.join("CLAUDE.md"), "# My Claude-only instructions\n").unwrap();

    let before = snapshot(&project);
    let status = application.context().inspect(&project).unwrap();
    assert_eq!(snapshot(&project), before, "inspect must not write");

    assert!(matches!(
        status.portability,
        Portability::VendorLocked { .. }
    ));
    if let Portability::VendorLocked { files } = &status.portability {
        assert_eq!(files, &[project.join("CLAUDE.md")]);
    }
}

#[test]
fn agents_md_alone_is_portable_and_claude_reads_it_with_no_bridge() {
    let root = temp("portable");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(project.join("AGENTS.md"), "# Mine\n").unwrap();

    let status = application.context().inspect(&project).unwrap();
    assert!(matches!(status.portability, Portability::Portable));
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::ReadsNatively
    ));
    assert!(!project.join("CLAUDE.md").exists());
}

#[test]
fn an_absent_bridge_harness_shows_not_detected_not_a_gap() {
    let root = temp("not-detected");
    let application = app(&root, false); // Claude Code absent
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(project.join("AGENTS.md"), "# Mine\n").unwrap();

    let status = application.context().inspect(&project).unwrap();
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::NotDetected
    ));
    // An absent harness is never a gap in the project's portability.
    assert!(matches!(status.portability, Portability::Portable));
}

// --- Fase 6: real projects with pre-existing, hand-written files ----------

/// A) CLAUDE.md with manual content, no AGENTS.md.
#[test]
fn scenario_a_manual_claude_md_survives_untouched() {
    let root = temp("scenario-a");
    let application = app(&root, true);
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(
        project.join("CLAUDE.md"),
        "My hand-written Claude instructions.\n",
    )
    .unwrap();

    let before = fs::read(project.join("CLAUDE.md")).unwrap();
    application.context().inspect(&project).unwrap();
    assert_eq!(fs::read(project.join("CLAUDE.md")).unwrap(), before);

    let status = application.context().inspect(&project).unwrap();
    let claude_source = status
        .sources
        .iter()
        .find(|s| s.file_name == "CLAUDE.md")
        .unwrap();
    assert!(claude_source.has_user_content);
    assert!(claude_source.managed_region_identities.is_empty());
}

/// C) An unrecognized manual vendor file (not a UZE-bridged surface) is
/// left alone by both inspection and planning.
#[test]
fn scenario_c_unrecognized_vendor_file_survives_untouched() {
    let root = temp("scenario-c");
    let application = app(&root, true);
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(
        project.join("VENDOR-NOTES.md"),
        "Notes, unrelated to UZE.\n",
    )
    .unwrap();

    let before = snapshot(&project);
    application.context().inspect(&project).unwrap();
    application.context().plan(&project).unwrap();
    assert_eq!(snapshot(&project), before);
}

/// D) AGENTS.md manual, already existing, no packages installed.
#[test]
fn scenario_d_manual_agents_md_with_no_packages_is_left_alone() {
    let root = temp("scenario-d");
    let application = app(&root, false);
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    fs::write(project.join("AGENTS.md"), "My own project conventions.\n").unwrap();

    let before = fs::read(project.join("AGENTS.md")).unwrap();
    let status = application.context().inspect(&project).unwrap();
    assert_eq!(fs::read(project.join("AGENTS.md")).unwrap(), before);
    let agents_source = status
        .sources
        .iter()
        .find(|s| s.file_name == "AGENTS.md")
        .unwrap();
    assert!(agents_source.has_user_content);
    assert!(agents_source.managed_region_identities.is_empty());
    assert!(status.retired_regions.is_none());

    // Reconcile must also leave it alone: there is nothing to take out.
    application.context().reconcile(&project).unwrap();
    assert_eq!(fs::read(project.join("AGENTS.md")).unwrap(), before);
}

/// E) A manual AGENTS.md in a project declaring a plugin that still ships
/// one of its own: reconcile adds nothing to it.
#[test]
fn scenario_e_a_declared_plugins_agents_md_never_reaches_the_project() {
    let root = temp("scenario-e");
    let application = app(&root, false);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(
        project.join("AGENTS.md"),
        "# My own conventions\n\nAlways write tests.\n",
    )
    .unwrap();

    application.context().reconcile(&project).unwrap();
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        "# My own conventions\n\nAlways write tests.\n"
    );
}

/// F) A manual CLAUDE.md is the operator's: reconcile adds no bridge to
/// it, and creates no AGENTS.md beside it.
#[test]
fn scenario_f_manual_claude_md_gets_no_bridge() {
    let root = temp("scenario-f");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(
        project.join("CLAUDE.md"),
        "## My personal Claude workflow notes\n",
    )
    .unwrap();

    application.context().reconcile(&project).unwrap();
    assert_eq!(
        fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
        "## My personal Claude workflow notes\n"
    );
    assert!(!project.join("AGENTS.md").exists());
}

/// Both recognized files present at once: exactly the "everything
/// together" state Fase 10 asks for, on top of the per-scenario A–F
/// coverage above.
#[test]
fn all_recognized_files_together_are_fully_portable() {
    let root = temp("all-recognized");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);
    fs::write(project.join("AGENTS.md"), "# Mine\n").unwrap();
    fs::write(project.join("CLAUDE.md"), "## My Claude workflow notes\n").unwrap();

    application.context().reconcile(&project).unwrap();
    let status = application.context().inspect(&project).unwrap();

    assert!(matches!(status.portability, Portability::Portable));
    for file_name in ["AGENTS.md", "CLAUDE.md"] {
        let source = status
            .sources
            .iter()
            .find(|s| s.file_name == file_name)
            .unwrap();
        assert!(source.exists);
    }
    assert!(
        fs::read_to_string(project.join("CLAUDE.md"))
            .unwrap()
            .contains("My Claude workflow notes")
    );
    // One warning expected: CLAUDE.md carries content of its own, which is
    // legitimate and disclosed, not a gap.
    assert_eq!(status.warnings.len(), 1);
}

// --- context operations never touch the Store -------------------------------

#[test]
fn context_operations_never_alter_the_installed_package_set() {
    let root = temp("store-untouched");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    uze_testkit::git::mark_repository(&project);
    declare_fixtures(&project);

    let before = application.plugins().list().unwrap();
    application.context().inspect(&project).unwrap();
    application.context().plan(&project).unwrap();
    application.context().reconcile(&project).unwrap();
    let after = application.plugins().list().unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before[0].id, after[0].id);
}

/// Declares the plugin that still ships an `AGENTS.md` in the project's
/// `agents.yaml`.
fn declare_fixtures(project: &Path) {
    uze_testkit::project::declare(project, &["uze-instructions-fixture-a@local"]);
}
