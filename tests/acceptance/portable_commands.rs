//! A project's plain `setup` and `gate` lines, as `uze status` reports
//! them on the platform the suite runs on: a line the portable shell runs
//! runs everywhere, and one it cannot run is named, with the reason, only
//! where no `sh` runs it instead.

use uze_testkit::temp::TestEnvironment;

use crate::util::uze_bin;

/// The commands `uze status` says do not run here, for a project whose
/// `agents.yaml` declares `workspace`.
fn not_running_here(workspace: &str) -> Vec<serde_json::Value> {
    let env = TestEnvironment::isolated();
    std::fs::write(env.project.join("AGENTS.md"), "# Workspace\n").unwrap();
    std::fs::write(
        env.project.join("agents.yaml"),
        format!("workspace:\n{workspace}"),
    )
    .unwrap();
    let init = env
        .command("git")
        .args(["init", "--quiet"])
        .output()
        .unwrap();
    assert!(init.status.success());
    let report = env.run_ok(uze_bin(), &["status", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    report["steps_not_spelled_here"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn has_sh() -> bool {
    uze_platform::shell::FAMILY == uze_platform::shell::Family::Posix
}

#[test]
fn a_portable_setup_and_gate_run_on_every_platform() {
    let steps = not_running_here(
        "  setup: test -f .env || cp .env.example .env\n  gate: cargo test --workspace\n",
    );
    assert!(steps.is_empty(), "{steps:?}");
}

#[test]
fn a_gate_the_portable_shell_cannot_read_runs_only_where_sh_does() {
    let steps = not_running_here("  gate: for f in *; do make; done\n");
    if has_sh() {
        assert!(steps.is_empty(), "sh runs it here: {steps:?}");
    } else {
        assert_eq!(steps.len(), 1, "{steps:?}");
        assert_eq!(steps[0]["step"], "gate");
        let why = steps[0]["not_portable"].as_str().unwrap();
        assert!(why.starts_with("`for` at column 1"), "{why}");
    }
}

#[test]
fn a_brace_parameter_keeps_its_posix_meaning_and_says_why_elsewhere() {
    let steps = not_running_here("  setup: cp .env.example ${ENV_FILE}\n");
    if has_sh() {
        assert!(steps.is_empty(), "sh runs it here: {steps:?}");
    } else {
        assert_eq!(steps[0]["step"], "setup");
        let why = steps[0]["not_portable"].as_str().unwrap();
        assert!(why.contains("`${`") && why.contains("$NAME"), "{why}");
    }
}

#[test]
fn a_posix_only_spelling_never_runs_in_the_portable_shell() {
    let steps = not_running_here("  setup:\n    posix: head -1 VERSION > .version\n");
    if has_sh() {
        assert!(steps.is_empty(), "{steps:?}");
    } else {
        assert_eq!(steps.len(), 1, "{steps:?}");
        assert!(
            steps[0]["not_portable"].is_null(),
            "it lacks a spelling, nothing more"
        );
    }
}
