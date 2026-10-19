//! Acceptance: a project that uses the package manager and the workspace.
//!
//! The package manager converges the project exactly as it does for a
//! person who only uses plugins, and neither it nor the workspace writes
//! into `AGENTS.md`. The workspace's own writes are driven through the
//! application, the layer the workspace client calls, since this suite
//! drives no pane.

use std::path::Path;

use uze_application::UzeApplication;
use uze_core::retired_region::WORKTREE_POLICY as POLICY_REGION_PREFIX;
use uze_core::{UzeHome, retired_region::PLUGIN_AUTHORING};
use uze_testkit::fixtures;
use uze_testkit::scenario::{PackageAndWorkspace, PackageOnly};
use uze_testkit::temp::TestEnvironment;

use crate::util::{install_fake_harnesses, uze_bin};

fn agents_md(project: &Path) -> String {
    std::fs::read_to_string(project.join("AGENTS.md")).unwrap_or_default()
}

fn application(env: &TestEnvironment) -> UzeApplication {
    UzeApplication::new(UzeHome::at(&env.uze_home), Vec::new())
}

#[test]
fn install_writes_no_uze_section_into_agents_md() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let world = PackageAndWorkspace::prepare(&env, "push");
    let before = agents_md(&world.project);

    env.run_ok(uze_bin(), &["install"]);
    let written = agents_md(&world.project);
    assert!(!written.contains(PLUGIN_AUTHORING), "{written}");
    assert!(!written.contains(POLICY_REGION_PREFIX), "{written}");
    assert_eq!(written, before, "AGENTS.md is the operator's: {written}");

    let status = env.run_ok(uze_bin(), &["status", "--format", "json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        status["drift"]["stale_projection"],
        serde_json::Value::Bool(false),
        "{status}"
    );
    assert!(world.shell.changed().is_empty());
}

#[test]
fn work_verbs_refuse_an_agent_started_by_hand_and_authoring_answers_it() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let _world = PackageAndWorkspace::prepare(&env, "merge");

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

    // Somebody starts using the workspace and chooses a policy: the choice
    // is the manifest's, and AGENTS.md stays the operator's.
    let before = agents_md(&world.project);
    let app = application(&env);
    app.workspace()
        .set_completion(
            &world.project,
            Some(uze_workspace::worktree::CompletionBehavior::Push),
        )
        .unwrap();
    assert_eq!(agents_md(&world.project), before);
    assert_eq!(
        std::fs::read(world.project.join("agents.lock")).ok(),
        lock,
        "the package manager's lock is untouched by the workspace"
    );

    // The explicit choice is removed again, and installing leaves the file
    // as it was.
    let manifest = world.project.join("agents.yaml");
    let commented = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("  delivery: push", "  # delivery: push");
    std::fs::write(&manifest, commented).unwrap();
    env.run_ok(uze_bin(), &["install"]);
    assert_eq!(agents_md(&world.project), before);
    assert!(world.shell.changed().is_empty());
}
