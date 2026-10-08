//! Worktree-policy projection: one declaration in `agents.yaml`, rendered
//! into the shared instruction baseline every harness already reads.
//!
//! Isolation itself is not tested here — UZE performs it by choosing an
//! agent's working directory at launch, which is harness-blind. What these
//! cover is the remainder that travels as text: the layout a subagent needs
//! to reproduce, the sentence that stops an already-isolated agent from
//! isolating again, and the completion rule.
//!
//! The region is the workspace's. The workspace keeps it in step in the
//! primary checkout; the package manager never writes, removes or counts
//! it, which the last section holds.

use std::{fs, path::Path, path::PathBuf};

use uze_application::UzeApplication;
use uze_core::{UzeHome, authoring::region as authoring_region, integration::AttachmentState};
use uze_integrations::registry::IntegrationRegistry;
use uze_testkit::git::Repository;
use uze_workspace::worktree::{self, CompletionBehavior};

/// The real four integrations, against isolated roots.
fn app(root: &Path) -> UzeApplication {
    let home = UzeHome::at(root.join("uze-home"));
    let registry = IntegrationRegistry::isolated(&root.join("harnesses"), &home);
    UzeApplication::new(home, registry.into_parts().0)
}

/// A repository whose primary checkout declares `manifest` and keeps the
/// `AGENTS.md` a project with instructions has.
fn project_with_policy(label: &str, manifest: &str) -> (Repository, PathBuf) {
    let repository = Repository::new(label);
    repository.commit_file("AGENTS.md", "# Project\n");
    let project = repository.root().to_path_buf();
    fs::write(project.join("agents.yaml"), manifest).unwrap();
    (repository, project)
}

/// The policy declared with every field left to its default — the shape
/// that proves projection does not depend on any of them being set.
const POLICY_MANIFEST: &str = "workspace: {}\n";

fn agents_md(project: &Path) -> String {
    fs::read_to_string(project.join("AGENTS.md")).unwrap_or_default()
}

// --- projection into the shared baseline ----------------------------------

#[test]
fn the_workspace_projects_the_declaration_into_the_shared_baseline() {
    let (repository, project) = project_with_policy("worktree-projection", POLICY_MANIFEST);
    let application = app(repository.root());

    let region = application
        .workspace()
        .sync_policy_region(&project)
        .unwrap()
        .expect("a repository answers");
    assert_eq!(region.state, AttachmentState::Matched);

    let written = agents_md(&project);
    assert!(
        written.contains(worktree::POLICY_REGION_PREFIX),
        "the region is marker-owned, not free text"
    );
    assert!(written.contains(worktree::WORKTREES_DIRECTORY));
    assert!(written.contains(worktree::BRANCH_PREFIX));

    // Idempotent: a second pass changes nothing and stays matched.
    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();
    assert_eq!(agents_md(&project), written);
}

/// The projected text must never ask for a top-level worktree. A harness
/// with its own worktree primitive activates on exactly that instruction and
/// would isolate a second time on top of the checkout UZE already placed the
/// agent in — the nesting this whole design avoids.
#[test]
fn the_projection_never_triggers_a_harnesss_own_isolation() {
    let (repository, project) =
        project_with_policy("worktree-no-double-isolation", POLICY_MANIFEST);
    let application = app(repository.root());
    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();

    let written = agents_md(&project).to_lowercase();
    assert!(!written.contains("before editing"), "{written}");
    assert!(!written.contains("create or reuse"), "{written}");
    assert!(
        written.contains("already isolated"),
        "an agent UZE placed must be told it is already isolated"
    );
    assert!(
        written.contains("operator's own checkout"),
        "an agent placed on the operator's branch must be told where it is: {written}"
    );
}

/// An agent a person started by hand reads the same file. The region says
/// before anything else that it is not for that agent, so the first command
/// it names is never one that refuses outside the workspace.
#[test]
fn an_agent_started_by_hand_is_told_first_that_the_region_is_not_for_it() {
    let (repository, project) = project_with_policy(
        "worktree-hand-started",
        "workspace:\n  branch: conventional\n",
    );
    let application = app(repository.root());
    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();

    let written = agents_md(&project);
    let statement = written
        .find("An agent started any other way can ignore it")
        .expect("the region says who it is for");
    let first_command = written
        .find("`uze agent work")
        .expect("the naming clause is projected");
    assert!(statement < first_command, "{written}");
    assert!(
        !written.contains("uze agent plugin create"),
        "authoring is the package manager's, not this region's: {written}"
    );
}

#[test]
fn a_project_declaring_nothing_gets_the_conventional_region() {
    let (repository, project) = project_with_policy("worktree-absent", "");
    let application = app(repository.root());

    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();
    assert!(agents_md(&project).contains(worktree::POLICY_REGION_PREFIX));
    assert!(agents_md(&project).contains("uze agent work name"));
    let view = application.workspace().policy_region(&project).unwrap();
    assert!(view.completion.is_none());
    assert!(view.in_step);
}

#[test]
fn the_declared_completion_behavior_is_what_reaches_the_baseline() {
    let (repository, project) =
        project_with_policy("worktree-completion", "workspace:\n  delivery: merge\n");
    let application = app(repository.root());

    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();
    let written = agents_md(&project);

    assert!(written.contains(CompletionBehavior::Merge.instruction_clause()));
    assert!(!written.contains(CompletionBehavior::Handoff.instruction_clause()));
    assert_eq!(
        application
            .workspace()
            .policy_region(&project)
            .unwrap()
            .completion,
        Some(CompletionBehavior::Merge)
    );
}

/// A project with no `AGENTS.md` has given its agents no instructions yet.
/// The workspace keeps its section of a file the project has and never
/// creates one: a tracked file appearing in the operator's checkout because
/// a screen opened is not the workspace's call.
#[test]
fn the_workspace_never_creates_the_file() {
    let repository = Repository::new("worktree-no-agents-md");
    let project = repository.root().to_path_buf();
    fs::write(project.join("agents.yaml"), POLICY_MANIFEST).unwrap();
    let application = app(repository.root());

    let region = application
        .workspace()
        .sync_policy_region(&project)
        .unwrap()
        .unwrap();
    assert_eq!(region.state, AttachmentState::Missing);
    assert!(!project.join("AGENTS.md").exists());
}

// --- ownership of the region ----------------------------------------------

/// ADR-009's rule applies to this region like any other: an edited region is
/// drift, and drift is refused rather than silently rewritten.
#[test]
fn an_edited_region_is_blocked_not_overwritten() {
    let (repository, project) = project_with_policy("worktree-drift", POLICY_MANIFEST);
    let application = app(repository.root());
    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();

    let file = project.join("AGENTS.md");
    let tampered = agents_md(&project).replace(worktree::WORKTREES_DIRECTORY, "somewhere-else");
    fs::write(&file, &tampered).unwrap();

    let region = application
        .workspace()
        .sync_policy_region(&project)
        .unwrap()
        .unwrap();
    assert_eq!(region.state, AttachmentState::Drifted);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        tampered,
        "drift is reported, never repaired"
    );
    assert!(
        !application
            .workspace()
            .policy_region(&project)
            .unwrap()
            .in_step
    );
}

/// A declaration must stay editable. Before the region identity carried the
/// rendered content's digest, changing the declaration produced a
/// permanently drifted region that reconciliation refused to touch —
/// projected once, never updatable.
#[test]
fn editing_the_declaration_replaces_its_region_rather_than_drifting() {
    let (repository, project) = project_with_policy("worktree-edit", POLICY_MANIFEST);
    let application = app(repository.root());
    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();

    fs::write(
        project.join("agents.yaml"),
        "workspace:\n  delivery: merge\n",
    )
    .unwrap();
    assert!(
        !application
            .workspace()
            .policy_region(&project)
            .unwrap()
            .in_step
    );

    let region = application
        .workspace()
        .sync_policy_region(&project)
        .unwrap()
        .unwrap();
    assert_eq!(region.state, AttachmentState::Matched);
    assert_eq!(region.removed_superseded.len(), 1);
    assert!(region.blocked_superseded.is_empty());

    let written = agents_md(&project);
    assert!(written.contains(CompletionBehavior::Merge.instruction_clause()));
    assert!(
        !written.contains(CompletionBehavior::Handoff.instruction_clause()),
        "the superseded statement must not survive beside the new one"
    );
    assert_eq!(
        written.matches("uze:begin project:worktree-policy").count(),
        1,
        "exactly one policy region at a time"
    );
}

/// Removing a declaration restores the conventional region rather than
/// leaving agents without their naming instruction.
#[test]
fn a_declaration_that_is_gone_restores_the_default_region() {
    let (repository, project) = project_with_policy("worktree-withdrawn", POLICY_MANIFEST);
    let application = app(repository.root());
    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();
    fs::write(project.join("agents.yaml"), "").unwrap();

    let region = application
        .workspace()
        .sync_policy_region(&project)
        .unwrap()
        .unwrap();
    assert_eq!(region.state, AttachmentState::Matched, "{region:?}");
    assert!(agents_md(&project).contains(worktree::POLICY_REGION_PREFIX));
    assert!(agents_md(&project).contains("feat|fix"));
}

// --- the package manager leaves it alone ----------------------------------

#[test]
fn package_reconciliation_neither_writes_nor_removes_the_workspace_region() {
    let (repository, project) = project_with_policy("worktree-pm-leaves-it", POLICY_MANIFEST);
    let application = app(repository.root());

    application.context().reconcile(&project).unwrap();
    let written = agents_md(&project);
    assert!(
        !written.contains(worktree::POLICY_REGION_PREFIX),
        "the package manager does not write it: {written}"
    );
    assert!(
        written.contains(authoring_region::REGION_PREFIX),
        "it writes its own region for a project with an agents.yaml: {written}"
    );

    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();
    fs::write(project.join("agents.yaml"), "").unwrap();
    application.context().reconcile(&project).unwrap();
    assert!(
        agents_md(&project).contains(worktree::POLICY_REGION_PREFIX),
        "nor removes it, even when the policy is gone"
    );
}

#[test]
fn a_stale_workspace_region_leaves_the_package_environment_clear() {
    let (repository, project) = project_with_policy("worktree-status-clear", POLICY_MANIFEST);
    let application = app(repository.root());
    application.context().reconcile(&project).unwrap();
    application
        .workspace()
        .sync_policy_region(&project)
        .unwrap();
    fs::write(
        project.join("agents.yaml"),
        "workspace:\n  delivery: merge\n",
    )
    .unwrap();

    let plan = application.project().plan(&project).unwrap();
    assert!(
        plan.stale_projection.is_none(),
        "the workspace's region is not the package manager's to count: {plan:?}"
    );
}
