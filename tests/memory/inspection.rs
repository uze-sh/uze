//! L1 contract for the read-only Context Manager surface:
//! `UzeApplication::context_inspect`/`context_plan`.
//!
//! The central proof this file exists for: inspection and planning are
//! genuinely zero-write, in every state a real project can be in, including
//! states UZE never created (Fase 6 — a project that already has its own
//! CLAUDE.md/AGENTS.md, written entirely by hand, long before UZE ever
//! touched it).
//!
//! The harness here stands in for Claude Code: it reads `AGENTS.md` itself
//! unless the project's own `CLAUDE.md` carries content, which it then
//! reads instead — a gap UZE reports and never writes its way out of.

use std::{
    fs,
    path::{Path, PathBuf},
};

use uze_application::UzeApplication;
use uze_application::application::{HarnessContextDelivery, Portability, ProjectContextStatus};
use uze_core::PackageSource;
use uze_core::{
    Result, UzeHome,
    integration::{AttachmentReceipt, AttachmentState, HarnessDetection, IntegrationPort},
    router::HarnessCapabilities,
};

// --- uze status: thin composition, distinct scope from doctor -------------

#[test]
fn status_reports_healthy_with_zero_issues_once_reconciled() {
    let root = temp("status-healthy");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    application.context().reconcile(&project).unwrap();

    let status = application.health().status(&project).unwrap();
    assert!(matches!(status.portability, Portability::Portable));
    assert_eq!(status.packages_installed, 1);
    assert_eq!(status.packages_contributing_here, 1);
    assert!(status.issues.is_empty());
}

#[test]
fn status_surfaces_an_unreconciled_contribution_as_an_issue() {
    let root = temp("status-issue");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();

    let status = application.health().status(&project).unwrap();
    assert!(!status.issues.is_empty());
    assert!(status.issues.iter().any(|issue| issue.contains("Missing")));
}

#[test]
fn status_surfaces_a_claude_md_read_in_place_of_agents_md() {
    let root = temp("status-shadowed");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(project.join("CLAUDE.md"), "## My Claude workflow notes\n").unwrap();
    application.context().reconcile(&project).unwrap();

    let status = application.health().status(&project).unwrap();
    assert!(
        status
            .issues
            .iter()
            .any(|issue| issue.contains("reads CLAUDE.md instead of AGENTS.md")),
        "{:?}",
        status.issues
    );
}

#[test]
fn status_distinguishes_installed_from_contributing_here() {
    let root = temp("status-counts");
    let application = app(&root, false);
    install(&application, fixture_a());
    // A second, Skill-only package: installed globally, but contributes no
    // Instruction resource, so it must not count as "contributing here".
    install(
        &application,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/uze"),
    );
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    application.context().reconcile(&project).unwrap();

    let status = application.health().status(&project).unwrap();
    assert_eq!(status.packages_installed, 2);
    assert_eq!(status.packages_contributing_here, 1);
}

fn temp(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

fn fixture_a() -> PathBuf {
    uze_testkit::fixtures::canonical("instructions-a")
}

struct StubShadowableHarness {
    stub_id: &'static str,
    present: bool,
}

impl IntegrationPort for StubShadowableHarness {
    fn id(&self) -> &'static str {
        self.stub_id
    }

    /// Declared like any real integration would — the Application reads
    /// `context_delivery`, never a vendor name.
    fn context_delivery(&self) -> uze_core::integration::ContextDelivery {
        uze_core::integration::ContextDelivery::Native {
            files: &[],
            shadowed_by: &["CLAUDE.md"],
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

fn app(root: &Path, claude_present: bool) -> UzeApplication {
    UzeApplication::new(
        UzeHome::at(root.join("uze-home")),
        vec![Box::new(StubShadowableHarness {
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
    fs::create_dir_all(project.join(".git")).unwrap();
    // Reconcile once so there's real managed state to inspect.
    application.context().reconcile(&project).unwrap();

    let before = snapshot(&project);
    let status = application.context().inspect(&project).unwrap();
    let after = snapshot(&project);
    assert_eq!(
        before, after,
        "context_inspect must not create, delete, or modify any file"
    );
    assert_eq!(status.contributions[0].state, AttachmentState::Matched);
}

#[test]
fn context_plan_never_writes_anything() {
    let root = temp("plan-snapshot");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();

    // Before any reconcile at all — the state with the most "would create"
    // actions, and therefore the state most tempting to accidentally write.
    let before = snapshot(&project);
    let plan = application.context().plan(&project).unwrap();
    let after = snapshot(&project);
    assert_eq!(
        before, after,
        "context_plan must not write anything even when everything is missing"
    );
    assert!(plan.has_changes());
}

// --- Fase 3: portability observation ---------------------------------------

#[test]
fn a_project_with_only_claude_md_is_vendor_locked() {
    let root = temp("claude-only");
    let application = app(&root, true);
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
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
fn agents_md_alone_is_read_natively_and_reconcile_writes_no_claude_md() {
    let root = temp("portable");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    application.context().reconcile(&project).unwrap();

    assert!(!project.join("CLAUDE.md").exists());
    let status = application.context().inspect(&project).unwrap();
    assert!(matches!(status.portability, Portability::Portable));
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::Native
    ));
}

/// An empty file is no instructions at all: it takes nothing's place.
#[test]
fn an_empty_claude_md_does_not_shadow_agents_md() {
    let root = temp("empty-claude-md");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(project.join("CLAUDE.md"), "\n").unwrap();
    application.context().reconcile(&project).unwrap();

    let status = application.context().inspect(&project).unwrap();
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::Native
    ));
}

#[test]
fn an_absent_bridge_harness_shows_not_detected_not_a_gap() {
    let root = temp("not-detected");
    let application = app(&root, false); // Claude Code absent
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    application.context().reconcile(&project).unwrap();

    let status = application.context().inspect(&project).unwrap();
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::NotDetected
    ));
    // An absent harness must never turn Portable into PartiallyPortable.
    assert!(matches!(status.portability, Portability::Portable));
}

// --- Fase 6: real projects with pre-existing, hand-written files ----------

/// A) CLAUDE.md with manual content, no AGENTS.md.
#[test]
fn scenario_a_manual_claude_md_survives_untouched() {
    let root = temp("scenario-a");
    let application = app(&root, true);
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
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
    fs::create_dir_all(project.join(".git")).unwrap();
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
    fs::create_dir_all(project.join(".git")).unwrap();
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
    assert!(status.contributions.is_empty());

    // Reconcile must also leave it alone: no packages means nothing to add.
    application.context().reconcile(&project).unwrap();
    assert_eq!(fs::read(project.join("AGENTS.md")).unwrap(), before);
}

/// E) AGENTS.md manual + UZE regions coexisting.
#[test]
fn scenario_e_manual_agents_md_plus_uze_region_coexist() {
    let root = temp("scenario-e");
    let application = app(&root, false);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(
        project.join("AGENTS.md"),
        "# My own conventions\n\nAlways write tests.\n",
    )
    .unwrap();

    application.context().reconcile(&project).unwrap();
    let content = fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(content.starts_with("# My own conventions\n\nAlways write tests.\n"));
    assert!(content.contains("uze-instructions-fixture-a"));

    let status = application.context().inspect(&project).unwrap();
    let agents_source = status
        .sources
        .iter()
        .find(|s| s.file_name == "AGENTS.md")
        .unwrap();
    assert!(
        agents_source.has_user_content,
        "manual conventions are still there"
    );
    assert_eq!(agents_source.managed_region_identities.len(), 1);
}

/// F) CLAUDE.md manual + AGENTS.md: the harness reads CLAUDE.md in its
/// place, and nothing UZE runs writes into it or moves its prose.
#[test]
fn scenario_f_a_claude_md_with_content_shadows_agents_md_and_stays_untouched() {
    let root = temp("scenario-f");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(
        project.join("CLAUDE.md"),
        "## My personal Claude workflow notes\n",
    )
    .unwrap();

    let before = fs::read(project.join("CLAUDE.md")).unwrap();
    application.context().reconcile(&project).unwrap();
    assert_eq!(fs::read(project.join("CLAUDE.md")).unwrap(), before);
    let agents_content = fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(!agents_content.contains("personal Claude workflow"));

    let status = application.context().inspect(&project).unwrap();
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::ShadowedBy { file } if file == &project.join("CLAUDE.md")
    ));
    let Portability::PartiallyPortable { gaps } = &status.portability else {
        panic!("expected a gap, got {:?}", status.portability);
    };
    assert_eq!(gaps, &["claude-code: reads CLAUDE.md instead of AGENTS.md"]);
    // The gap is said once, as a gap — never again as "expected and
    // supported" vendor content.
    assert!(status.warnings.is_empty(), "{:?}", status.warnings);
}

/// A CLAUDE.md that imports AGENTS.md delivers it: vendor-specific notes
/// beside the portable baseline are supported, and disclosed, not a gap.
#[test]
fn a_claude_md_importing_agents_md_is_fully_portable() {
    let root = temp("all-recognized");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(
        project.join("CLAUDE.md"),
        "## My Claude workflow notes\n\n@AGENTS.md\n",
    )
    .unwrap();

    application.context().reconcile(&project).unwrap();
    let status = application.context().inspect(&project).unwrap();

    assert!(matches!(status.portability, Portability::Portable));
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::Native
    ));
    for file_name in ["AGENTS.md", "CLAUDE.md"] {
        let source = status
            .sources
            .iter()
            .find(|s| s.file_name == file_name)
            .unwrap();
        assert!(source.exists);
    }
    assert_eq!(status.warnings.len(), 1);
}

/// The region earlier builds wrote into CLAUDE.md is an import like any
/// other: a project still carrying it keeps its context delivered.
#[test]
fn a_claude_md_holding_an_earlier_builds_import_region_still_delivers() {
    let root = temp("legacy-import-region");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(
        project.join("CLAUDE.md"),
        "<!-- uze:begin instruction-bridge -->\n@AGENTS.md\n<!-- uze:end instruction-bridge -->\n",
    )
    .unwrap();

    application.context().reconcile(&project).unwrap();
    let status = application.context().inspect(&project).unwrap();
    assert!(matches!(status.portability, Portability::Portable));
    assert!(matches!(
        harness_delivery(&status, "claude-code"),
        HarnessContextDelivery::Native
    ));
}

// --- context operations never touch the Store -------------------------------

#[test]
fn context_operations_never_alter_the_installed_package_set() {
    let root = temp("store-untouched");
    let application = app(&root, true);
    install(&application, fixture_a());
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();

    let before = application.plugins().list().unwrap();
    application.context().inspect(&project).unwrap();
    application.context().plan(&project).unwrap();
    application.context().reconcile(&project).unwrap();
    let after = application.plugins().list().unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before[0].id, after[0].id);
}
