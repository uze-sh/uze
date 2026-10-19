//! L1 contract: `uze agent context reconcile` writes nothing into a
//! project's `AGENTS.md`. What it does is take out, once and in a main
//! working tree only, what an earlier release wrote there: a package's
//! instructions region, and the `@AGENTS.md` bridge in a harness's own file
//! that went with it.
//!
//! Deterministic by construction: the bridge-declaring integration here is
//! a test double with a controllable `detect()`, not the real CLI.

use std::{
    fs,
    path::{Path, PathBuf},
};

use uze_application::UzeApplication;
use uze_application::application::BridgeFate;
use uze_core::PackageSource;
use uze_core::{
    Result, UzeError, UzeHome,
    integration::{AttachmentReceipt, HarnessDetection, IntegrationPort},
    router::HarnessCapabilities,
    text_region,
};

fn temp(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

/// A plugin that still ships a root `AGENTS.md`, as the instructions
/// capability once read.
fn plugin_shipping_agents_md() -> PathBuf {
    uze_testkit::fixtures::canonical("instructions-a")
}

/// A stand-in for the one harness that declares a file of its own beside
/// `AGENTS.md` (Claude Code), present or absent on demand.
struct StubBridgeHarness {
    present: bool,
}

impl IntegrationPort for StubBridgeHarness {
    fn id(&self) -> &'static str {
        "claude-code"
    }

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

fn app(root: &Path, claude_present: bool) -> UzeApplication {
    UzeApplication::new(
        UzeHome::at(root.join("uze-home")),
        vec![Box::new(StubBridgeHarness {
            present: claude_present,
        })],
    )
}

/// A main working tree: `.git` is a directory.
fn primary(root: &Path) -> PathBuf {
    let project = root.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    project
}

const OPERATOR: &str = "# My project\n\nRun the tests before you commit.\n";

/// What an earlier release left: the operator's text, one package region
/// after it, and the bridge in `CLAUDE.md` after `claude_md`, each written
/// the way that release wrote them.
fn left_by_an_earlier_release(project: &Path, claude_md: &str) {
    let agents_md = project.join("AGENTS.md");
    fs::write(&agents_md, OPERATOR).unwrap();
    text_region::attach(
        &agents_md,
        "package:greet:tools:instructions",
        "Say hello.\n",
    )
    .unwrap();
    let bridge = project.join("CLAUDE.md");
    if !claude_md.is_empty() {
        fs::write(&bridge, claude_md).unwrap();
    }
    text_region::attach(&bridge, "instruction-bridge", "@AGENTS.md").unwrap();
}

#[test]
fn a_declared_plugin_shipping_agents_md_adds_nothing_to_the_project() {
    let root = temp("plugin-agents-md-adds-nothing");
    let application = app(&root, true);
    application
        .plugins()
        .add(
            PackageSource::local(plugin_shipping_agents_md()),
            &uze_core::trust::AlwaysTrust,
        )
        .expect("fixture installs cleanly");
    let project = primary(&root);
    uze_testkit::project::declare(&project, &["uze-instructions-fixture-a@local"]);
    fs::write(project.join("AGENTS.md"), OPERATOR).unwrap();

    let report = application.context().reconcile(&project).unwrap();
    assert!(report.retired_regions.is_none(), "{report:?}");
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        OPERATOR
    );
    assert!(!project.join("CLAUDE.md").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_project_without_agents_md_gets_none() {
    let root = temp("no-agents-md-stays-none");
    let application = app(&root, true);
    let project = primary(&root);

    let report = application.context().reconcile(&project).unwrap();
    assert!(report.retired_regions.is_none());
    assert!(!project.join("AGENTS.md").exists());
    assert!(!project.join("CLAUDE.md").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_package_region_and_a_bridge_only_claude_md_go_together() {
    let root = temp("package-region-and-bridge-go");
    let application = app(&root, true);
    let project = primary(&root);
    left_by_an_earlier_release(&project, "");

    let plan = application.context().plan(&project).unwrap();
    assert!(plan.has_changes());
    let report = application.context().reconcile(&project).unwrap();
    let regions = report.retired_regions.expect("the old regions were found");
    assert_eq!(regions.removed, vec!["package:greet:tools:instructions"]);
    assert_eq!(regions.bridges[0].fate, BridgeFate::File);
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        OPERATOR,
        "the operator's text is byte-identical"
    );
    assert!(!project.join("CLAUDE.md").exists());
    assert!(
        !application.context().plan(&project).unwrap().has_changes(),
        "a second pass finds nothing"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_operators_lines_in_claude_md_stay_when_the_bridge_goes() {
    let root = temp("bridge-goes-operator-lines-stay");
    let application = app(&root, false);
    let project = primary(&root);
    left_by_an_earlier_release(&project, "# Claude only\n\nPrefer small diffs.\n\n");

    let report = application.context().reconcile(&project).unwrap();
    let regions = report.retired_regions.unwrap();
    assert_eq!(
        regions.bridges[0].fate,
        BridgeFate::Region,
        "a harness that is not detected still has its leftover taken out"
    );
    let claude_md = fs::read_to_string(project.join("CLAUDE.md")).unwrap();
    assert!(claude_md.contains("Prefer small diffs."), "{claude_md}");
    assert!(!claude_md.contains("@AGENTS.md"), "{claude_md}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_linked_worktree_is_left_as_its_branch_has_it() {
    let root = temp("linked-worktree-untouched");
    let application = app(&root, true);
    let slot = root.join("project");
    fs::create_dir_all(&slot).unwrap();
    fs::write(slot.join(".git"), "gitdir: ../.git/worktrees/ab12cd\n").unwrap();
    left_by_an_earlier_release(&slot, "");
    let agents_md = fs::read(slot.join("AGENTS.md")).unwrap();
    let claude_md = fs::read(slot.join("CLAUDE.md")).unwrap();

    let report = application.context().reconcile(&slot).unwrap();
    assert!(report.retired_regions.is_none());
    assert!(!application.context().plan(&slot).unwrap().has_changes());
    assert_eq!(fs::read(slot.join("AGENTS.md")).unwrap(), agents_md);
    assert_eq!(fs::read(slot.join("CLAUDE.md")).unwrap(), claude_md);
    fs::remove_dir_all(root).unwrap();
}

/// `uze install` in a slot leaves the regions its branch was cut with: the
/// removal belongs to the primary's commit, not to every branch.
#[test]
fn install_in_a_linked_worktree_leaves_its_retired_regions_as_committed() {
    let root = temp("linked-worktree-install");
    let application = app(&root, true);
    let slot = root.join("project");
    fs::create_dir_all(&slot).unwrap();
    fs::write(slot.join(".git"), "gitdir: ../.git/worktrees/ab12cd\n").unwrap();
    fs::write(slot.join("agents.yaml"), "workspace: {}\n").unwrap();
    left_by_an_earlier_release(&slot, "");
    let agents_md = fs::read(slot.join("AGENTS.md")).unwrap();
    let claude_md = fs::read(slot.join("CLAUDE.md")).unwrap();

    application
        .project()
        .install(&slot, &uze_core::trust::AlwaysTrust)
        .unwrap();
    assert_eq!(fs::read(slot.join("AGENTS.md")).unwrap(), agents_md);
    assert_eq!(fs::read(slot.join("CLAUDE.md")).unwrap(), claude_md);
    assert!(
        application
            .project()
            .plan(&slot)
            .unwrap()
            .stale_projection
            .is_none(),
        "a slot is not told to remove what its branch carries"
    );
    fs::remove_dir_all(root).unwrap();
}

/// The cleanup does not read the declaration, so an `agents.yaml` mid-edit
/// neither blocks it nor changes what it takes out.
#[test]
fn a_manifest_that_does_not_parse_does_not_change_the_cleanup() {
    let root = temp("unreadable-manifest-cleanup");
    let application = app(&root, false);
    let project = primary(&root);
    left_by_an_earlier_release(&project, "");
    fs::write(project.join("agents.yaml"), "marketplaces: [oops\n").unwrap();

    application.context().reconcile(&project).unwrap();
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        OPERATOR
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_project_root_that_does_not_exist_is_a_clean_error_not_a_write() {
    let root = temp("missing-project");
    let application = app(&root, false);
    let error = application
        .context()
        .reconcile(&root.join("does-not-exist"))
        .unwrap_err();
    assert!(matches!(error, UzeError::NotDirectory(_)));
}

/// A well-formed marker for an identity no release of UZE wrote belongs to
/// something else, and the cleanup does not assume it.
#[test]
fn a_foreign_looking_managed_region_is_left_untouched() {
    let root = temp("foreign-region");
    let application = app(&root, false);
    let project = primary(&root);
    let foreign =
        "<!-- uze:begin some-other-concern -->\nnot ours\n<!-- uze:end some-other-concern -->\n";
    fs::write(project.join("AGENTS.md"), foreign).unwrap();

    let report = application.context().reconcile(&project).unwrap();
    assert!(report.retired_regions.is_none());
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        foreign
    );
    fs::remove_dir_all(root).unwrap();
}
