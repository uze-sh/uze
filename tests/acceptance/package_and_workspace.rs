//! Acceptance: a project that uses the package manager and the workspace.
//!
//! The package manager converges the project exactly as it does for a
//! person who only uses plugins; the workspace keeps its own section of
//! `AGENTS.md`, which the package manager neither writes, removes nor
//! counts. The workspace's own writes are driven through the application,
//! the layer the workspace client calls, since this suite drives no pane.

use std::path::Path;

use uze_application::UzeApplication;
use uze_core::{UzeHome, authoring::region as authoring_region};
use uze_testkit::fixtures;
use uze_testkit::scenario::{PackageAndWorkspace, PackageOnly};
use uze_testkit::temp::TestEnvironment;
use uze_workspace::worktree::POLICY_REGION_PREFIX;

use crate::util::{install_fake_harnesses, uze_bin};

fn agents_md(project: &Path) -> String {
    std::fs::read_to_string(project.join("AGENTS.md")).unwrap_or_default()
}

fn application(env: &TestEnvironment) -> UzeApplication {
    UzeApplication::new(UzeHome::at(&env.uze_home), Vec::new())
}

#[test]
fn install_leaves_the_workspace_section_to_the_workspace() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let world = PackageAndWorkspace::prepare(&env, "pr");

    env.run_ok(uze_bin(), &["install"]);
    let written = agents_md(&world.project);
    assert!(
        written.contains(authoring_region::REGION_PREFIX),
        "{written}"
    );
    assert!(
        !written.contains(POLICY_REGION_PREFIX),
        "install does not write the workspace's section: {written}"
    );

    let app = application(&env);
    app.workspace()
        .sync_policy_region(&world.project)
        .unwrap()
        .expect("a repository answers");
    let written = agents_md(&world.project);
    let opening = written
        .find("An agent started any other way can ignore it")
        .expect("the section says who it is for");
    let first_command = written
        .find("`uze agent work")
        .expect("the section names the work verbs");
    assert!(opening < first_command, "{written}");

    // The declaration changes and the workspace has not run since: the
    // package manager has nothing to say about it.
    std::fs::write(
        world.project.join("agents.yaml"),
        "workspace:\n  delivery: merge\n",
    )
    .unwrap();
    let status = env.run_ok(uze_bin(), &["status", "--format", "json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        status["drift"]["stale_projection"],
        serde_json::Value::Bool(false),
        "{status}"
    );
    assert!(
        !app.workspace()
            .policy_region(&world.project)
            .unwrap()
            .in_step,
        "the workspace sees its section is behind"
    );
    assert!(world.shell.changed().is_empty());
}

#[test]
fn work_verbs_refuse_an_agent_started_by_hand_and_authoring_answers_it() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let _world = PackageAndWorkspace::prepare(&env, "handoff");

    let refused = env.run(uze_bin(), &["agent", "work", "name", "feat/demo"]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("not an agent UZE launched"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    let plugin = fixtures::canonical("skill-plugin");
    env.run_ok(
        uze_bin(),
        &["agent", "plugin", "check", plugin.to_str().unwrap()],
    );
}

#[test]
fn moving_from_the_package_manager_to_the_workspace_and_back() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let world = PackageOnly::prepare(&env);
    env.run_ok(uze_bin(), &["install"]);
    let lock = std::fs::read(world.project.join("agents.lock")).ok();

    // Somebody starts using the workspace and chooses a policy.
    let app = application(&env);
    app.workspace()
        .set_completion(
            &world.project,
            uze_workspace::worktree::CompletionBehavior::Pr,
        )
        .unwrap();
    app.workspace().sync_policy_region(&world.project).unwrap();
    assert!(agents_md(&world.project).contains(POLICY_REGION_PREFIX));
    assert_eq!(
        std::fs::read(world.project.join("agents.lock")).ok(),
        lock,
        "the package manager's lock is untouched by the workspace"
    );

    // The explicit choice is removed again. The package manager leaves the
    // section alone; the workspace restores its conventional default.
    let manifest = world.project.join("agents.yaml");
    let commented = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("  delivery: pr", "  # delivery: pr");
    std::fs::write(&manifest, commented).unwrap();
    env.run_ok(uze_bin(), &["install"]);
    assert!(
        agents_md(&world.project).contains(POLICY_REGION_PREFIX),
        "install never removes the workspace's section"
    );
    app.workspace().sync_policy_region(&world.project).unwrap();
    assert!(agents_md(&world.project).contains(POLICY_REGION_PREFIX));
    assert!(world.shell.changed().is_empty());
}
