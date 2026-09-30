//! Acceptance A9/A10: nested cwd resolution and workspace health —
//! running UZE from `project/subdir/deeper` must resolve the workspace
//! root and report a healthy locked environment.

use uze_testkit::fixtures;
use uze_testkit::scenario::Scenario;
use uze_testkit::temp::TestEnvironment;

use crate::util::{install_fake_harnesses, marketplace_json, uze_bin};

/// A9 — run from `project/subdir/deeper`: the nearest workspace root
/// (`AGENTS.md` + lock) is resolved, never the subdirectory.
#[test]
fn nested_cwd_resolves_workspace_root_and_installs() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let scenario = Scenario::new()
        .marketplace("ai", &marketplace_json("ai", "flow"))
        .marketplace_plugin("flow", fixtures::canonical("flow"))
        .lock_plugin_from_market("ai", "flow")
        .project_file("AGENTS.md", "# Workspace\n")
        .materialize(&env);
    env.run_ok(
        uze_bin(),
        &[
            "market",
            "add",
            scenario.marketplace.as_ref().unwrap().to_str().unwrap(),
        ],
    );

    let deep = env.nested_project("apps/web/src");
    let status = env
        .command(uze_bin())
        .current_dir(&deep)
        .args(["status"])
        .output()
        .expect("status must run from a nested cwd");
    assert!(status.status.success());
    let before = String::from_utf8_lossy(&status.stdout);
    assert!(
        before.contains("flow") && before.contains("not installed"),
        "nested cwd must still see the workspace lock, got: {before}"
    );

    let install = env
        .command(uze_bin())
        .current_dir(&deep)
        .args(["install"])
        .output()
        .expect("install must run from a nested cwd");
    assert!(
        install.status.success(),
        "install from nested cwd failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let after = env
        .command(uze_bin())
        .current_dir(&deep)
        .args(["status"])
        .output()
        .expect("status must run after install");
    let after = String::from_utf8_lossy(&after.stdout);
    assert!(
        after.contains("flow") && after.contains("installed") && after.contains("✓ ready"),
        "nested cwd must report the environment ready, got: {after}"
    );

    // The lock lives at the workspace root, never inside the subdir.
    assert!(env.project.join("agents.lock").is_file());
    assert!(!deep.join("agents.lock").exists());
}

/// A10 — workspace overview: before install the environment is not ready;
/// after `uze install` it is.
#[test]
fn workspace_overview_tracks_environment_readiness() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let scenario = Scenario::new()
        .marketplace("ai", &marketplace_json("ai", "flow"))
        .marketplace_plugin("flow", fixtures::canonical("flow"))
        .lock_plugin_from_market("ai", "flow")
        .materialize(&env);
    env.run_ok(
        uze_bin(),
        &[
            "market",
            "add",
            scenario.marketplace.as_ref().unwrap().to_str().unwrap(),
        ],
    );

    let before = env.run(uze_bin(), &["doctor", "--format", "json"]);
    let before: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    assert!(
        before["attachments"].is_array(),
        "doctor answers before anything is installed: {before}"
    );

    env.run_ok(uze_bin(), &["install"]);

    let after = env.run(uze_bin(), &["doctor", "--format", "json"]);
    let after: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    for attachment in after["attachments"].as_array().unwrap() {
        assert_eq!(attachment["state"]["missing"], 0, "{attachment}");
        assert_eq!(attachment["state"]["drifted"], 0, "{attachment}");
    }
}
