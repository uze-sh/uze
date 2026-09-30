//! Acceptance: a person who only uses the package manager.
//!
//! The whole lifecycle through the real binary, from a machine with a shell
//! of its own and harnesses the person starts by hand. What is proven is
//! what they never meet: no workspace policy, no workspace section in
//! `AGENTS.md`, no checkout, no agent record, no edit to their shell, and
//! a harness command that is still the harness's own binary.

use std::path::Path;

use uze_core::authoring::region as authoring_region;
use uze_testkit::fixtures;
use uze_testkit::scenario::PackageOnly;
use uze_testkit::temp::TestEnvironment;
use uze_workspace::worktree::POLICY_REGION_PREFIX;

use crate::util::{install_fake_harnesses, uze_bin};

/// Registers a marketplace carrying one canonical plugin and returns the
/// plugin's `name@market` spelling.
fn a_marketplace(env: &TestEnvironment) -> String {
    let (market_args, install_args) = uze_testkit::marketplace::marketplace_install_args(
        env.root(),
        &fixtures::canonical("skill-plugin"),
    );
    env.run_ok(
        uze_bin(),
        &market_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    install_args
        .last()
        .expect("the install arguments end with the plugin")
        .clone()
}

fn agents_md(project: &Path) -> String {
    std::fs::read_to_string(project.join("AGENTS.md")).unwrap_or_default()
}

#[test]
fn the_package_manager_alone_never_meets_the_workspace() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let world = PackageOnly::prepare(&env);

    let setup = env.run(uze_bin(), &["setup"]);
    let plugin = a_marketplace(&env);
    env.run_ok(uze_bin(), &["install", &plugin]);
    // Declaring and converging, as a teammate cloning the project would.
    env.run_ok(uze_bin(), &["install"]);

    // The plugin reached the harness natively: its skill is in Claude
    // Code's own discovery, not in anything a launch would add.
    let store_listing = env.run_ok(uze_bin(), &["status", "-m", "--format", "json"]);
    assert!(
        String::from_utf8_lossy(&store_listing.stdout).contains("skill"),
        "the plugin is installed on the machine"
    );

    // No workspace policy was declared on the person's behalf.
    assert_eq!(
        uze_workspace::declaration::declared(&world.project).unwrap(),
        None,
        "the agents.yaml uze created declares no policy"
    );
    let manifest = std::fs::read_to_string(world.project.join("agents.yaml")).unwrap();
    assert!(manifest.contains("# completion: handoff"), "{manifest}");

    // AGENTS.md carries the package manager's regions and nothing of the
    // workspace's.
    let written = agents_md(&world.project);
    assert!(
        written.contains(authoring_region::REGION_PREFIX),
        "{written}"
    );
    assert!(!written.contains(POLICY_REGION_PREFIX), "{written}");

    // No checkout, and no agent the workspace could have recorded.
    assert!(!world.project.join(".worktrees").exists());
    let projects = env.uze_home.join("state").join("projects");
    let recorded_agents = std::fs::read_dir(&projects)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| entry.path().join("agents.json").exists());
    assert!(!recorded_agents, "no agent record exists");

    // Status and update find nothing owed, and removal takes the plugin out.
    let status = env.run_ok(uze_bin(), &["status", "--format", "json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        status["drift"]["stale_projection"],
        serde_json::Value::Bool(false),
        "{status}"
    );
    env.run_ok(uze_bin(), &["update"]);
    let name = plugin.split('@').next().unwrap();
    env.run_ok(uze_bin(), &["remove", name]);

    // The person's shell was never edited, whatever setup did.
    assert!(
        world.shell.changed().is_empty(),
        "shell startup files changed: {:?}; setup said: {}",
        world.shell.changed(),
        String::from_utf8_lossy(&setup.stdout)
    );

    // And outside the workspace a harness command is the harness's own
    // binary: nothing puts uze's shims on this person's PATH.
    let resolved = env
        .command("sh")
        .args(["-c", "command -v claude"])
        .output()
        .unwrap();
    let resolved = String::from_utf8_lossy(&resolved.stdout);
    assert!(
        !resolved.contains(&env.uze_home.join("shims").display().to_string()),
        "claude resolves to {resolved}"
    );
}

#[test]
fn project_resources_a_harness_does_not_read_are_reported_as_inside_the_workspace() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let world = PackageOnly::prepare(&env);
    let skill = world.project.join(".agents/skills/local/SKILL.md");
    std::fs::create_dir_all(skill.parent().unwrap()).unwrap();
    std::fs::write(
        &skill,
        "---\nname: local\ndescription: a project skill.\n---\n\nBody.\n",
    )
    .unwrap();
    env.run(uze_bin(), &["setup"]);

    let status = env.run_ok(uze_bin(), &["status"]);
    let status = String::from_utf8_lossy(&status.stdout);
    assert!(
        status.contains("Inside the workspace") || status.contains("inside the workspace"),
        "a resource only the workspace carries says so: {status}"
    );
}

#[test]
fn a_workspace_section_the_workspace_rejects_fails_no_package_command() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    let world = PackageOnly::prepare(&env);
    // README.md is tracked, so the workspace refuses to link it.
    std::fs::write(
        world.project.join("agents.yaml"),
        "worktrees:\n  link: [README.md]\n",
    )
    .unwrap();

    env.run_ok(uze_bin(), &["status"]);
    env.run_ok(uze_bin(), &["install"]);
    assert!(
        uze_workspace::declaration::declared(&world.project).is_err(),
        "the workspace still reports it"
    );
}

#[test]
fn a_project_that_is_not_a_git_repository_reads_a_workspace_section_without_failing() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    std::fs::write(
        env.project.join("agents.yaml"),
        "worktrees:\n  link: [.env]\n",
    )
    .unwrap();

    env.run_ok(uze_bin(), &["status"]);
}
