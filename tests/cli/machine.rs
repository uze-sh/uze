// The cases that drive shebang stand-ins are Unix-only until the stand-ins
// dispatch through `uze-fake-harness` (windows-support task 9.2); what only
// they use is unused elsewhere.
#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::{path::PathBuf, process::Command};
use uze_testkit::fake_harness::{Action, FakeHarness, McpNames};
use uze_testkit::process::IsolatedHome;

fn package_fixture() -> PathBuf {
    uze_testkit::fixtures::canonical("skill-plugin")
}

/// Whether one of `entries` is the fixture's skill as UZE delivers it
/// loose: a real directory whose SKILL.md carries the qualified label.
fn contains_fixture_skill(entries: &[PathBuf]) -> bool {
    entries.iter().any(|entry| {
        entry.is_dir()
            && !entry.is_symlink()
            && std::fs::read_to_string(entry.join("SKILL.md")).is_ok_and(|skill| {
                skill.starts_with("---\nname: uze-agent-skill-conformance:uze-e2e\n")
            })
    })
}

/// `uze install -m` through a staged test marketplace — the product
/// rejects direct-path installs, so the test exercises the real user flow:
/// `market add` first, then `install -m <name>@<market>`.
fn install_via_marketplace_json(
    home: &std::path::Path,
    uze_home: &std::path::Path,
    package: &std::path::Path,
    path: &std::ffi::OsStr,
) -> std::process::Output {
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(home, package);
    let base = || {
        Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", uze_home)
            .isolated_home(home)
            .env("PATH", path)
            .args(&market_args)
            .output()
    };
    let market_add = base().unwrap();
    assert!(
        market_add.status.success(),
        "market add failed: {}",
        String::from_utf8_lossy(&market_add.stderr)
    );
    let mut with_json = install_args.clone();
    with_json.push("--format".to_owned());
    with_json.push("json".to_owned());
    Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", uze_home)
        .isolated_home(home)
        .env("PATH", path)
        .args(&with_json)
        .output()
        .unwrap()
}

fn install_via_marketplace(
    home: &std::path::Path,
    uze_home: &std::path::Path,
    package: &std::path::Path,
    path: &std::ffi::OsStr,
) -> std::process::Output {
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(home, package);
    let base = || {
        Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", uze_home)
            .isolated_home(home)
            .env("PATH", path)
            .args(&market_args)
            .output()
    };
    let market_add = base().unwrap();
    assert!(
        market_add.status.success(),
        "market add failed: {}",
        String::from_utf8_lossy(&market_add.stderr)
    );
    Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", uze_home)
        .isolated_home(home)
        .env("PATH", path)
        .args(&install_args)
        .output()
        .unwrap()
}

/// Copies the MCP fixture package into `dest_dir` with its `mcp.json`
/// placeholder command rewritten to the real, test-build-resolved path of
/// the fixture MCP server binary — see
/// `tests/_fixtures/canonical/mcp-plugin/README.md`.
fn mcp_package_fixture_with_resolved_binary(dest_dir: &std::path::Path) -> PathBuf {
    let source = uze_testkit::fixtures::canonical("mcp-plugin");
    std::fs::create_dir_all(dest_dir).unwrap();
    std::fs::copy(source.join("plugin.json"), dest_dir.join("plugin.json")).unwrap();
    let manifest = std::fs::read_to_string(source.join("mcp.json")).unwrap();
    // Inside a JSON string, so spelled as one: a Windows path's separators
    // are escapes there.
    let binary = serde_json::to_string(env!("CARGO_BIN_EXE_uze-mcp-conformance-fixture")).unwrap();
    let resolved = manifest.replace("__UZE_MCP_FIXTURE_BINARY__", binary.trim_matches('"'));
    std::fs::write(dest_dir.join("mcp.json"), resolved).unwrap();
    dest_dir.to_path_buf()
}

fn temporary_home(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

#[test]
fn no_subcommand_prints_help_even_inside_a_workspace_pane() {
    // Opening the workspace is `uze workspace`; a bare `uze` is help, and
    // `UZE_PANE`, which every process the terminal runtime spawns inherits,
    // no longer turns it into anything else.
    //
    // `HOME`/`UZE_HOME` are scoped for the same reason every other spawn in
    // this suite scopes them: a bare `uze` resolves `UzeHome::from_env()` and
    // reads the theme and keymap under it, and this was the one spawn left
    // reading the developer's real `~/.uze`.
    let home = temporary_home("cli-no-subcommand");
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .isolated_home(&home)
        .env("UZE_HOME", home.join(".uze"))
        .env("UZE_PANE", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Usage"));
    assert!(text.contains("workspace"));
    let _ = std::fs::remove_dir_all(&home);
}

/// Writes a fake `claude`/`codex` executable that understands `--version`
/// and just enough of `mcp get`/`mcp add`/`mcp remove` (tracked via a
/// sibling state directory of touched files, one per registered entry
/// name) to exercise `uze setup`'s detection and the full attachment
/// lifecycle — Skills and MCP alike — deterministically, independent of
/// whether real harness CLIs are installed on the machine running this
/// test. Claude's `mcp add` shape is `--scope user --transport stdio
/// <name> -- <command> [args...]`; Codex's is `<name> -- <command>
/// [args...]` — the script skips known flags and takes the first
/// remaining token as the entry name, working for both shapes.
fn fake_harness_bin_dir(label: &str) -> PathBuf {
    let dir = temporary_home(label);
    std::fs::create_dir_all(&dir).unwrap();
    let mcp_state_dir = dir.join("mcp-state");
    std::fs::create_dir_all(&mcp_state_dir).unwrap();
    for (name, version_line) in [
        ("claude", "9.9.9 (Fake Claude)"),
        ("codex", "codex-cli 9.9.9"),
        ("opencode", "opencode v9.9.9"),
        ("opencode2", "opencode2 v9.9.9"),
        ("agy", "agy 9.9.9"),
    ] {
        FakeHarness::new(&dir, name)
            .shared_log(dir.join("commands.log"))
            .version_line(version_line)
            .on_prefix(["plugin", "list"], Action::stdout(r#"{"imports":[]}"#))
            // Real `agy` stages the copy under the plugin's own declared
            // manifest name, not the source directory's basename (verified
            // against real agy 1.1.22 — see antigravity/plugin.rs's
            // attach_generated_plugin).
            .on_prefix(
                ["plugin", "install"],
                Action::StagePlugin {
                    under_home: PathBuf::from(".gemini/config/plugins"),
                },
            )
            .on_prefix(["plugin"], Action::Exit(0))
            // Claude's own rule: a registry name outside it is refused, and
            // the stand-in refusing it too is what keeps a label the real
            // CLI would reject from passing here.
            .on_prefix(
                ["mcp"],
                Action::McpRegistry {
                    state_dir: mcp_state_dir.clone(),
                    names: if name == "claude" {
                        McpNames::Claude
                    } else {
                        McpNames::Any
                    },
                },
            )
            .on_prefix([""], Action::stdout(version_line))
            .build();
    }
    dir
}

/// A fake legacy V2-only installation. The isolated PATH deliberately has no
/// stable `opencode`: this proves `uze setup opencode` handles `opencode2`
/// without passing it the stable CLI's incompatible `upgrade` subcommand.
// Its stand-in programs are POSIX shell scripts.
#[cfg(unix)]
fn fake_legacy_opencode_bin_dir(label: &str) -> PathBuf {
    let dir = temporary_home(label);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("commands.log");
    FakeHarness::new(&dir, "opencode2")
        .shared_log(&log)
        .version_line("opencode2 v9.9.9")
        .on_prefix([""], Action::stdout("opencode2 v9.9.9"))
        .build();
    // The POSIX installer route runs through `sh`: this one records it and
    // installs nothing.
    FakeHarness::new(&dir, "sh").shared_log(&log).build();
    dir
}

#[test]
fn inspect_reports_an_installed_plugin_without_vendor_writes() {
    let home = temporary_home("cli-inspect");
    let add = install_via_marketplace(
        &home,
        &home,
        &package_fixture(),
        &uze_testkit::process::system_path(),
    );
    assert!(add.status.success());
    let before = std::fs::read(home.join("state/attachments.json")).ok();
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::system_path())
        .args(["inspect", "uze-agent-skill-conformance", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["plugin"]["id"], "uze-agent-skill-conformance@test");
    assert_eq!(report["capabilities"].as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(home.join("state/attachments.json")).ok(),
        before
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn add_and_inspect_use_the_same_injected_uze_home() {
    let home = temporary_home("cli-store");
    let add = install_via_marketplace_json(
        &home,
        &home,
        &package_fixture(),
        &uze_testkit::process::system_path(),
    );
    assert!(add.status.success());
    let installed: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(
        installed["plugin"]["id"],
        "uze-agent-skill-conformance@test"
    );
    assert!(PathBuf::from(installed["plugin"]["store_path"].as_str().unwrap()).starts_with(&home));

    let inspect = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::system_path())
        .args(["inspect", "uze-agent-skill-conformance", "--format", "json"])
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let report: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(report["plugin"]["id"], "uze-agent-skill-conformance@test");
    assert!(
        report["plugin"]["store_path"]
            .as_str()
            .unwrap()
            .contains(&uze_testkit::process::native("store/plugins")),
        "store_path should live under the Store's plugins dir, got {}",
        report["plugin"]["store_path"]
    );
    let _ = std::fs::remove_dir_all(home);
}

/// Deterministic: `PATH` is cleared so no real harness binary or installer
/// can be resolved. Explicit setup records each blocked provisioning
/// attempt, exits non-zero so a scripted caller (an image build) cannot read
/// success over a missing binary, and never creates harness-owned
/// directories under the isolated `HOME`.
///
/// Also the other half of the interactive contract: a bare `uze setup` asks
/// a terminal which harnesses to provision, and takes the whole catalog
/// anywhere the question cannot be answered. Piped output is exactly that
/// case, so this run must reach both harnesses rather than block on a
/// prompt nobody can see.
#[test]
fn setup_reports_absent_harnesses_as_failure_without_writing_state() {
    let home = temporary_home("cli-setup-absent");
    // Through a proxy nobody answers: a route that downloads a harness by an
    // absolute path (OpenCode's on Windows) would otherwise install it.
    let unreachable = "http://127.0.0.1:9";
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", "")
        .env("HTTPS_PROXY", unreachable)
        .env("https_proxy", unreachable)
        .env("ALL_PROXY", unreachable)
        .arg("setup")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.matches("setup failed").count() >= 2, "{stdout}");
    assert!(stdout.contains("0 of "), "{stdout}");
    assert!(
        stderr.contains("setup incomplete: 0 of") && stderr.contains("codex"),
        "{stderr}"
    );
    assert!(!home.join(".claude/skills").exists());
    assert!(!home.join(".agents/skills").exists());
    let _ = std::fs::remove_dir_all(home);
}

/// Deterministic: `uze doctor` is headless and must not print credential
/// material. With the default `uze` seed, a fresh home on a machine where
/// harnesses are detected will already show them as prepared (the default
/// plugin auto-prepares), so this test is deterministic by clearing `PATH` — no
/// harness is detected, therefore both remain "not configured" even after
/// the default plugin's store entry is seeded.
#[test]
fn doctor_reports_package_bytes_no_install_records_and_keeps_them() {
    let home = temporary_home("cli-doctor-unregistered-bytes");
    let stray = home.join("store/plugins/local/stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("plugin.json"), r#"{"name":"stray"}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", "")
        .arg("doctor")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Package bytes no install records"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&stray.file_name().unwrap().to_string_lossy().into_owned()),
        "{stdout}"
    );
    assert!(
        stray.join("plugin.json").is_file(),
        "doctor never deletes bytes"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn doctor_reports_not_configured_before_any_setup() {
    let home = temporary_home("cli-doctor-before-setup");
    // The system's own tools and Git: no harness can be found, and the
    // machine still has what doctor requires of it.
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::system_path())
        .arg("doctor")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Claude Code"));
    assert!(stdout.contains("Codex"));
    assert!(stdout.matches("not found").count() >= 2, "{stdout}");
    // Default `uze` is seeded even when no harness is present.
    assert!(stdout.contains("uze"));
    let _ = std::fs::remove_dir_all(home);
}

/// The report closes on where its findings are explained, as `--help` does.
#[test]
fn doctor_ends_with_the_documentation_url() {
    let home = temporary_home("cli-doctor-docs-url");
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", "")
        .arg("doctor")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.trim_end().lines().last().unwrap_or_default();
    assert!(
        last.contains("docs") && last.contains("https://uze.sh/docs"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// L2 setup conformance: this list is intentionally compared to the product
/// registry below. Registering another harness therefore requires an explicit
/// setup scenario here rather than silently inheriting partial coverage.
const SETUP_CONFORMANCE_HARNESSES: [(&str, &str, &str); 4] = [
    ("claude-code", "claude", "update"),
    ("codex", "codex", "update"),
    ("opencode", "opencode", "upgrade"),
    ("antigravity", "agy", "update"),
];

/// Every registered integration gets a deterministic `uze setup <harness>`
/// command-level contract. The fake executables record their own invocations,
/// proving the update reaches a real vendor binary on PATH rather than a UZE
/// shim, while keeping installer/network behavior out of `cargo test`.
#[test]
fn setup_conformance_matrix_covers_every_registered_harness() {
    use uze_core::home::UzeHome;
    use uze_integrations::registry::IntegrationRegistry;

    let registry_root = temporary_home("cli-setup-conformance-registry");
    let registry_home = UzeHome::at(registry_root.join("uze"));
    let registry = IntegrationRegistry::isolated(&registry_root, &registry_home);
    let mut registered = registry.ids();
    registered.sort_unstable();
    let mut covered = SETUP_CONFORMANCE_HARNESSES
        .iter()
        .map(|(id, _, _)| *id)
        .collect::<Vec<_>>();
    covered.sort_unstable();
    assert_eq!(
        covered, registered,
        "every registered harness needs setup conformance"
    );

    let home = temporary_home("cli-setup-conformance-home");
    let uze_home = temporary_home("cli-setup-conformance-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-setup-conformance-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);

    for (harness, executable, update_command) in SETUP_CONFORMANCE_HARNESSES {
        let output = Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", &path)
            .args(["setup", harness])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "uze setup {harness} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("up to date"),
            "unexpected setup output for {harness}: {stdout}"
        );
        let executable = uze_platform::executable::file_name(executable);
        assert!(
            uze_home.join("shims").join(&executable).exists(),
            "uze setup {harness} must create its default {executable} shim"
        );
        let commands = std::fs::read_to_string(fake_bin.join("commands.log")).unwrap();
        let reached = format!("{}{executable}|{update_command}", std::path::MAIN_SEPARATOR);
        assert!(
            commands.lines().any(|line| line.ends_with(&reached)),
            "{harness} did not use the documented {update_command} route through the resolved real {executable} binary: {commands}"
        );
    }

    let _ = std::fs::remove_dir_all(registry_root);
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

#[test]
// OpenCode's installer is POSIX shell; Windows gets its distributed build instead.
#[cfg(unix)]
fn setup_opencode_legacy_binary_uses_installer_not_stable_upgrade() {
    let home = temporary_home("cli-setup-opencode2-home");
    let uze_home = temporary_home("cli-setup-opencode2-uze-home");
    let fake_bin = fake_legacy_opencode_bin_dir("cli-setup-opencode2-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);

    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &uze_home)
        .isolated_home(&home)
        .env("PATH", &path)
        .args(["setup", "opencode"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "uze setup opencode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = std::fs::read_to_string(fake_bin.join("commands.log")).unwrap();
    assert!(commands.contains("opencode.ai/v2/install"), "{commands}");
    assert!(commands.contains("opencode2|--version"), "{commands}");
    assert!(
        !commands.contains("opencode2|upgrade"),
        "legacy OpenCode must not receive stable-only `upgrade`: {commands}"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// `uze setup codex` with Codex present takes its update route and records
/// the version it verified afterwards, as the provisioning history.
#[test]
fn setup_codex_records_the_version_it_verified_after_the_update() {
    use uze_core::provisioning::{ProvisionAction, ProvisionStatus};

    let home = temporary_home("cli-setup-codex-version-home");
    let uze_home = temporary_home("cli-setup-codex-version-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-setup-codex-version-bin");
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &uze_home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::path_with(&[&fake_bin]))
        .args(["setup", "codex"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let record = uze_core::state::provisioning(&uze_core::UzeHome::at(&uze_home), "codex")
        .unwrap()
        .expect("the setup is recorded");
    assert_eq!(record.action, ProvisionAction::Update);
    assert_eq!(record.status, ProvisionStatus::Verified);
    assert_eq!(record.version.as_deref(), Some("9.9.9"));

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// A plugin installed while no harness exists reaches Claude Code when a
/// later `uze setup claude-code` finds one, as the one native package
/// Claude is handed: no capability-level copy beside it, and nothing
/// delivered twice by a second setup.
#[test]
fn setup_delivers_a_package_stored_before_the_harness_once_and_natively() {
    let home = temporary_home("cli-setup-after-add-home");
    let uze_home = temporary_home("cli-setup-after-add-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-setup-after-add-bin");
    let claude_receipts = || -> Vec<serde_json::Value> {
        let ledger: serde_json::Value = std::fs::read(uze_home.join("state/attachments.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        ledger["receipts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|receipt| {
                receipt["integration"] == "claude-code"
                    && receipt["package_id"] == "uze-agent-skill-conformance@test"
            })
            .collect()
    };

    let add = install_via_marketplace(
        &home,
        &uze_home,
        &package_fixture(),
        &uze_testkit::process::system_path(),
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(
        claude_receipts().is_empty(),
        "no harness existed, so nothing was delivered"
    );

    let setup = || {
        let output = Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", uze_testkit::process::path_with(&[&fake_bin]))
            .args(["setup", "claude-code"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    setup();
    let delivered = claude_receipts();
    assert_eq!(
        delivered.len(),
        1,
        "one package-level delivery: {delivered:?}"
    );
    assert_eq!(
        delivered[0]["artifact"]["INTEGRATION_OWNED"]["kind"], "claude-plugin-generated",
        "{delivered:?}"
    );
    assert!(
        delivered[0]["resource_identity"].is_null(),
        "the package is delivered whole, not capability by capability: {delivered:?}"
    );
    let skills = std::fs::read_dir(home.join(".claude/skills"))
        .unwrap()
        .count();
    assert_eq!(
        skills, 0,
        "no capability-level copy beside the native package"
    );

    setup();
    assert_eq!(
        claude_receipts(),
        delivered,
        "a second setup delivers nothing twice"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// A fresh OpenCode install lands where its official installer puts it,
/// `~/.opencode/bin`, and only edits the shell's rc files: `uze setup
/// opencode` verifies it there and says where it is, and that the shell it
/// runs in does not reach it until a new one starts.
#[test]
// Drives the POSIX installer route (`curl … | sh`) with stand-ins.
#[cfg(unix)]
fn setup_opencode_reports_where_a_fresh_install_landed_outside_path() {
    use std::os::unix::fs::PermissionsExt;

    let home = temporary_home("cli-setup-opencode-fresh-home");
    let uze_home = temporary_home("cli-setup-opencode-fresh-uze-home");
    let fake_bin = temporary_home("cli-setup-opencode-fresh-bin");
    std::fs::create_dir_all(&fake_bin).unwrap();
    let installed = home.join(".opencode/bin/opencode");
    // Stands in for `sh -c "installer=$(curl ...) && ... | bash"`: it
    // records the route it was handed and installs where the real one does.
    let sh = fake_bin.join("sh");
    std::fs::write(
        &sh,
        format!(
            "#!/bin/sh\necho \"$*\" >> \"{log}\"\nmkdir -p \"{dir}\"\nprintf '#!/bin/sh\\necho opencode v9.9.9\\n' > \"{bin}\"\nchmod 755 \"{bin}\"\n",
            log = fake_bin.join("commands.log").display(),
            dir = installed.parent().unwrap().display(),
            bin = installed.display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &uze_home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::path_with(&[&fake_bin]))
        .env_remove("OPENCODE_INSTALL_DIR")
        .env_remove("XDG_BIN_DIR")
        .args(["setup", "opencode"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = std::fs::read_to_string(fake_bin.join("commands.log")).unwrap();
    assert!(commands.contains("opencode.ai/v2/install"), "{commands}");
    assert!(installed.is_file());
    assert!(
        stdout.contains("v9.9.9") && stdout.contains("installed"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "installed at {}",
            installed.strip_prefix(&home).map_or_else(
                |_| installed.display().to_string(),
                |rest| format!("~/{}", rest.display())
            )
        )) && stdout.contains("open a new shell"),
        "{stdout}"
    );
    assert!(
        home.join(".config/opencode/skills").is_dir(),
        "prepared after verification"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// A fresh Claude Code or Codex install lands where its native installer
/// puts it, `~/.local/bin/<program>`, which the shell `uze setup` runs in
/// does not reach yet: setup verifies it there, says where it is, puts the
/// shim in front of it, and warns about nothing.
// Drives the POSIX installer route (`curl … | sh`) with stand-ins.
#[cfg(unix)]
fn assert_fresh_native_install_found_outside_path(
    program: &str,
    display_name: &str,
    installer: &str,
    version_line: &str,
) {
    use std::os::unix::fs::PermissionsExt;

    let home = temporary_home(&format!("cli-setup-{program}-fresh-home"));
    let uze_home = temporary_home(&format!("cli-setup-{program}-fresh-uze-home"));
    let fake_bin = temporary_home(&format!("cli-setup-{program}-fresh-bin"));
    std::fs::create_dir_all(&fake_bin).unwrap();
    let installed = home.join(".local/bin").join(program);
    // Stands in for the vendor's `curl ... | sh` route: it records the
    // route it was handed and installs where the real one does.
    let sh = fake_bin.join("sh");
    std::fs::write(
        &sh,
        format!(
            "#!/bin/sh\necho \"$*\" >> \"{log}\"\nmkdir -p \"{dir}\"\nprintf '#!/bin/sh\\necho \"$0|$*\" >> \"{log}\"\\nif [ \"$1\" = --version ]; then echo \"{version_line}\"; fi\\n' > \"{bin}\"\nchmod 755 \"{bin}\"\n",
            log = fake_bin.join("commands.log").display(),
            dir = installed.parent().unwrap().display(),
            bin = installed.display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &uze_home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::path_with(&[&fake_bin]))
        .args(["setup", program])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}{stderr}");
    let commands = std::fs::read_to_string(fake_bin.join("commands.log")).unwrap();
    assert!(commands.contains(installer), "{commands}");
    assert!(
        commands.contains(&format!("{}|--version", installed.display())),
        "verified at the installer's destination: {commands}"
    );
    assert!(
        stdout.contains(display_name) && stdout.contains("9.9.9") && stdout.contains("installed"),
        "{stdout}{stderr}"
    );
    assert!(
        stdout.contains(&format!(
            "installed at {}",
            installed.strip_prefix(&home).map_or_else(
                |_| installed.display().to_string(),
                |rest| format!("~/{}", rest.display())
            )
        )) && stdout.contains("open a new shell"),
        "{stdout}"
    );
    assert!(
        !format!("{stdout}{stderr}").contains("warning"),
        "a fresh install warns about nothing: {stdout}{stderr}"
    );
    assert!(
        uze_home.join("shims").join(program).exists(),
        "the shim stands in front of the binary the next shell reaches"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

#[test]
// Drives the POSIX installer route (`curl … | sh`) with stand-ins.
#[cfg(unix)]
fn setup_claude_reports_where_a_fresh_install_landed_outside_path() {
    assert_fresh_native_install_found_outside_path(
        "claude",
        "Claude Code",
        "claude.ai/install.sh",
        "9.9.9 (Claude Code)",
    );
}

#[test]
// Drives the POSIX installer route (`curl … | sh`) with stand-ins.
#[cfg(unix)]
fn setup_codex_reports_where_a_fresh_install_landed_outside_path() {
    assert_fresh_native_install_found_outside_path(
        "codex",
        "Codex",
        "chatgpt.com/codex/install.sh",
        "codex-cli 9.9.9",
    );
}

/// Deterministic end-to-end: `uze setup` against fake, PATH-resolvable
/// `claude`/`codex` executables (so no real harness install is required to
/// run this test), then `uze add` alone attaching the shared fixture skill
/// for both — matching the target `uze setup` / `uze add` / plain harness
/// invocation experience, minus the real invocation itself. Setup running
/// twice must not duplicate recorded state or managed artifacts.
#[test]
fn setup_then_add_attaches_transparently_without_a_separate_sync_step() {
    let home = temporary_home("cli-setup-then-add-home");
    let uze_home = temporary_home("cli-setup-then-add-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-setup-then-add-bin");
    let path = std::env::join_paths(
        std::iter::once(fake_bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();

    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", &path)
            .args(args)
            .output()
            .unwrap();
        // `doctor` fails the command over what it finds, and fake harnesses
        // are always found wanting somewhere; its report is what is read.
        assert!(
            output.status.success() || args == ["doctor"],
            "uze {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let setup_once = run(&["setup"]);
    assert!(
        setup_once.contains("Claude Code") && setup_once.contains("9.9.9"),
        "{setup_once}"
    );
    assert!(setup_once.contains("Codex"), "{setup_once}");
    assert!(home.join(".claude/skills").is_dir());
    assert!(home.join(".agents/skills").is_dir());

    // Idempotent: a second `uze setup` does not fail or duplicate state.
    run(&["setup"]);
    let doctor = run(&["doctor"]);
    // Both fake harnesses' provisioning reported `Verified` above, and `status()` reflects that once recorded —
    // see `IntegrationPort::status`'s doc comment.
    assert!(doctor.matches("✓ set up").count() >= 2, "{doctor}");

    // `uze install -m` alone attaches both, without any separate sync
    // command.
    //
    // Both the default `uze` package and this single-skill fixture qualify
    // for Generated Native Package (ADR-013: no explicit `.claude-plugin/
    // plugin.json`, but a conventional `skills/` directory UZE can safely
    // represent) — so Claude receives package-level delivery, not a
    // per-resource `.claude/skills` symlink. Codex has no envelope for
    // either package and still decomposes at the capability level, so its
    // resource-level attachment output is unchanged.
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&home, &package_fixture());
    run(&market_args.iter().map(String::as_str).collect::<Vec<_>>());
    // The route each harness took is the detailed report's to say.
    let mut add_args: Vec<&str> = install_args.iter().map(String::as_str).collect();
    add_args.push("--verbose");
    let add = run(&add_args);
    assert!(
        add.contains("Claude Code  native package, generated manifest"),
        "{add}"
    );
    assert!(
        add.contains("Codex  native package, generated manifest"),
        "{add}"
    );

    // `.claude/skills` is prepared (by `install`) but stays empty: neither
    // package decomposes into it anymore. The generated envelope directory
    // is where delivery now lives, one subdirectory per generatable
    // package, each independently rebuildable from the Store.
    let claude_skills_entries: Vec<_> = std::fs::read_dir(home.join(".claude/skills"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(
        claude_skills_entries.is_empty(),
        "no package should decompose into .claude/skills once generatable, got {claude_skills_entries:?}"
    );
    let generated_root = uze_home.join("runtime/attachments/claude/generated");
    assert!(
        generated_root
            .join("uze-agent-skill-conformance@test/.claude-plugin/plugin.json")
            .is_file(),
        "the fixture's generated Claude envelope should exist"
    );
    // The envelope is a copy of the package: Claude's own copy into its
    // plugin cache does not follow links, so a linked skill arrived empty.
    let skill = generated_root.join("uze-agent-skill-conformance@test/skills/uze-e2e");
    assert!(
        !skill.is_symlink() && skill.join("SKILL.md").is_file(),
        "the generated envelope should carry the skill as real files"
    );
    assert!(
        generated_root
            .join("uze@uze-official/.claude-plugin/plugin.json")
            .is_file(),
        "the default uze package's generated Claude envelope should exist too"
    );

    // OpenCode has no plugin, so each skill lands in its own root as a
    // directory of its own.
    let opencode_entries: Vec<_> = std::fs::read_dir(home.join(".config/opencode/skills"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(
        opencode_entries.len() >= 2,
        "OpenCode should have default + fixture, got {opencode_entries:?}"
    );
    assert!(opencode_entries.iter().all(|p| !p.is_symlink()));
    assert!(
        contains_fixture_skill(&opencode_entries),
        "OpenCode should contain the qualified fixture skill"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// A user with OpenCode already installed should not have to learn that an
/// extra UZE setup step is required before their first package works. `add`
/// detects the executable, prepares only UZE-owned prerequisites, then
/// attaches the package through the normal integration lifecycle.
#[test]
fn add_prepares_a_detected_opencode_and_attaches_without_prior_setup() {
    let home = temporary_home("cli-add-autoprepares-opencode-home");
    let uze_home = temporary_home("cli-add-autoprepares-opencode-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-add-autoprepares-opencode-bin");
    let path = std::env::join_paths(
        std::iter::once(fake_bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();

    let output = install_via_marketplace(&home, &uze_home, &package_fixture(), &path);
    assert!(
        output.status.success(),
        "uze add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let skills_dir = home.join(".config/opencode/skills");
    let entries: Vec<_> = std::fs::read_dir(&skills_dir)
        .expect("detected OpenCode should have its skills delivered")
        .map(|entry| entry.unwrap().path())
        .collect();
    // Default `uze` (`uze-uze`) plus the fixture.
    assert!(
        entries.len() >= 2,
        "should have default + fixture, got {entries:?}"
    );
    assert!(entries.iter().all(|p| !p.is_symlink()));
    assert!(
        contains_fixture_skill(&entries),
        "the qualified fixture skill should be present alongside the default plugin"
    );

    let integrations = std::fs::read_to_string(uze_home.join("cache/harnesses.json")).unwrap();
    assert!(integrations.contains("\"opencode\""));

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// Deterministic end-to-end for MCP (see ADR-007): fake, PATH-resolvable
/// `claude`/`codex` scripts that understand `mcp get`/`mcp add`/`mcp
/// remove` well enough to prove `uze setup` + `uze add` registers the MCP
/// fixture for both, idempotently, without a real harness binary. No
/// network, credentials, or LLM involved — this only proves the
/// attach/idempotency/removal mechanics, not real harness behavior.
#[test]
fn setup_then_add_attaches_the_mcp_fixture_idempotently_and_removal_works() {
    let home = temporary_home("cli-mcp-home");
    let uze_home = temporary_home("cli-mcp-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-mcp-bin");
    let mcp_package_dir = temporary_home("cli-mcp-package");
    let package = mcp_package_fixture_with_resolved_binary(&mcp_package_dir);
    let path = std::env::join_paths(
        std::iter::once(fake_bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();

    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", &path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "uze {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    run(&["setup"]);
    // This fixture (`agent-plugin-mcp`) has only `mcp.json`, no `skills/`
    // and no `.claude-plugin/plugin.json`/`.codex-plugin/plugin.json` —
    // under Generated Native Package (ADR-013) an MCP-only package
    // is just as eligible as a Skill-only one, so BOTH Claude and Codex now
    // receive package-level delivery covering the one MCP resource — no
    // resource-level `mcp add` for either. Opencode has no package envelope
    // and stays resource-level as a native `mcp.servers` config entry.
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&home, &package);
    run(&market_args.iter().map(String::as_str).collect::<Vec<_>>());
    // The route each harness took is the detailed report's to say.
    let mut add_args: Vec<&str> = install_args.iter().map(String::as_str).collect();
    add_args.push("--verbose");
    let add = run(&add_args);
    assert!(
        add.contains("Claude Code  native package, generated manifest"),
        "{add}"
    );
    assert!(
        add.contains("Codex  native package, generated manifest"),
        "{add}"
    );

    // Opencode is resource-level native (no package envelope): UZE writes
    // the `mcp.servers` entry itself, and the receipt is the source of truth.
    let ledger: serde_json::Value =
        serde_json::from_slice(&std::fs::read(uze_home.join("state/attachments.json")).unwrap())
            .unwrap();
    let receipts = ledger["receipts"].as_array().unwrap();
    assert!(receipts.len() >= 2);
    // With the default `uze` seeded, attachments also contain its own
    // package-level receipts (the default package is generatable too, see
    // the skill-fixture CLI test); filter to this MCP package's receipts
    // before asserting their shape.
    let mcp_receipts: Vec<_> = receipts
        .iter()
        .filter(|receipt| receipt["package_id"] == "uze-mcp-conformance@test")
        .collect();
    assert!(
        mcp_receipts.len() >= 3,
        "expected at least 3 receipts for the MCP package (claude + codex package-level + opencode resource-level), got {mcp_receipts:?}"
    );
    let opencode_receipt = mcp_receipts
        .iter()
        .find(|r| r["integration"] == "opencode")
        .expect("opencode receipt missing");
    assert!(
        opencode_receipt["artifact"]
            .get("VENDOR_CONFIG_ENTRY")
            .is_some()
            || opencode_receipt["artifact"]
                .get("VendorConfigEntry")
                .is_some(),
        "opencode's receipt for the MCP package must be resource-level VendorConfigEntry (no package), got {opencode_receipt:?}"
    );
    let claude_receipt = mcp_receipts
        .iter()
        .find(|r| r["integration"] == "claude-code")
        .expect("claude receipt missing");
    assert_eq!(
        claude_receipt["artifact"]["INTEGRATION_OWNED"]["kind"], "claude-plugin-generated",
        "claude's receipt for an envelope-less MCP-only package must be the generated package kind, not a resource-level VendorConfigEntry: {claude_receipt:?}"
    );
    assert_eq!(
        claude_receipt["artifact"]["INTEGRATION_OWNED"]["origin"],
        "generated"
    );
    let codex_receipt = mcp_receipts
        .iter()
        .find(|r| r["integration"] == "codex")
        .expect("codex receipt missing");
    assert_eq!(
        codex_receipt["artifact"]["INTEGRATION_OWNED"]["kind"], "marketplace-plugin-generated",
        "codex's receipt for an envelope-less MCP-only package must be the generated package kind, not a resource-level VendorConfigEntry: {codex_receipt:?}"
    );
    assert_eq!(
        codex_receipt["artifact"]["INTEGRATION_OWNED"]["origin"],
        "generated"
    );

    // Idempotent: `install -m` a second time does not fail. Both
    // integrations' package delivery re-resolves to the same
    // already-installed selector — no reinstall, no resource-level replay.
    let second_add = run(&add_args);
    assert!(
        second_add.contains("Claude Code  native package, generated manifest"),
        "{second_add}"
    );
    assert!(
        second_add.contains("Codex  native package, generated manifest"),
        "{second_add}"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
    let _ = std::fs::remove_dir_all(mcp_package_dir);
}

/// `uze remove <plugin> -m` — the machine-level verb (renamed from the old,
/// unnamespaced root `remove`, which used to reach this exact flow via an
/// implicit fallback — see `plugin_remove_never_confused_with_project_remove`
/// / ADR-019 for why that fallback is gone).
#[test]
fn plugin_remove_uses_the_package_centric_application_flow() {
    let home = temporary_home("cli-remove");
    let add = install_via_marketplace(
        &home,
        &home,
        &package_fixture(),
        &uze_testkit::process::system_path(),
    );
    assert!(add.status.success());
    let remove = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::system_path())
        .args([
            "remove",
            "-m",
            "uze-agent-skill-conformance",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(remove.status.success());
    let report: serde_json::Value = serde_json::from_slice(&remove.stdout).unwrap();
    assert_eq!(report["outcome"], "REMOVED");
    let list = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .args(["status", "-m", "--format", "json"])
        .output()
        .unwrap();
    assert!(list.status.success());
    // With the default `uze` seeded, the machine read model is not empty
    // after removing the fixture — the default plugin remains. Filter it
    // out for this test's original assertion that the user-added package
    // is gone.
    let plugins = serde_json::from_slice::<serde_json::Value>(&list.stdout).unwrap()["packages"]
        .as_array()
        .unwrap()
        .clone();
    let non_default: Vec<_> = plugins
        .iter()
        .filter(|p| p["id"] != "uze@uze-official")
        .collect();
    assert!(
        non_default.is_empty(),
        "expected no non-default plugins after remove, got {plugins:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// Every name on record is lowercase, so the case a person types is
/// forgiven: `Name@Test` installs `name@test`, and `NAME` removes it.
#[test]
fn a_name_typed_in_another_case_resolves_to_the_one_on_record() {
    let home = temporary_home("cli-typed-case");
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&home, &package_fixture());
    let uze = |args: &[String]| {
        Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &home)
            .isolated_home(&home)
            .env("PATH", uze_testkit::process::system_path())
            .current_dir(&home)
            .args(args)
            .output()
            .unwrap()
    };
    assert!(uze(&market_args).status.success());
    let spec = install_args.last().unwrap();
    let shouted: Vec<String> = install_args
        .iter()
        .map(|arg| {
            if arg == spec {
                arg.to_uppercase()
            } else {
                arg.clone()
            }
        })
        .collect();
    let install = uze(&shouted);
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let plugin = spec.split_once('@').unwrap().0;
    let remove = uze(&[
        "remove".to_owned(),
        "-m".to_owned(),
        plugin.to_uppercase(),
        "--format".to_owned(),
        "json".to_owned(),
    ]);
    assert!(
        remove.status.success(),
        "{}",
        String::from_utf8_lossy(&remove.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&remove.stdout).unwrap();
    assert_eq!(report["outcome"], "REMOVED");
    let _ = std::fs::remove_dir_all(home);
}

/// ADR-019's central breaking change: root `uze remove` is strictly
/// project-scoped. Run with no `agents.lock` anywhere in `home`'s ancestry
/// (a plain temp dir, not a project), this must now fail loudly — never
/// silently fall through to removing the machine-installed package, the
/// pre-ADR-019 behavior `plugin_remove_uses_the_package_centric_application_flow`
/// exercises deliberately instead.
#[test]
fn root_remove_no_longer_falls_back_to_global_removal() {
    let home = temporary_home("cli-remove-no-fallback");
    let add = install_via_marketplace(
        &home,
        &home,
        &package_fixture(),
        &uze_testkit::process::system_path(),
    );
    assert!(add.status.success());

    // `current_dir(&home)` matters here: this repo's own root (the ambient
    // cwd `cargo test` runs from) has a real `agents.lock` of its own
    // (UZE dogfoods itself) — running from there would resolve a *real*
    // project root and hit `NotInLock` instead of the `NoLock` case this
    // test targets. `home` is a fresh temp dir with no `agents.lock`,
    // `AGENTS.md`, or `.git` anywhere in its ancestry.
    let remove = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", uze_testkit::process::system_path())
        .current_dir(&home)
        .args(["remove", "uze-agent-skill-conformance"])
        .output()
        .unwrap();
    assert!(
        !remove.status.success(),
        "root `remove` outside a project must fail, not silently succeed"
    );
    let stderr = String::from_utf8_lossy(&remove.stderr);
    assert!(
        stderr.contains("no project environment found"),
        "expected a no-project-environment error, got: {stderr}"
    );
    assert!(
        stderr.contains("uze remove"),
        "error should point at the machine-level equivalent, got: {stderr}"
    );

    // The whole point: the machine-installed package must survive untouched.
    let list = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .args(["status", "-m", "--format", "json"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let plugins = serde_json::from_slice::<serde_json::Value>(&list.stdout).unwrap()["packages"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        plugins
            .iter()
            .any(|p| p["id"] == "uze-agent-skill-conformance@test"),
        "package must survive a failed project-scoped remove, got {plugins:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// Edits the SKILL.md of one skill directory this package owns by hand,
/// so reconciliation reports `Drifted` and the removal plan refuses to
/// touch it — the lifecycle-safety outcome `Blocked` reports.
fn drift_a_managed_attachment(home: &std::path::Path) {
    let skills = home.join(".config/opencode/skills");
    let managed = std::fs::read_dir(&skills)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|entry| contains_fixture_skill(std::slice::from_ref(entry)))
        .expect("the fixture attaches at least one managed skill directory");
    std::fs::write(managed.join("SKILL.md"), "edited by hand\n").unwrap();
}

/// `Blocked` means the safety check refused and nothing was removed. The
/// report used to print and the process exit 0, so
/// `uze remove x -m && uze install y -m` ran the second half after
/// the first had done nothing.
#[test]
fn a_blocked_removal_reports_and_fails() {
    let home = temporary_home("cli-remove-blocked-home");
    let uze_home = temporary_home("cli-remove-blocked-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-remove-blocked-bin");
    let path = std::env::join_paths(
        std::iter::once(fake_bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();

    let add = install_via_marketplace(&home, &uze_home, &package_fixture(), &path);
    assert!(
        add.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    drift_a_managed_attachment(&home);

    for format in ["text", "json"] {
        let removal = Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", &path)
            .args([
                "remove",
                "-m",
                "uze-agent-skill-conformance",
                "--format",
                format,
            ])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&removal.stdout);
        assert!(
            stdout.contains("blocked") || stdout.contains("BLOCKED"),
            "the report must still say what happened, got: {stdout}"
        );
        assert!(
            !removal.status.success(),
            "a blocked removal reported success ({format}): {stdout}"
        );
    }

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// The same for `uze update -m`, which blocks on the same check: it
/// removes the installed package before putting the new one in place.
#[test]
fn a_blocked_update_reports_and_fails() {
    let home = temporary_home("cli-update-blocked-home");
    let uze_home = temporary_home("cli-update-blocked-uze-home");
    let fake_bin = fake_harness_bin_dir("cli-update-blocked-bin");
    let path = std::env::join_paths(
        std::iter::once(fake_bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();

    let add = install_via_marketplace(&home, &uze_home, &package_fixture(), &path);
    assert!(
        add.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    drift_a_managed_attachment(&home);

    let update = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &uze_home)
        .isolated_home(&home)
        .env("PATH", &path)
        .args([
            "update",
            "-m",
            "uze-agent-skill-conformance",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&update.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("the report must stay parseable ({error}): {stdout}"));
    assert_eq!(
        report["outcomes"][0]["outcome"], "BLOCKED",
        "the report must still say what happened, got: {stdout}"
    );
    assert!(
        !update.status.success(),
        "a blocked update reported success: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&update.stderr);
    assert!(
        !stderr.contains("nothing was changed"),
        "the failure must not claim more than it knows: {stderr}"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// Updating a machine whose packages are already at their source's head
/// moves nothing, and says so — every time it is asked, under a header
/// that names the machine rather than a project that is not there.
#[test]
fn a_machine_update_with_nothing_new_says_already_current() {
    let home = temporary_home("cli-update-current-home");
    let uze_home = temporary_home("cli-update-current-uze-home");
    let path = uze_testkit::process::system_path();

    let add = install_via_marketplace(&home, &uze_home, &package_fixture(), &path);
    assert!(
        add.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    for _ in 0..2 {
        let update = Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", &path)
            // The scope and each plugin's own line are the detailed report's.
            .args(["update", "-m", "--verbose"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&update.stdout);
        assert!(
            update.status.success(),
            "{} {stdout}",
            String::from_utf8_lossy(&update.stderr)
        );
        assert!(
            stdout.contains("Packages on this machine") && !stdout.contains("This project"),
            "got: {stdout}"
        );
        assert!(
            stdout.contains("already current") && !stdout.contains("moved to"),
            "an update that moved nothing claimed a move: {stdout}"
        );
    }

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(uze_home);
}

/// An author edits a skill in the checkout a marketplace is linked to and
/// asks the machine to update the plugin: the Store takes the edit, and the
/// report says it moved and where from — never "already current" over bytes
/// that just changed.
#[test]
fn a_machine_update_of_a_linked_edit_says_it_moved_from_the_working_tree() {
    let home = temporary_home("cli-update-linked-home");
    let uze_home = home.join("uze");
    let market = home.join("market");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join(".gitconfig"),
        "[user]\n\tname = Test\n\temail = t@example.invalid\n",
    )
    .unwrap();
    let uze = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_uze"))
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", uze_testkit::process::system_path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "uze {args:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    uze(&[
        "agent",
        "market",
        "create",
        "tools",
        "--at",
        market.to_str().unwrap(),
    ]);
    uze(&["agent", "plugin", "create", "greet", "--market", "tools"]);
    uze(&["install", "-m", "greet@tools"]);
    let stored_skill = || {
        let store = uze_home.join("store/plugins");
        [store.join("tools/greet"), store.join("greet")]
            .iter()
            .map(|root| root.join("skills/greet/SKILL.md"))
            .find_map(|skill| std::fs::read_to_string(skill).ok())
            .expect("the Store carries greet@tools")
    };
    assert!(!stored_skill().contains("edited, never committed"));

    let skill = market.join("plugins/greet/skills/greet/SKILL.md");
    let edited = format!(
        "{}\nedited, never committed\n",
        std::fs::read_to_string(&skill).unwrap()
    );
    std::fs::write(&skill, edited).unwrap();

    let update = uze(&["update", "greet", "-m"]);
    assert!(
        stored_skill().contains("edited, never committed"),
        "a linked marketplace follows the working tree"
    );
    assert!(
        !update.contains("already current") && update.contains("linked working tree"),
        "the Store took the edit in, and the report must say so: {update}"
    );

    let again = uze(&["update", "greet", "-m"]);
    assert!(
        again.contains("up to date"),
        "nothing was edited since: {again}"
    );

    // A project declaring the plugin re-ingests the working tree on its own
    // update: that ingest is what left a later machine update with nothing
    // to do, so it is the one that has to say it happened.
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let in_project = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_uze"))
            .current_dir(&project)
            .env("UZE_HOME", &uze_home)
            .isolated_home(&home)
            .env("PATH", uze_testkit::process::system_path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "uze {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    for git in [
        &["init", "-q"][..],
        &["commit", "-q", "--allow-empty", "-m", "init"],
    ] {
        assert!(
            Command::new("git")
                .current_dir(&project)
                .isolated_home(&home)
                .args(git)
                .status()
                .unwrap()
                .success()
        );
    }
    in_project(&["install", "greet@tools"]);
    std::fs::write(
        &skill,
        format!(
            "{}\nedited again\n",
            std::fs::read_to_string(&skill).unwrap()
        ),
    )
    .unwrap();
    let project_update = in_project(&["update"]);
    assert!(stored_skill().contains("edited again"));
    assert!(
        project_update.contains("updated from the linked working tree")
            && project_update.contains("pins nothing"),
        "the project update ingested the edit and must say so: {project_update}"
    );
    let after_project = uze(&["update", "greet", "-m"]);
    assert!(
        after_project.contains("up to date"),
        "the edit was taken in, and reported, by the project update: {after_project}"
    );

    let _ = std::fs::remove_dir_all(home);
}

/// A bin directory whose `agy` is detected and refuses every plugin install,
/// optionally beside an `opencode` that delivers.
fn refusing_harness_bin_dir(label: &str, with_opencode: bool) -> PathBuf {
    let dir = temporary_home(label);
    std::fs::create_dir_all(&dir).unwrap();
    FakeHarness::new(&dir, "agy")
        .version_line("agy 9.9.9")
        .on_prefix(["plugin", "list"], Action::stdout(r#"{"imports":[]}"#))
        .on_prefix(
            ["plugin", "install"],
            Action::Refuse {
                reason: "the plugin cache is read-only".to_owned(),
            },
        )
        .on_prefix(["plugin"], Action::Exit(0))
        .on_prefix([""], Action::stdout("agy 9.9.9"))
        .build();
    if with_opencode {
        opencode_answering(&dir);
    }
    dir
}

/// A stand-in OpenCode in `bin` that answers every call with its version.
fn opencode_answering(bin: &std::path::Path) {
    FakeHarness::new(bin, "opencode")
        .version_line("opencode v9.9.9")
        .on_prefix([""], Action::stdout("opencode v9.9.9"))
        .build();
}

fn machine_json(
    home: &std::path::Path,
    path: &std::ffi::OsStr,
    args: &[&str],
) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", home)
        .isolated_home(home)
        .env("PATH", path)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?} must answer in JSON ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// "Failed to install" used to leave the package installed: `status -m`
/// listed it and `doctor` counted its receipts as matched.
#[test]
fn an_install_the_only_harness_refuses_is_not_installed() {
    let home = temporary_home("cli-install-refused");
    let fake_bin = refusing_harness_bin_dir("cli-install-refused-bin", false);
    let path = uze_testkit::process::path_with(&[&fake_bin]);

    let add = install_via_marketplace(&home, &home, &package_fixture(), &path);
    let stderr = String::from_utf8_lossy(&add.stderr);
    assert!(!add.status.success(), "a refused install reported success");
    assert!(
        stderr.contains("the plugin cache is read-only")
            && stderr.contains("Nothing was installed"),
        "the harness's refusal is named: {stderr}"
    );

    // UZE's own plugin re-seeds itself, so it stays — recorded as not
    // delivered — and the package that was asked for is the one that must
    // be gone.
    let asked_for = "uze-agent-skill-conformance@test";
    let status = machine_json(&home, &path, &["status", "-m"]);
    let listed = status["packages"].as_array().expect("packages");
    assert!(
        listed.iter().all(|package| package["id"] != asked_for),
        "{status}"
    );
    assert!(
        listed
            .iter()
            .all(|package| !package["undelivered"].as_array().unwrap().is_empty()),
        "what stays says it was not delivered: {status}"
    );
    let doctor = machine_json(&home, &path, &["doctor"]);
    assert!(
        doctor["attachments"]
            .as_array()
            .expect("attachments")
            .iter()
            .all(|package| package["plugin"] != asked_for),
        "{doctor}"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// One harness takes the package and the other refuses: the install
/// fails, the package stays, and the listing names what it did not reach.
#[test]
fn an_install_one_harness_refuses_is_listed_as_partially_delivered() {
    let home = temporary_home("cli-install-partial");
    let fake_bin = refusing_harness_bin_dir("cli-install-partial-bin", true);
    let path = uze_testkit::process::path_with(&[&fake_bin]);

    let add = install_via_marketplace_json(&home, &home, &package_fixture(), &path);
    assert!(!add.status.success(), "a partial install reported success");
    let report: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap_or_else(|error| {
        panic!(
            "the report is still printed ({error}): {}",
            String::from_utf8_lossy(&add.stdout)
        )
    });
    let deliveries = report["deliveries"].as_array().expect("deliveries");
    let failed: Vec<&serde_json::Value> = deliveries
        .iter()
        .filter(|delivery| delivery["outcome"] == "failed")
        .collect();
    assert_eq!(failed.len(), 1, "{report}");
    assert!(
        failed[0]["error"]
            .as_str()
            .is_some_and(|error| error.contains("read-only")),
        "{report}"
    );
    assert!(
        deliveries
            .iter()
            .any(|delivery| delivery["outcome"] == "delivered"),
        "{report}"
    );

    let status = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", &path)
        .args(["status", "-m"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(
        text.contains("partially delivered") && text.contains("read-only"),
        "{text}"
    );

    let doctor = Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", &home)
        .isolated_home(&home)
        .env("PATH", &path)
        .args(["doctor"])
        .output()
        .unwrap();
    let doctor = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        doctor.contains("not delivered") && doctor.contains("read-only"),
        "doctor names what the package did not reach: {doctor}"
    );

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// An agent an earlier build delivered under its bare file name — a link
/// named `reviewer.md` in OpenCode's agents directory — is taken back off
/// on the next install and replaced by the file carrying its plugin's
/// label, so an upgrade never leaves the old name answering beside the new.
#[test]
fn an_agent_delivered_under_its_old_bare_name_is_renamed_on_the_next_install() {
    use std::fs;
    use uze_core::{UzeHome, exposure::ManagedArtifact, state};

    let home = temporary_home("cli-agent-rename");
    let package = home.join("crew");
    fs::create_dir_all(package.join("agents")).unwrap();
    fs::write(package.join("plugin.json"), r#"{"name": "crew"}"#).unwrap();
    fs::write(
        package.join("agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews a change.\n---\nReview.\n",
    )
    .unwrap();
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    opencode_answering(&bin);
    let path = uze_testkit::process::path_with(&[&bin]);
    let uze_home = home.join(".uze");

    let first = install_via_marketplace_json(&home, &uze_home, &package, &path);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let agents = home.join(".config/opencode/agents");
    let labelled = agents.join(format!(
        "{}.md",
        uze_core::path::file_name_for("crew:reviewer")
    ));
    assert!(labelled.is_file() && !labelled.is_symlink());

    // The ledger and the disk as 1.0.0-beta.3 left them.
    let uze = UzeHome::at(&uze_home);
    let receipt = state::receipts(&uze, None)
        .unwrap()
        .into_iter()
        .find(|receipt| matches!(receipt.artifact, ManagedArtifact::GeneratedFile { .. }))
        .expect("the agent's receipt");
    state::forget_receipt(&uze, &receipt).unwrap();
    fs::remove_file(&labelled).unwrap();
    let store_definition = fs::read_dir(uze.store_dir().join("plugins"))
        .unwrap()
        .flat_map(|market| fs::read_dir(market.unwrap().path()).unwrap())
        .map(|plugin| plugin.unwrap().path().join("agents/reviewer.md"))
        .find(|definition| definition.is_file())
        .expect("the Store holds the definition");
    let bare = agents.join("reviewer.md");
    uze_core::persistence::create_symlink(&store_definition, &bare).unwrap();
    let mut legacy = receipt.clone();
    legacy.artifact = ManagedArtifact::SymlinkReference {
        path: bare.clone(),
        target: store_definition,
    };
    state::record_receipt(&uze, legacy).unwrap();

    let second = install_via_marketplace_json(&home, &uze_home, &package, &path);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(
        bare.symlink_metadata().is_err(),
        "the old bare name no longer answers"
    );
    assert!(labelled.is_file() && !labelled.is_symlink());
    let agent_receipts: Vec<_> = state::receipts(&uze, None)
        .unwrap()
        .into_iter()
        .filter(|receipt| {
            receipt
                .resource_identity
                .as_deref()
                .is_some_and(|identity| identity.contains("agents/reviewer.md"))
        })
        .collect();
    assert_eq!(agent_receipts.len(), 1, "{agent_receipts:?}");
    let _ = fs::remove_dir_all(home);
}

/// A skill and an agent of one name are two different things in two
/// different places — `~/.config/opencode/skills/crew:review/` and
/// `~/.config/opencode/agents/crew:review.md` — so neither is refused as
/// holding the other's name.
#[test]
fn a_skill_and_an_agent_of_one_name_are_both_delivered() {
    use std::fs;

    let home = temporary_home("cli-skill-agent-same-name");
    let package = home.join("crew");
    fs::create_dir_all(package.join("agents")).unwrap();
    fs::create_dir_all(package.join("skills/review")).unwrap();
    fs::write(package.join("plugin.json"), r#"{"name": "crew"}"#).unwrap();
    fs::write(
        package.join("skills/review/SKILL.md"),
        "---\nname: review\ndescription: Reviews.\n---\nSkill.\n",
    )
    .unwrap();
    fs::write(
        package.join("agents/review.md"),
        "---\nname: review\ndescription: Reviews.\n---\nAgent.\n",
    )
    .unwrap();
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    opencode_answering(&bin);
    let path = uze_testkit::process::path_with(&[&bin]);

    let add = install_via_marketplace_json(&home, &home.join(".uze"), &package, &path);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let report = String::from_utf8_lossy(&add.stdout);
    assert!(!report.contains("not delivered"), "{report}");
    assert!(
        home.join(".config/opencode/skills")
            .join(uze_core::path::file_name_for("crew:review"))
            .join("SKILL.md")
            .is_file()
    );
    assert!(
        home.join(".config/opencode/agents")
            .join(format!(
                "{}.md",
                uze_core::path::file_name_for("crew:review")
            ))
            .is_file()
    );
    let _ = fs::remove_dir_all(home);
}

/// A project's `uze install` that places a plugin one harness refuses says
/// so and exits non-zero, as `install -m` does: the environment it reports
/// is not the one every harness received.
#[test]
fn a_project_install_one_harness_refuses_fails_and_names_it() {
    let home = temporary_home("cli-project-install-partial");
    let fake_bin = refusing_harness_bin_dir("cli-project-install-partial-bin", true);
    let path = uze_testkit::process::path_with(&[&fake_bin]);
    let package = package_fixture();
    let name = uze_testkit::marketplace::package_manifest_name(&package);
    let market = home.join("market");
    uze_testkit::marketplace::stage(
        &market,
        &serde_json::json!({
            "name": "test",
            "plugins": [{ "name": name, "source": format!("./plugins/{name}") }],
        })
        .to_string(),
        &[(name.clone(), package)],
    );
    let project = home.join("project");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::write(
        project.join("agents.yaml"),
        format!(
            "marketplaces:\n  test:\n    path: {}\n    plugins:\n      - {name}\n",
            market.display()
        ),
    )
    .unwrap();

    let install = Command::new(env!("CARGO_BIN_EXE_uze"))
        .current_dir(&project)
        .env("UZE_HOME", home.join(".uze"))
        .isolated_home(&home)
        .env("PATH", &path)
        .args(["install"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        !install.status.success(),
        "a partial install reported success"
    );
    assert!(
        stderr.contains("not delivered everywhere") && stderr.contains("read-only"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// A package with a skill and an agent whose frontmatter only Claude Code
/// reads whole, so every route a harness can take shows up somewhere.
fn composed_package(home: &std::path::Path) -> PathBuf {
    use std::fs;

    let package = home.join("crew");
    fs::create_dir_all(package.join("agents")).unwrap();
    fs::create_dir_all(package.join("skills/review")).unwrap();
    fs::write(package.join("plugin.json"), r#"{"name": "crew"}"#).unwrap();
    fs::write(
        package.join("skills/review/SKILL.md"),
        "---\nname: review\ndescription: Reviews a change.\n---\nReview it.\n",
    )
    .unwrap();
    fs::write(
        package.join("agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews a change.\nmodel: haiku\ntools: Read, Grep\n---\nReview.\n",
    )
    .unwrap();
    package
}

fn uze_at(home: &std::path::Path, path: &std::ffi::OsStr, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_uze"))
        .env("UZE_HOME", home.join(".uze"))
        .isolated_home(home)
        .env("PATH", path)
        .env_remove("CLAUDE_CONFIG_DIR")
        .args(args)
        .output()
        .unwrap()
}

fn uze_json_at(home: &std::path::Path, path: &std::ffi::OsStr, args: &[&str]) -> serde_json::Value {
    let mut with_json = args.to_vec();
    with_json.extend(["--format", "json"]);
    let output = uze_at(home, path, &with_json);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?} must answer in JSON ({error}): {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// `uze inspect --harness` is computed without attaching anything, and is
/// only worth running before a release if it says what the install says:
/// the same route and the same capabilities short of native, harness by
/// harness.
#[test]
fn inspect_and_install_report_agree_on_every_harness() {
    let home = temporary_home("cli-inspect-agrees");
    let fake_bin = fake_harness_bin_dir("cli-inspect-agrees-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);
    let package = composed_package(&home);

    let add = install_via_marketplace_json(&home, &home.join(".uze"), &package, &path);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let installed: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let delivered = installed["deliveries"].as_array().expect("deliveries");
    assert_eq!(delivered.len(), 4, "every harness is detected: {installed}");
    let attachments = std::fs::read(home.join(".uze/state/attachments.json")).ok();

    let inspected = uze_json_at(&home, &path, &["inspect", "crew"]);
    for install in delivered {
        let integration = install["integration"].as_str().unwrap();
        let view = inspected["deliveries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|view| view["integration"] == integration)
            .unwrap_or_else(|| panic!("inspect names {integration}: {inspected}"));
        assert_eq!(view["detected"], true, "{view}");
        assert_eq!(view["route"], install["route"], "{integration} route");
        assert_eq!(
            view["shortfalls"], install["shortfalls"],
            "{integration} shortfalls"
        );

        let one = uze_json_at(&home, &path, &["inspect", "crew", "--harness", integration]);
        let only = one["deliveries"].as_array().unwrap();
        assert_eq!(only.len(), 1, "{one}");
        assert_eq!(only[0], *view, "narrowed to {integration}");
        let text = uze_at(&home, &path, &["inspect", "crew", "--harness", integration]);
        assert!(text.status.success());
        let text = String::from_utf8_lossy(&text.stdout);
        assert!(
            text.contains("crew:review") && text.contains("crew:reviewer"),
            "{integration} names each capability as a session sees it: {text}"
        );
    }
    let opencode = inspected["deliveries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|view| view["integration"] == "opencode")
        .unwrap();
    assert!(
        opencode["shortfalls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|one| one["capability"] == "crew:reviewer" && one["route"] == "DEGRADED"),
        "the fields OpenCode cannot read are said: {opencode}"
    );
    assert_eq!(
        std::fs::read(home.join(".uze/state/attachments.json")).ok(),
        attachments,
        "inspect records nothing"
    );
    let unknown = uze_at(&home, &path, &["inspect", "crew", "--harness", "nowhere"]);
    assert!(!unknown.status.success(), "an unknown harness is refused");

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// A path Claude Code would load as a component no canonical capability
/// defines is left out of its delivery, and `inspect` says so for exactly
/// the paths the package holds, on that harness alone.
#[test]
fn inspect_names_the_paths_a_harness_does_not_receive() {
    let home = temporary_home("cli-inspect-withheld");
    let fake_bin = fake_harness_bin_dir("cli-inspect-withheld-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);
    let package = composed_package(&home);
    std::fs::create_dir_all(package.join("commands")).unwrap();
    std::fs::write(package.join("commands/hello.md"), "Say hello.\n").unwrap();
    std::fs::create_dir_all(package.join("bin")).unwrap();
    std::fs::write(package.join("bin/tool"), "#!/bin/sh\n").unwrap();

    let add = install_via_marketplace_json(&home, &home.join(".uze"), &package, &path);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let inspected = uze_json_at(&home, &path, &["inspect", "crew"]);
    for view in inspected["deliveries"].as_array().unwrap() {
        let expected = if view["integration"] == "claude-code" {
            serde_json::json!(["commands", "bin"])
        } else {
            serde_json::Value::Null
        };
        assert_eq!(view["withheld"], expected, "{view}");
    }
    let text = uze_at(
        &home,
        &path,
        &["inspect", "crew", "--harness", "claude-code"],
    );
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(
        text.contains("commands not delivered") && text.contains("bin not delivered"),
        "{text}"
    );
    assert!(!text.contains("monitors"), "{text}");

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// Receipts that match are not a delivery a harness reads: Claude Code's
/// cached copy of the plugin can be empty, and an agent file an earlier
/// build wrote can carry fields OpenCode drops the agent over. Doctor
/// compares what the plan expects with what each harness would load.
#[test]
fn doctor_reports_an_empty_plugin_cache_and_an_unreadable_agent() {
    use std::fs;
    use uze_core::{UzeHome, exposure::ManagedArtifact, state};

    let home = temporary_home("cli-doctor-intent");
    let fake_bin = fake_harness_bin_dir("cli-doctor-intent-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);
    let package = composed_package(&home);
    let add = install_via_marketplace_json(&home, &home.join(".uze"), &package, &path);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let uze = UzeHome::at(home.join(".uze"));
    let receipts: Vec<_> = state::receipts(&uze, None)
        .unwrap()
        .into_iter()
        .filter(|receipt| receipt.package_id.starts_with("crew@"))
        .collect();

    // Claude's records say the plugin is installed and enabled, and the copy
    // they point at holds nothing.
    let claude = receipts
        .iter()
        .find(|receipt| receipt.integration == "claude-code" && receipt.resource_identity.is_none())
        .expect("the Claude plugin receipt");
    let ManagedArtifact::IntegrationOwned {
        selector, detail, ..
    } = &claude.artifact
    else {
        panic!("{claude:?}");
    };
    let marketplace_root = detail["marketplace_root"].as_str().unwrap();
    let marketplace = selector.rsplit_once('@').unwrap().1;
    let cache = home.join(".claude/plugins/cache/crew");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        home.join(".claude/plugins/known_marketplaces.json"),
        serde_json::json!({ marketplace: {
            "source": { "source": "directory", "path": marketplace_root },
            "installLocation": marketplace_root
        }})
        .to_string(),
    )
    .unwrap();
    fs::write(
        home.join(".claude/plugins/installed_plugins.json"),
        serde_json::json!({ "version": 2, "plugins": {
            selector.as_str(): [{ "scope": "user", "installPath": cache }]
        }})
        .to_string(),
    )
    .unwrap();
    fs::write(
        home.join(".claude/settings.json"),
        serde_json::json!({ "enabledPlugins": { selector.as_str(): true } }).to_string(),
    )
    .unwrap();

    // The agent file as 1.0.0-beta.3 wrote it, and its receipt with it.
    let opencode = receipts
        .iter()
        .find(|receipt| {
            receipt.integration == "opencode"
                && matches!(receipt.artifact, ManagedArtifact::GeneratedFile { .. })
        })
        .expect("the OpenCode agent receipt");
    let ManagedArtifact::GeneratedFile { path: agent, .. } = &opencode.artifact else {
        unreachable!();
    };
    let written =
        "---\ndescription: Reviews a change.\nmodel: haiku\ntools: Read, Grep\n---\nReview.\n";
    fs::write(agent, written).unwrap();
    state::forget_receipt(&uze, opencode).unwrap();
    let mut earlier = opencode.clone();
    earlier.artifact = ManagedArtifact::GeneratedFile {
        path: agent.clone(),
        content: written.to_owned(),
    };
    state::record_receipt(&uze, earlier).unwrap();

    let doctor = uze_json_at(&home, &path, &["doctor"]);
    let harness = |id: &str| {
        doctor["deliveries"]
            .as_array()
            .expect("deliveries")
            .iter()
            .find(|package| package["plugin"].as_str().unwrap().starts_with("crew@"))
            .and_then(|package| {
                package["harnesses"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|harness| harness["integration"] == id)
                    .cloned()
            })
            .unwrap_or_else(|| panic!("doctor checks crew on {id}: {doctor}"))
    };
    let unreadable = |harness: &serde_json::Value, capability: &str, says: &str| {
        harness["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| {
                finding["kind"] == "unreadable"
                    && finding["capability"] == capability
                    && finding["detail"].as_str().unwrap().contains(says)
            })
    };
    let claude = harness("claude-code");
    assert!(
        unreadable(&claude, "crew:review", "cached copy")
            && unreadable(&claude, "crew:reviewer", "cached copy"),
        "{claude}"
    );
    assert_eq!(claude["present"], 0, "{claude}");
    let opencode = harness("opencode");
    assert!(
        unreadable(&opencode, "crew:reviewer", "`model`")
            && unreadable(&opencode, "crew:reviewer", "`tools`")
            && unreadable(&opencode, "crew:reviewer", "`mode`"),
        "{opencode}"
    );
    assert!(
        !opencode["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["capability"] == "crew:review"),
        "the skill beside it is fine: {opencode}"
    );

    let text = uze_at(&home, &path, &["doctor"]);
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(
        text.contains("crew:reviewer") && text.contains("unreadable"),
        "{text}"
    );

    let _ = fs::remove_dir_all(home);
    let _ = fs::remove_dir_all(fake_bin);
}

/// A PATH whose only harness is a stand-in OpenCode, written under `home`.
fn opencode_only_path(home: &std::path::Path) -> std::ffi::OsString {
    use std::fs;

    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    opencode_answering(&bin);
    uze_testkit::process::path_with(&[&bin])
}

/// `crew`, carrying one agent, `reviewer`.
fn crew_with_an_agent(home: &std::path::Path) -> PathBuf {
    let package = home.join("crew");
    std::fs::create_dir_all(package.join("agents")).unwrap();
    std::fs::write(package.join("plugin.json"), r#"{"name": "crew"}"#).unwrap();
    std::fs::write(
        package.join("agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews a change.\n---\nReview.\n",
    )
    .unwrap();
    package
}

/// The one receipt recording a generated file: the agent's.
fn agent_receipt(uze: &uze_core::UzeHome) -> uze_core::integration::AttachmentReceipt {
    uze_core::state::receipts(uze, None)
        .unwrap()
        .into_iter()
        .find(|receipt| {
            matches!(
                receipt.artifact,
                uze_core::exposure::ManagedArtifact::GeneratedFile { .. }
            )
        })
        .expect("the agent's receipt")
}

/// An agent file an earlier build wrote under the bare name, which the
/// operator edited since, is theirs: the next install leaves the edit where
/// it is and holds the agent back, rather than offering it a second time
/// under its label beside the edit.
#[test]
fn an_earlier_agent_file_the_operator_edited_is_held_back_not_replaced() {
    use std::fs;
    use uze_core::{UzeHome, exposure::ManagedArtifact, state};

    let home = temporary_home("cli-agent-edited-earlier-shape");
    let package = crew_with_an_agent(&home);
    let path = opencode_only_path(&home);
    let uze_home = home.join(".uze");
    let first = install_via_marketplace_json(&home, &uze_home, &package, &path);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let agents = home.join(".config/opencode/agents");
    let labelled = agents.join(format!(
        "{}.md",
        uze_core::path::file_name_for("crew:reviewer")
    ));
    assert!(labelled.is_file());

    // What an earlier build wrote, under the name it gave the agent.
    let uze = UzeHome::at(&uze_home);
    let receipt = agent_receipt(&uze);
    state::forget_receipt(&uze, &receipt).unwrap();
    fs::remove_file(&labelled).unwrap();
    let bare = agents.join("reviewer.md");
    let written = "---\ndescription: Reviews a change.\nmode: subagent\n---\nReview.\n";
    fs::write(&bare, written).unwrap();
    let mut earlier = receipt.clone();
    earlier.artifact = ManagedArtifact::GeneratedFile {
        path: bare.clone(),
        content: written.to_owned(),
    };
    state::record_receipt(&uze, earlier).unwrap();
    // And the operator's edit since.
    fs::write(&bare, "edited by hand\n").unwrap();

    let second = install_via_marketplace_json(&home, &uze_home, &package, &path);
    let report: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap_or_else(|_| {
        panic!(
            "the install answers in JSON: {}{}",
            String::from_utf8_lossy(&second.stdout),
            String::from_utf8_lossy(&second.stderr)
        )
    });

    assert_eq!(fs::read_to_string(&bare).unwrap(), "edited by hand\n");
    assert!(
        !labelled.exists(),
        "the agent is not offered a second time beside the edit"
    );
    assert!(
        report["blocked"].as_array().unwrap().iter().any(|held| {
            held["integration"] == "opencode"
                && held["reason"]
                    .as_str()
                    .unwrap()
                    .contains("changed since it was made")
        }),
        "the agent is reported held, with why: {report}"
    );
    let _ = fs::remove_dir_all(home);
}

/// What doctor finds of `crew` on `integration`. Doctor repairs what it
/// can before it reports, so the delivery is read while another mutation
/// holds the lock, which is when what doctor finds is what is on disk.
fn crew_findings(
    home: &std::path::Path,
    path: &std::ffi::OsStr,
    integration: &str,
) -> Vec<serde_json::Value> {
    let _held =
        uze_core::persistence::MutationLock::acquire(&uze_core::UzeHome::at(home.join(".uze")))
            .unwrap();
    let doctor = uze_json_at(home, path, &["doctor"]);
    doctor["deliveries"]
        .as_array()
        .expect("deliveries")
        .iter()
        .find(|package| package["plugin"].as_str().unwrap().starts_with("crew@"))
        .and_then(|package| {
            package["harnesses"]
                .as_array()
                .unwrap()
                .iter()
                .find(|harness| harness["integration"] == integration)
                .cloned()
        })
        .unwrap_or_else(|| panic!("doctor checks crew on {integration}: {doctor}"))["findings"]
        .as_array()
        .unwrap()
        .clone()
}

/// An agent whose file is gone is reported missing, by the name a session
/// would have called it.
#[test]
fn doctor_reports_an_agent_whose_file_is_gone_as_missing() {
    let home = temporary_home("cli-doctor-missing");
    let path = opencode_only_path(&home);
    let add =
        install_via_marketplace_json(&home, &home.join(".uze"), &crew_with_an_agent(&home), &path);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    std::fs::remove_file(home.join(".config/opencode/agents").join(format!(
        "{}.md",
        uze_core::path::file_name_for("crew:reviewer")
    )))
    .unwrap();

    let findings = crew_findings(&home, &path, "opencode");
    assert!(
        findings
            .iter()
            .any(|finding| finding["kind"] == "missing" && finding["capability"] == "crew:reviewer"),
        "{findings:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// A hook entry delivered intact but under another name than its plan
/// gives it is reported renamed, saying the name it answers to instead.
/// Antigravity CLI's named entries, which only the POSIX wrapper makes.
#[cfg(unix)]
#[test]
fn doctor_reports_a_hook_delivered_under_another_name_as_renamed() {
    use uze_core::{UzeHome, exposure::ManagedArtifact, state};

    let home = temporary_home("cli-doctor-renamed");
    let fake_bin = fake_harness_bin_dir("cli-doctor-renamed-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);
    let add = install_via_marketplace_json(
        &home,
        &home.join(".uze"),
        &crew_with_a_server_and_hooks(&home),
        &path,
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let uze = UzeHome::at(home.join(".uze"));
    let receipt = state::receipts(&uze, None)
        .unwrap()
        .into_iter()
        .find(|receipt| {
            receipt.integration == "antigravity"
                && matches!(receipt.artifact, ManagedArtifact::HookConfigEntry { .. })
        })
        .expect("Antigravity CLI holds the hook as a named entry");
    let ManagedArtifact::HookConfigEntry {
        config_file,
        entry_name,
        ..
    } = &receipt.artifact
    else {
        unreachable!();
    };
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config_file).unwrap()).unwrap();
    let entry = config
        .as_object_mut()
        .unwrap()
        .remove(entry_name)
        .expect("the entry is in the config");
    config["watch"] = entry;
    std::fs::write(config_file, config.to_string()).unwrap();
    state::forget_receipt(&uze, &receipt).unwrap();
    let mut moved = receipt.clone();
    if let ManagedArtifact::HookConfigEntry { entry_name, .. } = &mut moved.artifact {
        *entry_name = "watch".to_owned();
    }
    state::record_receipt(&uze, moved).unwrap();

    let findings = crew_findings(&home, &path, "antigravity");
    assert!(
        findings.iter().any(|finding| finding["kind"] == "renamed"
            && finding["capability"] == entry_name.as_str()
            && finding["detail"] == "delivered as `watch`"),
        "{findings:?}"
    );
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// `crew`, carrying an MCP server and hooks for two events: `ensure-ui`
/// on session start and `watch` before a shell command, each spelled for
/// both shells.
fn crew_with_a_server_and_hooks(home: &std::path::Path) -> PathBuf {
    let package = home.join("crew");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("plugin.json"), r#"{"name": "crew"}"#).unwrap();
    std::fs::write(
        package.join("mcp.json"),
        r#"{"mcpServers":{"docs":{"type":"stdio","command":"/bin/true","args":[]}}}"#,
    )
    .unwrap();
    std::fs::write(
        package.join("hooks.json"),
        r#"{"hooks":{"SessionStart":[{"id":"ensure-ui","hooks":[{"type":"command","command":{"posix":"${PLUGIN_ROOT}/ensure-ui","windows":"& '${PLUGIN_ROOT}/ensure-ui.ps1'"}}]}],"PreToolUse":[{"id":"watch","matcher":"shell","hooks":[{"type":"command","command":{"posix":"${PLUGIN_ROOT}/watch","windows":"& '${PLUGIN_ROOT}/watch.ps1'"}}]}]}}"#,
    )
    .unwrap();
    package
}

/// The effective view of a package with a server and hooks names both for
/// the harness asked about.
#[test]
fn inspect_names_the_servers_and_hooks_a_harness_receives() {
    let home = temporary_home("cli-inspect-mcp-hooks");
    let fake_bin = fake_harness_bin_dir("cli-inspect-mcp-hooks-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);
    let add = install_via_marketplace_json(
        &home,
        &home.join(".uze"),
        &crew_with_a_server_and_hooks(&home),
        &path,
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let inspected = uze_json_at(
        &home,
        &path,
        &["inspect", "crew", "--harness", "claude-code"],
    );

    let [claude] = inspected["deliveries"].as_array().unwrap().as_slice() else {
        panic!("narrowed to Claude Code: {inspected}");
    };
    let kinds: Vec<&str> = claude["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|capability| capability["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds.iter().filter(|kind| **kind == "mcp").count(),
        1,
        "{claude}"
    );
    assert_eq!(
        kinds.iter().filter(|kind| **kind == "hook").count(),
        2,
        "{claude}"
    );
    assert!(
        claude["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|capability| capability["exposed_name"].is_string()
                && capability["route"].is_string()),
        "each is named with its route: {claude}"
    );
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}

/// Antigravity CLI runs a session start through its undocumented flat
/// `SessionStart` key: the install delivers that group beside the package's
/// other hook — through the POSIX wrapper, the only one it takes — and
/// reports no shortfall for it.
#[cfg(unix)]
#[test]
fn antigravity_takes_a_session_start_hook_beside_the_rest() {
    let home = temporary_home("cli-antigravity-session-start");
    let fake_bin = fake_harness_bin_dir("cli-antigravity-session-start-bin");
    let path = uze_testkit::process::path_with(&[&fake_bin]);
    let add = install_via_marketplace_json(
        &home,
        &home.join(".uze"),
        &crew_with_a_server_and_hooks(&home),
        &path,
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    let antigravity = report["deliveries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|delivery| delivery["integration"] == "antigravity")
        .unwrap_or_else(|| panic!("Antigravity CLI is delivered to: {report}"));

    assert!(
        !antigravity["shortfalls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|shortfall| shortfall["capability"]
                .as_str()
                .unwrap()
                .contains("ensure-ui")),
        "{antigravity}"
    );
    assert!(
        antigravity["attachments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|location| location.as_str().unwrap().ends_with(":ensure-ui")),
        "the session start reaches Antigravity CLI: {antigravity}"
    );
    assert!(
        antigravity["attachments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|location| location.as_str().unwrap().ends_with(":watch")),
        "the other hook reaches Antigravity CLI: {antigravity}"
    );
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(fake_bin);
}
