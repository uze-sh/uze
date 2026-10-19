//! What UZE leaves in a project's `AGENTS.md`: nothing of its own.
//!
//! Earlier releases projected the workspace's policy and the plugin
//! authoring verbs into the file. The workspace's instructions now reach an
//! agent through its launch context and the verbs through the `uze:author`
//! Skill, so reconciling a project takes those regions back out — once, in
//! the primary checkout, and only while they hold what was written.

use std::{fs, path::Path, path::PathBuf};

use uze_application::UzeApplication;
use uze_core::{UzeHome, digest, retired_region, text_region};
use uze_integrations::registry::IntegrationRegistry;
use uze_testkit::git::Repository;

fn app(root: &Path) -> UzeApplication {
    let home = UzeHome::at(root.join("uze-home"));
    let registry = IntegrationRegistry::isolated(&root.join("harnesses"), &home);
    UzeApplication::new(home, registry.into_parts().0)
}

const OPERATOR_TEXT: &str = "# Project\n\nWhat the operator wrote.\n";

/// A repository whose `AGENTS.md` carries what an earlier release wrote
/// beside the operator's own text.
fn project_with_old_regions(label: &str) -> (Repository, PathBuf, Vec<String>) {
    let repository = Repository::new(label);
    let project = repository.root().to_path_buf();
    let agents_md = project.join("AGENTS.md");
    fs::write(&agents_md, OPERATOR_TEXT).unwrap();
    let identities = [
        (
            retired_region::WORKTREE_POLICY,
            "## Concurrent work isolation\n\n- old text\n",
        ),
        (
            retired_region::PLUGIN_AUTHORING,
            "## Authoring plugins\n\n- old text\n",
        ),
    ]
    .into_iter()
    .map(|(prefix, body)| {
        let identity = format!("{prefix}/{}", digest::short_hex(body.as_bytes()));
        text_region::attach(&agents_md, &identity, body).unwrap();
        identity
    })
    .collect();
    fs::write(project.join("agents.yaml"), "workspace: {}\n").unwrap();
    (repository, project, identities)
}

fn agents_md(project: &Path) -> String {
    fs::read_to_string(project.join("AGENTS.md")).unwrap_or_default()
}

#[test]
fn reconciling_removes_what_an_earlier_release_wrote_and_nothing_else() {
    let (repository, project, identities) = project_with_old_regions("retired-removed");
    let application = app(repository.root());

    let plan = application.context().plan(&project).unwrap();
    assert!(
        plan.has_changes(),
        "the old regions are planned for removal"
    );

    let report = application.context().reconcile(&project).unwrap();
    let regions = report.retired_regions.expect("the old regions were found");
    assert_eq!(regions.removed, identities);
    assert!(regions.kept.is_empty());
    assert_eq!(agents_md(&project), OPERATOR_TEXT, "byte-identical");

    let again = application.context().reconcile(&project).unwrap();
    assert!(
        again.retired_regions.is_none(),
        "a second pass finds nothing"
    );
}

#[test]
fn a_region_edited_by_hand_is_the_operators_now() {
    let (repository, project, identities) = project_with_old_regions("retired-edited");
    let edited = agents_md(&project).replacen("- old text", "- my own rule", 1);
    fs::write(project.join("AGENTS.md"), &edited).unwrap();

    let report = app(repository.root())
        .context()
        .reconcile(&project)
        .unwrap();
    let regions = report.retired_regions.unwrap();
    assert_eq!(regions.kept.len(), 1, "{regions:?}");
    assert_eq!(regions.kept[0].0, identities[0]);
    assert!(agents_md(&project).contains("- my own rule"));
    assert!(!agents_md(&project).contains(retired_region::PLUGIN_AUTHORING));
}

#[test]
fn reconciling_a_project_writes_no_uze_text_into_agents_md() {
    let repository = Repository::new("retired-nothing-written");
    let project = repository.root().to_path_buf();
    fs::write(project.join("AGENTS.md"), OPERATOR_TEXT).unwrap();
    fs::write(
        project.join("agents.yaml"),
        "workspace:\n  delivery: push\n",
    )
    .unwrap();

    app(repository.root())
        .context()
        .reconcile(&project)
        .unwrap();
    assert_eq!(agents_md(&project), OPERATOR_TEXT);
}

#[test]
fn the_workspace_client_only_reads_what_is_left() {
    let (repository, project, identities) = project_with_old_regions("retired-client-reads");
    let application = app(repository.root());
    let before = agents_md(&project);

    let plan = application
        .workspace()
        .retired_regions(&project)
        .expect("the client is told");
    assert_eq!(plan.remove, identities);
    assert_eq!(agents_md(&project), before, "the client writes nothing");
}
