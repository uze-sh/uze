//! Grammar/precedence tests for ADR-019 (`docs/adr/019-explicit-project-
//! machine-boundary-in-cli-command-grammar.md`): built-ins must always take
//! precedence over `<plugin>@<market>` shorthand, the shorthand must
//! require `@`, and an unrecognized flag after it must never be silently
//! ignored. Each test below corresponds to one line of the ambiguity list
//! the change was reviewed against.

use std::{path::PathBuf, process::Command};
use uze_testkit::process::IsolatedHome;

fn temporary_home(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

fn uze(home: &PathBuf) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_uze"));
    command
        .env("UZE_HOME", home)
        .isolated_home(home)
        .env("PATH", uze_testkit::process::system_path())
        // Isolates project-root resolution from this repo's own real
        // `agents.lock` — see `root_remove_no_longer_falls_back_to_global_removal`
        // in tests/cli.rs for why this matters.
        .current_dir(home);
    command
}

fn package_fixture() -> PathBuf {
    uze_testkit::fixtures::canonical("skill-plugin")
}

/// `uze flow@ai` — no marketplace `ai` registered, so this must reach the
/// shorthand's own resolution logic (and fail there, on the marketplace
/// lookup) rather than being misparsed as an unrecognized command.
#[test]
fn shorthand_reaches_project_resolution_not_unrecognized_command() {
    let home = temporary_home("shorthand-basic");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["flow@ai"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unrecognized subcommand"),
        "flow@ai must not be treated as an unrecognized command, got: {stderr}"
    );
    assert!(
        stderr.contains("marketplace") && stderr.contains("ai"),
        "expected a marketplace-not-found error, got: {stderr}"
    );
    assert!(
        !PathBuf::from("agents.lock").exists() || !home.join("agents.lock").is_file(),
        "a failed shorthand must never leave a lock behind"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// `uze remove flow` — the built-in `Remove` variant, never reinterpreted:
/// it has no `@`, so it was never shorthand-eligible in the first place,
/// but this proves it actually reaches `Command::Remove`'s own project-
/// scoped logic (a `NoProjectEnvironment`-shaped failure here, not a
/// generic "unrecognized subcommand").
#[test]
fn remove_is_the_builtin_not_shorthand() {
    let home = temporary_home("remove-builtin");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["remove", "flow"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unrecognized subcommand"),
        "remove must dispatch as a built-in, got: {stderr}"
    );
    assert!(
        stderr.contains("no project environment found"),
        "expected the project-scoped remove error, got: {stderr}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// `uze market add <path>` — machine-level, must never touch a project's
/// `agents.lock` even though one is present in the working directory.
#[test]
fn market_add_never_touches_the_project_lock() {
    let home = temporary_home("market-add");
    std::fs::create_dir_all(&home).unwrap();
    // A local marketplace root: needs its own `marketplace.json`.
    let market_root = home.join("market");
    std::fs::create_dir_all(&market_root).unwrap();
    std::fs::write(
        market_root.join("marketplace.json"),
        r#"{"name": "ai", "plugins": []}"#,
    )
    .unwrap();

    let output = uze(&home)
        .args(["market", "add", market_root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "market add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !home.join("agents.lock").is_file(),
        "`market add` must never create agents.lock"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// `uze install <path>` — a direct source without a marketplace is
/// rejected by the product (the marketplace is the provenance contract,
/// ADR-019), and the marketplace flow never touches the project lock.
#[test]
fn plugin_install_requires_a_marketplace_and_never_touches_the_project_lock() {
    let home = temporary_home("plugin-install-path");
    std::fs::create_dir_all(&home).unwrap();
    let rejected = uze(&home)
        .args(["install", package_fixture().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !rejected.status.success(),
        "a direct-path install must be rejected"
    );
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        stderr.contains("marketplace"),
        "the rejection must point at the marketplace contract: {stderr}"
    );
    assert!(
        !home.join("agents.lock").is_file(),
        "`install` must never create agents.lock"
    );

    // The marketplace flow is the supported path.
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&home, &package_fixture());
    let market_add = uze(&home).args(&market_args).output().unwrap();
    assert!(
        market_add.status.success(),
        "market add failed: {}",
        String::from_utf8_lossy(&market_add.stderr)
    );
    let output = uze(&home).args(&install_args).output().unwrap();
    assert!(
        output.status.success(),
        "marketplace install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !home.join("agents.lock").is_file(),
        "`install -m` must never create agents.lock"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// `uze doctor` — a built-in with no `@`, unambiguous by construction.
#[test]
fn doctor_is_the_builtin() {
    let home = temporary_home("doctor-builtin");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["doctor"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("home"), "{stdout}");
    let _ = std::fs::remove_dir_all(home);
}

/// `uze status` — a built-in with no `@`, unambiguous by construction.
#[test]
fn status_is_the_builtin() {
    let home = temporary_home("status-builtin");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["status"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A temp home is no project, so `status` answers the machine read
    // model — the absence of a project is an answer, not a fault.
    assert!(stdout.contains("no project here"), "{stdout}");
    let _ = std::fs::remove_dir_all(home);
}

/// `uze unknown` — no `@`, matches no built-in: a real unrecognized
/// command, not silent shorthand. Must fail with a `clap`-shaped error and
/// a hint pointing at the `@market` form.
#[test]
fn bare_unknown_name_is_an_unrecognized_command_with_a_hint() {
    let home = temporary_home("unknown-bare");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["unknown"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown command `unknown`"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("unknown@<market>"),
        "expected a hint toward the shorthand form, got: {stderr}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// A near miss of a command is a typo, and is answered with the command
/// rather than with a plugin nobody meant.
#[test]
fn a_typo_of_a_command_is_answered_with_the_command() {
    let home = temporary_home("unknown-typo");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["instal"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("uze install"), "got: {stderr}");
    assert!(!stderr.contains("@<market>"), "got: {stderr}");
    let _ = std::fs::remove_dir_all(home);
}

/// `uze flow@ai --unknown` — an unrecognized flag after the shorthand must
/// be rejected by `clap`, never silently ignored (the exact bug the
/// pre-`clap` `argv[1].contains('@')` parser had).
#[test]
fn shorthand_rejects_an_unknown_flag_instead_of_ignoring_it() {
    let home = temporary_home("shorthand-unknown-flag");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["flow@ai", "--unknown"]).output().unwrap();
    assert!(
        !output.status.success(),
        "an unrecognized flag must fail the command, not be silently dropped"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--unknown") || stderr.to_lowercase().contains("unexpected argument"),
        "expected clap's own unrecognized-argument error, got: {stderr}"
    );
    assert!(
        !home.join("agents.lock").is_file(),
        "a rejected flag must never let the shorthand proceed and write a lock"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// `uze doctor@foo` — `doctor` is a built-in name, but the *token* is
/// `"doctor@foo"`, which no built-in name equals exactly. Since no
/// built-in name can ever contain `@` (ADR-019's soundness argument), this
/// must be classified as shorthand: plugin `doctor` from marketplace
/// `foo` — not the `doctor` diagnostics command.
#[test]
fn builtin_name_followed_by_at_market_is_still_shorthand() {
    let home = temporary_home("doctor-at-foo");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["doctor@foo"]).output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Proof it did NOT run diagnostics: `doctor`'s own success output
    // (its "store" row) never appears, and the failure is a marketplace lookup,
    // not a coincidentally-similar diagnostics report.
    assert!(
        !stdout.contains("store"),
        "doctor@foo must not run the doctor command, got stdout: {stdout}"
    );
    assert!(
        !stderr.contains("unrecognized subcommand"),
        "doctor@foo must be classified as shorthand, got: {stderr}"
    );
    assert!(
        stderr.contains("marketplace") && stderr.contains("foo"),
        "expected a marketplace-not-found error for `foo`, got: {stderr}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// `uze --help` lists every command flat, grouped by the part of uze it
/// belongs to, with no scope heading: a command reports its own scope
/// (ADR-054), so a heading such as "Project:" would claim one it may not
/// have. It ends by saying where the documentation is, for a person and
/// for an agent.
#[test]
fn help_lists_commands_flat_and_ends_at_the_documentation() {
    let home = temporary_home("help-flat");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("Project:"), "a scope heading is back");
    assert!(!stdout.contains("Machine:"), "a scope heading is back");
    assert!(
        stdout.contains("plugin@market"),
        "missing the install example"
    );
    for command in ["install", "market", "workspace", "setup"] {
        assert!(
            stdout
                .lines()
                .any(|line| line.trim_start().starts_with(command)),
            "`{command}` is not listed"
        );
    }
    let last = stdout.lines().rev().find(|line| !line.trim().is_empty());
    assert!(
        last.is_some_and(|line| line.contains("https://uze.sh/llms.txt")),
        "the help does not end at the documentation for agents"
    );
    assert!(stdout.contains("https://uze.sh/docs"));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn setup_consolidates_harness_operations_without_a_harness_namespace() {
    let home = temporary_home("setup-surface");
    std::fs::create_dir_all(&home).unwrap();

    let help = uze(&home).args(["setup", "help"]).output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout);
    for usage in ["<agent>...", "setup list", "setup inspect"] {
        assert!(help.contains(usage), "setup help missing `{usage}`: {help}");
    }

    let root_help = uze(&home).args(["--help"]).output().unwrap();
    assert!(root_help.status.success());
    assert!(
        !String::from_utf8_lossy(&root_help.stdout)
            .lines()
            .any(|line| line.trim_start().starts_with("harness ")),
        "the redundant namespace must not be part of the command surface"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// `uze market --help` must list only `market`'s own verbs — namespace
/// help stays self-contained, per the same requirement's second scenario.
#[test]
fn market_help_is_self_contained() {
    let home = temporary_home("market-help");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["market", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for verb in ["add", "remove", "list", "inspect"] {
        assert!(stdout.contains(verb), "market --help missing `{verb}`");
    }
    for unrelated in ["harness", "doctor"] {
        assert!(
            !stdout.contains(unrelated),
            "market --help unexpectedly mentions unrelated `{unrelated}`: {stdout}"
        );
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn every_public_help_route_uses_the_uze_renderer_and_dash_help_is_rejected() {
    let home = temporary_home("unified-help");
    std::fs::create_dir_all(&home).unwrap();

    for argument in ["help", "--help", "-h"] {
        let output = uze(&home).args([argument]).output().unwrap();
        assert!(output.status.success(), "root `{argument}` must succeed");
        assert!(String::from_utf8_lossy(&output.stdout).contains("uze"));
    }
    for (command, title) in [
        ("market", "uze market"),
        ("config", "uze config"),
        ("setup", "uze setup"),
    ] {
        let output = uze(&home).args([command, "--help"]).output().unwrap();
        assert!(output.status.success(), "{command} help must succeed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(title), "expected custom title in: {stdout}");
        assert!(
            !stdout.contains("Usage:"),
            "the generated Clap help must not leak through: {stdout}"
        );
    }

    let invalid = uze(&home).args(["-help"]).output().unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("`-help` is not supported"));
    let _ = std::fs::remove_dir_all(home);
}

/// Every built-in command/subcommand name, at every nesting level, is free
/// of `@` — the machine-checkable half of ADR-019's soundness argument
/// (`specs/cli-command-grammar/spec.md`'s "No built-in command can ever
/// collide with a shorthand token" requirement).
#[test]
fn no_builtin_command_name_contains_at() {
    let home = temporary_home("help-listing");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).args(["--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Skip the lines that legitimately document/demonstrate the
        // shorthand form itself (the install example and the Examples section).
        if line.contains("<plugin>@<market>")
            || line.contains("plugin@market")
            || line.contains("flow@ai")
            || line.trim_start().starts_with("uze ")
        {
            continue;
        }
        assert!(
            !line.contains('@'),
            "a help line unexpectedly contains '@' outside the shorthand form: {line}"
        );
    }
    let _ = std::fs::remove_dir_all(home);
}

/// `uze --help | head -3` — the shell idiom every report invites. Rust's
/// runtime ignores `SIGPIPE`, so the first `println!` after the reader
/// leaves used to panic and exit 101; the process must instead end at the
/// signal, quietly. The reader here closes before the child writes at all,
/// which is the same condition without the race a real `head` introduces.
#[test]
fn a_reader_that_leaves_ends_the_command_quietly() {
    use std::process::Stdio;

    let home = temporary_home("closed-pipe");
    std::fs::create_dir_all(&home).unwrap();
    for arguments in [vec!["--help"], vec!["doctor"]] {
        let mut child = uze(&home)
            .args(&arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let output = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_ne!(
            output.status.code(),
            Some(101),
            "`uze {}` panicked on a closed pipe: {stderr}",
            arguments.join(" ")
        );
        assert!(
            !stderr.contains("panicked"),
            "`uze {}` reported a panic: {stderr}",
            arguments.join(" ")
        );
    }
    let _ = std::fs::remove_dir_all(home);
}

/// An argument that is not UTF-8 is clap's to refuse — `std::env::args()`
/// used to panic on one before clap ever saw the command line.
#[test]
fn a_non_utf8_argument_is_refused_rather_than_panicked_on() {
    let home = temporary_home("non-utf8-argument");
    std::fs::create_dir_all(&home).unwrap();
    let output = uze(&home).arg(not_unicode()).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert_ne!(output.status.code(), Some(101), "panicked: {stderr}");
    assert!(!stderr.contains("panicked"), "panicked: {stderr}");
    let _ = std::fs::remove_dir_all(home);
}

/// `uze market add help` asked to add a marketplace named `help`. It used
/// to print the `market` help page and exit 0, which a script reads as the
/// marketplace having been added.
#[test]
fn a_trailing_help_positional_is_a_value_not_a_help_request() {
    let home = temporary_home("positional-help");
    std::fs::create_dir_all(&home).unwrap();

    for arguments in [
        vec!["market", "add", "help"],
        vec!["remove", "help"],
        vec!["plugin", "inspect", "help"],
    ] {
        let output = uze(&home).args(&arguments).output().unwrap();
        let rendered = arguments.join(" ");
        assert!(
            !output.status.success(),
            "`uze {rendered}` did nothing and reported success"
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("uze market <command>"),
            "`uze {rendered}` printed a help page instead of acting"
        );
    }

    // The verb itself still reaches the renderer, at every depth it stands at.
    for arguments in [vec!["help"], vec!["help", "market"], vec!["market", "help"]] {
        let output = uze(&home).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "`uze {}` must still print help",
            arguments.join(" ")
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("uze"));
    }
    let _ = std::fs::remove_dir_all(home);
}

/// Registers the staged test marketplace from `home` and returns the
/// package spec it offers.
fn staged_market(home: &PathBuf) -> String {
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(home, &package_fixture());
    let market_add = uze(home).args(&market_args).output().unwrap();
    assert!(
        market_add.status.success(),
        "market add failed: {}",
        String::from_utf8_lossy(&market_add.stderr)
    );
    install_args.last().unwrap().clone()
}

/// Outside a project a package install is the machine half alone, and
/// both spellings of it — `install` and the shorthand — say so, in the
/// scope line they end with rather than a title claiming a project.
#[test]
fn a_package_install_outside_a_project_reports_the_machine_scope() {
    let home = temporary_home("install-scope-no-project");
    std::fs::create_dir_all(&home).unwrap();
    let spec = staged_market(&home);

    for arguments in [
        vec!["install", spec.as_str()],
        vec![spec.as_str()],
        vec!["install", "-m", spec.as_str()],
    ] {
        let output = uze(&home).args(&arguments).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let rendered = arguments.join(" ");
        assert!(
            output.status.success(),
            "`uze {rendered}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("on this machine only"),
            "`uze {rendered}` must end with the scope it touched: {stdout}"
        );
        assert!(
            !stdout.contains("Added to this project") && !stdout.contains("this project"),
            "`uze {rendered}` declared nothing and must not claim a project: {stdout}"
        );
    }
    assert!(!home.join("agents.yaml").exists());
    let _ = std::fs::remove_dir_all(home);
}

/// `-m` promises no project file is touched; converging a project is
/// nothing but touching them, so `install -m` with no package is refused.
#[test]
fn install_m_without_a_package_is_refused_and_touches_no_project_file() {
    let home = temporary_home("install-m-no-package");
    std::fs::create_dir_all(home.join(".git")).unwrap();

    let output = uze(&home).args(["install", "-m"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "`install -m` converged: {stderr}");
    assert!(
        stderr.contains("-m") && stderr.contains("<name>@<marketplace>"),
        "the refusal must say `-m` needs a package: {stderr}"
    );
    for file in ["agents.yaml", "agents.lock", "AGENTS.md"] {
        assert!(!home.join(file).exists(), "`install -m` wrote {file}");
    }
    let _ = std::fs::remove_dir_all(home);
}

/// `uze install <path>` converged the project at `path` before the package
/// form shared the positional, and still does when the path is a
/// project's root.
#[test]
fn install_with_a_project_directory_converges_that_project() {
    let home = temporary_home("install-project-path");
    let project = home.join("project");
    std::fs::create_dir_all(project.join(".git")).unwrap();

    for (directory, argument) in [(&home, "project"), (&project, ".")] {
        let output = uze(directory).args(["install", argument]).output().unwrap();
        assert!(
            output.status.success(),
            "`uze install {argument}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            project.join("agents.yaml").is_file(),
            "`uze install {argument}` did not converge the project"
        );
        std::fs::remove_file(project.join("agents.yaml")).unwrap();
    }
    let _ = std::fs::remove_dir_all(home);
}

/// A removed spelling is answered with its replacement whatever followed
/// it — not clap's bare error, and not the shorthand's "did you mean".
#[test]
fn a_removed_spelling_names_its_replacement_on_any_argv_it_prefixes() {
    let home = temporary_home("removed-spellings");
    std::fs::create_dir_all(&home).unwrap();

    for (arguments, replacement) in [
        (vec!["agent", "task"], "uze agent work"),
        (vec!["agent", "task", "name", "feat/x"], "uze agent work"),
        (vec!["theme"], "uze config theme"),
        (vec!["theme", "list"], "uze config theme"),
        (vec!["plugin", "install", "x@y"], "root verbs"),
    ] {
        let output = uze(&home).args(&arguments).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            stderr.contains(replacement) && !stderr.contains("did you mean"),
            "`uze {}` must name `{replacement}`, got: {stderr}",
            arguments.join(" ")
        );
    }
    let _ = std::fs::remove_dir_all(home);
}

/// `status -m` inside a project states the machine because it was asked
/// to, not because there is no project — and its help says what it shows.
#[test]
fn status_m_inside_a_project_does_not_claim_there_is_none() {
    let home = temporary_home("status-m-in-project");
    std::fs::create_dir_all(home.join(".git")).unwrap();

    let output = uze(&home).args(["status", "-m"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("uze@uze-official") && !stdout.contains("no project here"),
        "got: {stdout}"
    );

    let help = uze(&home).args(["status", "--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.contains("Show this project's environment") && !help.contains("Remove from"),
        "got: {help}"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_marketplace_removed_whole_says_its_registry_entry_went() {
    let home = temporary_home("market-removal-entry");
    std::fs::create_dir_all(&home).unwrap();
    let spec = staged_market(&home);
    let market = spec.rsplit_once('@').unwrap().1;

    let output = uze(&home)
        .args(["market", "remove", market])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("1 marketplace removed"), "got: {stdout}");
    let _ = std::fs::remove_dir_all(home);
}

/// `config.toml` is the operator's text: a word this build does not know
/// reads as silent, is reported by the verb that reads it, and is never
/// written over.
#[test]
fn an_unknown_notification_choice_is_reported_and_left_alone() {
    let home = temporary_home("notification-unknown");
    std::fs::create_dir_all(&home).unwrap();
    let written = "[notifications]\nagent_finished = \"loudly\"\n";
    std::fs::write(home.join("config.toml"), written).unwrap();

    let output = uze(&home)
        .args(["config", "notification"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stdout.contains("silent"), "got: {stdout}");
    assert!(
        stderr.contains("`loudly`"),
        "the word went unreported: {stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("config.toml")).unwrap(),
        written
    );
    let _ = std::fs::remove_dir_all(home);
}

/// The CLI flips the same switch the management screen does: written to
/// `[extensions]` in `config.toml`, read back by the listing, and an id
/// UZE does not carry refused rather than recorded.
#[test]
fn an_extension_is_switched_from_the_cli() {
    let home = temporary_home("extension-switch");
    std::fs::create_dir_all(&home).unwrap();

    let output = uze(&home)
        .args(["config", "extension", "architect", "off"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = std::fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("[extensions]"), "got: {written}");
    assert!(written.contains("architect = false"), "got: {written}");

    let output = uze(&home)
        .args(["config", "extension", "--format", "json"])
        .output()
        .unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let enabled = |id: &str| {
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == id)
            .map(|entry| entry["enabled"].as_bool().unwrap())
    };
    assert_eq!(enabled("architect"), Some(false));
    assert_eq!(enabled("code"), Some(true));

    let output = uze(&home)
        .args(["config", "extension", "nope", "off"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "an unknown id is refused");
    assert_eq!(
        std::fs::read_to_string(home.join("config.toml")).unwrap(),
        written,
        "and nothing is written for it"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// A marketplace whose manifest names it outside the name rule is refused
/// before anything is recorded, and told the name it meant.
#[test]
fn market_add_refuses_a_name_outside_the_rule_and_records_nothing() {
    let home = temporary_home("market-add-name-rule");
    let market_root = home.join("market");
    std::fs::create_dir_all(&market_root).unwrap();
    std::fs::write(
        market_root.join("marketplace.json"),
        r#"{"name": "My_Market", "plugins": []}"#,
    )
    .unwrap();

    let output = uze(&home)
        .args(["market", "add", market_root.to_str().unwrap()])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "a bad name must be refused");
    assert!(stderr.contains("try `my-market`"), "{stderr}");

    let list = uze(&home)
        .args(["market", "list", "--format", "json"])
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&list.stdout);
    assert!(
        !listed.to_lowercase().contains("my_market") && !listed.contains("my-market"),
        "nothing was recorded: {listed}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// A plugin whose own `plugin.json` names it outside the rule is refused
/// at install, before any byte reaches the Store, even when the
/// marketplace entry that points at it is well named.
#[test]
fn a_plugin_named_outside_the_rule_is_refused_at_install() {
    let home = temporary_home("install-name-rule");
    let plugin = home.join("source");
    std::fs::create_dir_all(plugin.join("skills/flow")).unwrap();
    std::fs::write(
        plugin.join("plugin.json"),
        r#"{"name": "Flow", "description": "x"}"#,
    )
    .unwrap();
    std::fs::write(
        plugin.join("skills/flow/SKILL.md"),
        "---\nname: flow\ndescription: x\n---\nbody\n",
    )
    .unwrap();
    uze_testkit::marketplace::stage(
        &home.join("market"),
        r#"{"name": "test", "plugins": [{"name": "flow", "source": "./plugins/flow"}]}"#,
        &[("flow".to_owned(), plugin)],
    );
    let added = uze(&home)
        .args(["market", "add", home.join("market").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let install = uze(&home)
        .args(["install", "-m", "flow@test"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        !install.status.success(),
        "an uppercase name must be refused"
    );
    assert!(stderr.contains("try `flow`"), "{stderr}");
    let stored = home.join("store/plugins/test");
    assert!(
        !stored.exists() || std::fs::read_dir(&stored).unwrap().next().is_none(),
        "nothing reached the Store"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// Two spellings of one name are one package: installing again in
/// another case finds the package already there rather than adding a
/// second Store entry.
#[test]
fn two_spellings_of_one_name_are_one_package() {
    let home = temporary_home("install-two-spellings");
    let spec = staged_market(&home);
    for typed in [spec.clone(), spec.to_uppercase()] {
        let output = uze(&home)
            .args(["install", "-m", typed.as_str()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`install -m {typed}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stored: Vec<String> = std::fs::read_dir(home.join("store/plugins/test"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(stored, vec![spec.split_once('@').unwrap().0.to_owned()]);

    let status = uze(&home)
        .args(["status", "-m", "--format", "json"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    let from_test = report["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| {
            package["id"]
                .as_str()
                .is_some_and(|id| id.to_lowercase().ends_with("@test"))
        })
        .count();
    assert_eq!(from_test, 1, "one package, one registration: {report}");
    let _ = std::fs::remove_dir_all(home);
}

/// A marketplace kept in a directory of a larger repository: adding it
/// names the remote as its identity and says that the local directory, not
/// the remote, is what is read, and how.
#[test]
fn market_add_of_a_subdirectory_says_what_it_reads() {
    let home = temporary_home("market-add-subdirectory");
    let repository = uze_testkit::git::Repository::new("market-add-subdirectory-repo");
    repository.commit_file(
        "aikit/marketplace.json",
        r#"{"name": "aikit", "plugins": []}"#,
    );
    repository.git(&[
        "remote",
        "add",
        "origin",
        "https://gitlab.com/team/monorepo.git",
    ]);
    let marketplace = repository.root().join("aikit");

    let output = uze(&home)
        .args(["market", "add", marketplace.to_str().unwrap()])
        .output()
        .unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(output.status.success(), "{said}");
    assert!(said.contains("gitlab.com/team/monorepo"), "{said}");
    assert!(
        said.contains(
            &uze_platform::path::canonical(&marketplace)
                .unwrap()
                .display()
                .to_string()
        ),
        "{said}"
    );
    assert!(said.contains("mirrored"), "{said}");
    let _ = std::fs::remove_dir_all(home);
}

/// An argument no Unicode string spells: a byte UTF-8 never starts with on
/// Unix, an unpaired surrogate on Windows.
#[cfg(unix)]
fn not_unicode() -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt as _;
    std::ffi::OsString::from_vec(vec![0xff])
}

// A string that is not Unicode is a lone surrogate on Windows, an invalid byte on Unix.
#[cfg(windows)]
fn not_unicode() -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt as _;
    std::ffi::OsString::from_wide(&[0xd800])
}
