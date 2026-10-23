//! A project's plain `setup` and `gate` lines, as `uze status` reports
//! them on the platform the suite runs on: a line the portable shell runs
//! runs everywhere, and one it cannot run is named with the reason
//! everywhere: as a command that does not run where no `sh` runs it
//! instead, and as one that runs only in `sh` where one does.

use uze_testkit::temp::TestEnvironment;

use crate::util::uze_bin;

/// What `uze status` says about a project's commands, for a project whose
/// `agents.yaml` declares `workspace`: those that do not run here, and
/// those that run here only in `sh`.
struct Reported {
    not_running: Vec<serde_json::Value>,
    only_in_sh: Vec<serde_json::Value>,
}

fn reported(workspace: &str) -> Reported {
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
    let list = |key: &str| report[key].as_array().cloned().unwrap_or_default();
    Reported {
        not_running: list("steps_not_spelled_here"),
        only_in_sh: list("steps_only_in_sh"),
    }
}

/// The one step reported as not portable, and why, whichever list this
/// platform puts it in.
fn not_portable(reported: &Reported) -> (&serde_json::Value, &str) {
    let (step, other) = if has_sh() {
        (&reported.only_in_sh, &reported.not_running)
    } else {
        (&reported.not_running, &reported.only_in_sh)
    };
    assert!(other.is_empty(), "{other:?}");
    assert_eq!(step.len(), 1, "{step:?}");
    (&step[0]["step"], step[0]["not_portable"].as_str().unwrap())
}

fn has_sh() -> bool {
    uze_platform::shell::FAMILY == uze_platform::shell::Family::Posix
}

#[test]
fn a_portable_setup_and_gate_run_on_every_platform() {
    let reported =
        reported("  setup: test -f .env || cp .env.example .env\n  gate: cargo test --workspace\n");
    assert!(
        reported.not_running.is_empty(),
        "{:?}",
        reported.not_running
    );
    assert!(reported.only_in_sh.is_empty(), "{:?}", reported.only_in_sh);
}

#[test]
fn a_gate_the_portable_shell_cannot_read_runs_only_where_sh_does_and_says_why() {
    let reported = reported("  gate: for f in *; do make; done\n");
    let (step, why) = not_portable(&reported);
    assert_eq!(step, "gate");
    assert!(why.starts_with("`for` at column 1"), "{why}");
}

#[test]
fn a_brace_parameter_keeps_its_posix_meaning_and_says_why() {
    let reported = reported("  setup: cp .env.example ${ENV_FILE}\n");
    let (step, why) = not_portable(&reported);
    assert_eq!(step, "setup");
    assert!(why.contains("`${`") && why.contains("$NAME"), "{why}");
}

#[test]
fn an_unsupported_flag_is_named_on_every_platform() {
    let reported = reported("  setup: head -1 VERSION > .version\n");
    let (_, why) = not_portable(&reported);
    assert!(why.starts_with("`-1`"), "{why}");
}

#[test]
fn a_posix_only_spelling_is_missing_only_where_there_is_no_sh() {
    let reported = reported("  setup:\n    posix: head -1 VERSION > .version\n");
    assert!(
        reported.only_in_sh.is_empty(),
        "a `posix` spelling says where it runs: {:?}",
        reported.only_in_sh
    );
    if has_sh() {
        assert!(
            reported.not_running.is_empty(),
            "{:?}",
            reported.not_running
        );
    } else {
        assert_eq!(reported.not_running.len(), 1, "{:?}", reported.not_running);
        assert!(
            reported.not_running[0]["not_portable"].is_null(),
            "it lacks a spelling, nothing more"
        );
    }
}
