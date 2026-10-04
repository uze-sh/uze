//! Acceptance A6/A7: remove lifecycle and drift-blocked destructive
//! removal, driven through the public CLI against fake harnesses.

use uze_testkit::fixtures;
use uze_testkit::temp::TestEnvironment;

use crate::util::{install_fake_harnesses, uze_bin};

/// A6 — remove: install → inspect matched → remove → artifacts gone, Store
/// semantics correct (machine remove never touches the project lock).
#[test]
fn remove_lifecycle_cleans_artifacts_and_keeps_project_lock_untouched() {
    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);
    // Give the project a lock so we can prove machine remove never touches it.
    std::fs::write(
        env.project.join("agents.lock"),
        "version: 1\nmarketplaces: {}\nplugins: {}\n",
    )
    .unwrap();

    let fixture = fixtures::canonical("skill-plugin");
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&env.home, &fixture);
    env.run_ok(
        uze_bin(),
        &market_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut with_json = install_args.clone();
    with_json.push("--format".to_owned());
    with_json.push("json".to_owned());
    env.run_ok(
        uze_bin(),
        &with_json.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let inspect = env.run_ok(
        uze_bin(),
        &["inspect", "uze-agent-skill-conformance", "--format", "json"],
    );
    let report: serde_json::Value = serde_json::from_slice(&inspect.stdout).expect("json report");
    assert_eq!(report["plugin"]["id"], "uze-agent-skill-conformance@test");

    let remove = env.run_ok(
        uze_bin(),
        &[
            "remove",
            "-m",
            "uze-agent-skill-conformance",
            "--format",
            "json",
        ],
    );
    let report: serde_json::Value = serde_json::from_slice(&remove.stdout).expect("json report");
    assert_eq!(report["outcome"], "REMOVED");

    let list = env.run_ok(uze_bin(), &["status", "-m", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&list.stdout).expect("json list");
    let ids: Vec<&str> = json["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|plugin| plugin["id"].as_str())
        .collect();
    assert!(
        !ids.contains(&"uze-agent-skill-conformance"),
        "removed package must be gone from the store, got {ids:?}"
    );
    assert!(
        !env.uze_home
            .join("store/packages/uze-agent-skill-conformance")
            .exists(),
        "store package dir must be removed"
    );
    assert!(
        !env.home.join(".claude/skills").is_dir()
            || std::fs::read_dir(env.home.join(".claude/skills"))
                .unwrap()
                .count()
                == 0,
        "no stale skill projection may survive removal"
    );
    let lock = std::fs::read_to_string(env.project.join("agents.lock")).unwrap();
    assert_eq!(
        lock, "version: 1\nmarketplaces: {}\nplugins: {}\n",
        "machine-scoped remove must never touch the project lock"
    );
}

/// A7 — drift: install, tamper a managed artifact, `doctor` reports drift,
/// and destructive `plugin remove` is blocked with the artifact preserved.
#[test]
fn drift_blocks_destructive_remove_and_preserves_the_artifact() {
    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);

    let fixture = fixtures::canonical("skill-plugin");
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&env.home, &fixture);
    env.run_ok(
        uze_bin(),
        &market_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &install_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    // OpenCode's entry is a directory UZE wrote; editing its SKILL.md by
    // hand is the canonical drifted shape (a foreign file in place of the
    // directory would be a Conflict, not Drift).
    let managed = env
        .home
        .join(".config/opencode/skills")
        .join(uze_core::path::file_name_for(
            "uze-agent-skill-conformance:uze-e2e",
        ));
    assert!(
        managed.is_dir() && !managed.is_symlink(),
        "expected the managed OpenCode skill directory, got a different projection"
    );
    let skill = managed.join("SKILL.md");
    std::fs::write(&skill, "edited by hand\n").unwrap();

    // Drift is a problem, so doctor fails the command as well as saying so.
    let doctor = env.run(uze_bin(), &["doctor"]);
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(!doctor.status.success(), "{stdout}");
    assert!(
        stdout.contains("1 file drifted"),
        "doctor must report the drifted attachment, got: {stdout}"
    );

    let remove = env.run(uze_bin(), &["remove", "-m", "uze-agent-skill-conformance"]);
    let remove_out = String::from_utf8_lossy(&remove.stdout);
    assert!(
        remove_out.contains("Removal blocked") && remove_out.contains("Drift"),
        "a drifted attachment must block destructive removal, got: {remove_out}"
    );
    assert_eq!(
        std::fs::read_to_string(&skill).unwrap(),
        "edited by hand\n",
        "a blocked removal must leave the drifted artifact untouched"
    );
}
