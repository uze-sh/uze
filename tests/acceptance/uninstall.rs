//! Acceptance: `uze uninstall` takes UZE off the machine, in either mode,
//! one test per scenario of `specs/machine-uninstall/spec.md` (change
//! `vendor-stores-and-uninstall`). The real binary against the fake
//! harnesses; what is asserted is what the machine holds afterwards.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use uze_testkit::{
    fake_harness::{Action, FakeHarness, MarketplaceVendor},
    fixtures,
    temp::TestEnvironment,
};

use crate::store_ownership::{is_on_demand_scratch, uze_home_references};
use crate::util::{install_fake_harnesses, uze_bin};

/// A machine UZE set up for every fake harness, with a plugin delivered
/// whose skill, MCP server and hook each name its package root.
fn delivered() -> TestEnvironment {
    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);
    env.run(uze_bin(), &["setup"]);
    let (market, add) = uze_testkit::marketplace::marketplace_add_args(
        env.root(),
        &fixtures::canonical("store-tie"),
    );
    env.run_ok(
        uze_bin(),
        &market.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &add.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env
}

/// `uze uninstall <args>`, with nobody at a terminal.
fn uninstall(env: &TestEnvironment, args: &[&str]) -> Output {
    uninstall_with(env, uze_bin(), args)
}

fn uninstall_with(env: &TestEnvironment, binary: &Path, args: &[&str]) -> Output {
    env.command(binary)
        .arg("uninstall")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("uze runs")
}

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Every file under `root`, with its bytes: what "nothing changed" is
/// compared by.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(path);
            } else {
                // The fake CLIs' invocation logs grow with every question
                // asked of them, which is not the machine changing.
                if path.starts_with(root.join("bin")) {
                    continue;
                }
                files.push((path.clone(), fs::read(&path).unwrap_or_default()));
            }
        }
    }
    files.sort();
    files
}

/// Every call a fake vendor CLI answered, one argument list each.
fn invocations(env: &TestEnvironment, name: &str) -> Vec<Vec<String>> {
    fs::read_to_string(
        env.fake_bin
            .join(".invocations")
            .join(format!("{name}.log")),
    )
    .unwrap_or_default()
    .lines()
    .map(|line| line.split_whitespace().map(str::to_owned).collect())
    .collect()
}

/// The paths whose bytes differ between two snapshots, or that only one
/// holds: empty when nothing changed.
fn changed(before: &[(PathBuf, Vec<u8>)], after: &[(PathBuf, Vec<u8>)]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = before
        .iter()
        .filter(|entry| !after.contains(entry))
        .chain(after.iter().filter(|entry| !before.contains(entry)))
        .map(|(path, _)| path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// The top-level names left in `$UZE_HOME`.
fn home_entries(env: &TestEnvironment) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(&env.uze_home)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// What a fake vendor CLI holds as installed, and the marketplace it holds.
fn vendor_state(env: &TestEnvironment, vendor: &str) -> (String, Option<String>) {
    let state = env.root().join("fake-state").join(vendor);
    (
        fs::read_to_string(state.join("installed")).unwrap_or_default(),
        fs::read_to_string(state.join("name")).ok(),
    )
}

/// The marketplaces a fake vendor CLI has registered, as `(name, root)`.
fn registered(env: &TestEnvironment, vendor: &str) -> Vec<(String, String)> {
    crate::vendor_store_moves::registered(env, vendor)
}

/// A copy of the binary at `directory`, recorded by the installer as the
/// file it placed (`state/install.json`), so uninstall may remove it.
fn placed_by_the_installer(env: &TestEnvironment, directory: &Path) -> PathBuf {
    fs::create_dir_all(directory).unwrap();
    let binary = directory.join(uze_platform::executable::file_name("uze"));
    uze_platform::executable::install(&binary, &fs::read(uze_bin()).unwrap()).unwrap();
    let state = env.uze_home.join("state");
    fs::create_dir_all(&state).unwrap();
    fs::write(
        state.join("install.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "binary": fs::canonicalize(&binary).unwrap(),
            "version": env!("CARGO_PKG_VERSION"),
        }))
        .unwrap(),
    )
    .unwrap();
    binary
}

// --- Uninstall shows its plan before acting -------------------------------

// Linux only: `script` is what gives uninstall a terminal to ask on here.
#[cfg(target_os = "linux")]
#[test]
fn the_plan_is_declined() {
    use std::io::Write as _;

    let env = delivered();
    let before = snapshot(env.root());
    let Ok(mut running) = env
        .command("script")
        .args([
            "-qec",
            &format!("{} uninstall", uze_bin().display()),
            "/dev/null",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        eprintln!("skipped: no `script` here to give uninstall a terminal");
        return;
    };
    // `enter` keeps the first mode offered, and `n` declines the plan.
    let _ = running.stdin.take().unwrap().write_all(b"\rn\r");
    let output = running.wait_with_output().unwrap();

    assert!(
        said(&output).contains("nothing changed"),
        "{}",
        said(&output)
    );
    assert_eq!(
        changed(&before, &snapshot(env.root())),
        Vec::<PathBuf>::new(),
        "{}",
        said(&output)
    );
}

#[test]
fn non_interactive_use_prints_the_plan_and_names_yes() {
    let env = delivered();
    let before = snapshot(env.root());

    let output = uninstall(&env, &[]);

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Remove") && stdout.contains("Keep"),
        "{stdout}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("uze uninstall --yes"),
        "{}",
        said(&output)
    );
    assert_eq!(
        changed(&before, &snapshot(env.root())),
        Vec::<PathBuf>::new()
    );
}

// --- Uninstall delivers nothing -------------------------------------------

#[test]
fn a_second_purge_after_one_that_detached_the_bundled_plugin_delivers_nothing() {
    let env = delivered();
    let purged = uninstall(&env, &["--yes", "--purge"]);
    assert!(purged.status.success(), "{}", said(&purged));
    let asked_before = invocations(&env, "claude").len();

    let again = uninstall(&env, &["--yes", "--purge"]);

    assert!(again.status.success(), "{}", said(&again));
    assert!(!env.uze_home.exists(), "a second run recreated the home");
    let installs: Vec<_> = invocations(&env, "claude")[asked_before..]
        .iter()
        .filter(|argv| argv.iter().any(|word| word == "install" || word == "add"))
        .cloned()
        .collect();
    assert!(installs.is_empty(), "delivered again: {installs:?}");
    assert!(vendor_state(&env, "claude").0.trim().is_empty());
}

// --- By default, only the program is removed ------------------------------

#[test]
fn default_uninstall_keeps_the_packages_the_stores_the_records_and_the_settings() {
    let env = delivered();
    fs::write(env.uze_home.join("config.toml"), "").unwrap();
    let delivered_before = ["claude", "codex", "agy"].map(|vendor| vendor_state(&env, vendor));

    let output = uninstall(&env, &["--yes"]);

    assert!(output.status.success(), "{}", said(&output));
    let kept = home_entries(&env);
    for name in &kept {
        assert!(
            [
                "sources",
                "store",
                "state",
                "config.toml",
                "keys.json",
                "theme-overrides.json",
                "themes"
            ]
            .contains(&name.as_str()),
            "{name} survived the default uninstall: {kept:?}"
        );
    }
    assert!(kept.contains(&"state".to_owned()) && kept.contains(&"config.toml".to_owned()));
    assert_eq!(
        ["claude", "codex", "agy"].map(|vendor| vendor_state(&env, vendor)),
        delivered_before,
        "a harness's plugins were touched"
    );
    assert!(
        said(&output).contains("uze uninstall --purge"),
        "the report does not say how to purge later: {}",
        said(&output)
    );
}

/// Every path a harness was pointed at still exists once the program is
/// gone: what "every harness still loads every plugin" rests on.
#[test]
fn default_uninstall_leaves_every_harness_pointed_at_something_that_exists() {
    let env = delivered();

    let output = uninstall(&env, &["--yes"]);

    assert!(output.status.success(), "{}", said(&output));
    let uze_home = env.uze_home.to_string_lossy().into_owned();
    let dangling: Vec<_> = uze_home_references(&env)
        .into_iter()
        .filter(|(_, path)| !is_on_demand_scratch(path, &uze_home))
        .filter(|(_, path)| !Path::new(path).exists())
        .collect();
    assert!(dangling.is_empty(), "pointed at what is gone: {dangling:?}");
}

#[test]
fn reinstalled_after_a_default_uninstall_it_resumes_with_no_new_delivery() {
    let env = delivered();
    let output = uninstall(&env, &["--yes"]);
    assert!(output.status.success(), "{}", said(&output));
    let asked_before = invocations(&env, "claude").len();

    let status = env.run_ok(uze_bin(), &["status", "-m"]);

    assert!(
        String::from_utf8_lossy(&status.stdout).contains("store-tie"),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
    let delivered_again: Vec<_> = invocations(&env, "claude")[asked_before..]
        .iter()
        .filter(|argv| {
            argv.starts_with(&["plugin".to_owned(), "install".to_owned()])
                || argv.starts_with(&[
                    "plugin".to_owned(),
                    "marketplace".to_owned(),
                    "add".to_owned(),
                ])
        })
        .cloned()
        .collect();
    assert!(delivered_again.is_empty(), "{delivered_again:?}");
}

// --- Purge removes every trace by receipt ---------------------------------

#[test]
fn a_purge_where_everything_matches_leaves_no_trace() {
    let env = delivered();

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    assert!(!env.uze_home.exists(), "{}", said(&output));
    for vendor in ["claude", "codex", "agy"] {
        let (installed, marketplace) = vendor_state(&env, vendor);
        assert!(
            installed.trim().is_empty(),
            "{vendor} still lists {installed}"
        );
        assert!(
            marketplace.is_none(),
            "{vendor} still lists {marketplace:?}"
        );
    }
}

/// No configuration or discovery file names `$UZE_HOME` once it is purged.
#[test]
fn a_purge_leaves_no_harness_file_naming_the_home() {
    let env = delivered();

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    let references = uze_home_references(&env);
    assert!(references.is_empty(), "{references:?}");
}

/// A harness prepared by an install alone, never by `setup`, still has
/// what preparing wrote (OpenCode's `uze.ts`) owned by a receipt, so a purge
/// takes it back like everything else.
#[test]
fn a_purge_after_an_install_with_no_setup_leaves_no_harness_file_naming_the_home() {
    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);
    let (market, add) = uze_testkit::marketplace::marketplace_add_args(
        env.root(),
        &fixtures::canonical("store-tie"),
    );
    env.run_ok(
        uze_bin(),
        &market.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &add.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    let references = uze_home_references(&env);
    assert!(references.is_empty(), "{references:?}");
}

#[test]
fn the_operators_codex_hooks_keep_their_place() {
    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);
    let hooks = env.home.join(".codex/hooks.json");
    fs::create_dir_all(hooks.parent().unwrap()).unwrap();
    let operator = serde_json::json!({
        "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "operator-guard"}]}]}
    });
    fs::write(&hooks, serde_json::to_vec_pretty(&operator).unwrap()).unwrap();
    env.run(uze_bin(), &["setup"]);
    let (market, add) = uze_testkit::marketplace::marketplace_add_args(
        env.root(),
        &fixtures::canonical("store-tie"),
    );
    env.run_ok(
        uze_bin(),
        &market.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &add.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let placed: serde_json::Value = serde_json::from_slice(&fs::read(&hooks).unwrap()).unwrap();
    assert!(
        placed["hooks"]["PreToolUse"]
            .as_array()
            .is_some_and(|groups| groups.len() > 1),
        "UZE added no hook group beside the operator's: {placed}"
    );
    assert_eq!(
        placed["hooks"]["PreToolUse"][0], operator["hooks"]["PreToolUse"][0],
        "the operator's group moved when UZE's arrived"
    );

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    let left: serde_json::Value = serde_json::from_slice(&fs::read(&hooks).unwrap()).unwrap();
    assert_eq!(left["hooks"]["PreToolUse"], operator["hooks"]["PreToolUse"]);
}

/// A skill directory UZE copied into a harness's own discovery root, as
/// its receipt names it: the delivery a person edits by hand. Read from
/// the ledger, never found by walking the home: a vendor CLI keeps copies
/// of its own there too (Antigravity's plugin cache), which no receipt
/// owns, and which one a walk meets first depends on the file system's
/// listing order.
fn a_delivered_skill_copy(env: &TestEnvironment) -> Option<PathBuf> {
    fn paths(value: &serde_json::Value, found: &mut Vec<PathBuf>) {
        match value {
            serde_json::Value::Object(entries) => {
                for (key, value) in entries {
                    match value {
                        serde_json::Value::String(path) if key == "path" => {
                            found.push(PathBuf::from(path));
                        }
                        other => paths(other, found),
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    paths(value, found);
                }
            }
            _ => {}
        }
    }
    let ledger: serde_json::Value =
        serde_json::from_slice(&fs::read(env.uze_home.join("state/attachments.json")).ok()?)
            .ok()?;
    let mut found = Vec::new();
    paths(&ledger, &mut found);
    found.into_iter().find(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("locate"))
            && path.join("SKILL.md").is_file()
    })
}

#[test]
fn a_purge_leaves_an_edited_skill_and_lists_it() {
    let env = delivered();
    let edited = a_delivered_skill_copy(&env).expect("a skill was copied into a harness");
    fs::write(edited.join("SKILL.md"), "edited by hand\n").unwrap();

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    assert_eq!(
        fs::read_to_string(edited.join("SKILL.md")).unwrap(),
        "edited by hand\n"
    );
    assert!(
        said(&output).contains(&edited.display().to_string()),
        "{}",
        said(&output)
    );
    assert!(!env.uze_home.exists(), "the rest was not removed");
    assert!(vendor_state(&env, "claude").0.trim().is_empty());
}

/// Every marketplace a fake vendor CLI lists whose root is gone: a
/// registration that outlived what it names.
fn dangling_registrations(env: &TestEnvironment) -> Vec<(String, String, String)> {
    ["claude", "codex", "agy", "opencode"]
        .into_iter()
        .flat_map(|vendor| {
            registered(env, vendor)
                .into_iter()
                .map(move |(name, root)| (vendor.to_owned(), name, root))
        })
        .filter(|(_, _, root)| !Path::new(root).exists())
        .collect()
}

#[test]
fn a_purge_that_leaves_an_edited_skill_leaves_no_registration_behind_its_root() {
    let env = delivered();
    let edited = a_delivered_skill_copy(&env).expect("a skill was copied into a harness");
    fs::write(edited.join("SKILL.md"), "edited by hand\n").unwrap();

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    let dangling = dangling_registrations(&env);
    assert!(dangling.is_empty(), "{dangling:?}: {}", said(&output));
}

/// A build edited by hand is left, and so is the store that holds it,
/// named in the plan and the report; no registration names a root that
/// is gone.
#[test]
fn a_purge_keeps_an_edited_build_and_the_store_that_holds_it() {
    let env = delivered();
    let build = env.uze_home.join("store/claude/store-tie@test");
    let edited = fs::read_dir(build.join("skills"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path().join("SKILL.md"))
        .find(|path| path.is_file())
        .expect("the build carries a skill");
    fs::write(&edited, "edited by hand\n").unwrap();

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    assert_eq!(fs::read_to_string(&edited).unwrap(), "edited by hand\n");
    let store = env.uze_home.join("store/claude");
    assert!(
        said(&output).contains(&store.display().to_string()),
        "{}",
        said(&output)
    );
    let dangling = dangling_registrations(&env);
    assert!(dangling.is_empty(), "{dangling:?}: {}", said(&output));
    assert!(
        !env.uze_home.join("sources").exists() && !env.uze_home.join("state").exists(),
        "the rest of the home was kept: {:?}",
        home_entries(&env)
    );
}

#[test]
fn a_purge_interrupted_on_opencode_leaves_it_whole_or_without_uze_ts() {
    let env = delivered();
    let plugin = env.home.join(".config/opencode/plugins/uze.ts");
    assert!(plugin.is_file(), "OpenCode was never given uze.ts");

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    assert!(!plugin.exists());
}

// Unix: the bare `PATH` below names Unix system directories.
#[cfg(unix)]
#[test]
fn a_purge_with_a_vendor_cli_gone_lists_its_registrations_and_leaves_its_files() {
    let env = delivered();
    let codex = env
        .fake_bin
        .join(uze_platform::executable::file_name("codex"));
    fs::remove_file(&codex).unwrap();
    let codex_home = env.home.join(".codex");
    assert!(codex_home.is_dir(), "Codex was never set up");
    let before = snapshot(&codex_home);

    // Nothing on `PATH` but the stand-ins and the system's own tools, so a
    // Codex installed on the machine running the suite is not found either.
    let output = env
        .command(uze_bin())
        .args(["uninstall", "--yes", "--purge"])
        .env(
            "PATH",
            uze_testkit::temp::join_paths(&[
                &env.fake_bin,
                Path::new("/usr/bin"),
                Path::new("/bin"),
            ]),
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", said(&output));
    assert!(said(&output).contains("Codex"), "{}", said(&output));
    assert!(said(&output).contains("not installed"), "{}", said(&output));
    assert_eq!(
        changed(&before, &snapshot(&codex_home)),
        Vec::<PathBuf>::new(),
        "Codex's files were edited by hand"
    );
    assert!(vendor_state(&env, "claude").0.trim().is_empty());
}

// --- Uninstall can run again until it is done ------------------------------

#[test]
fn an_interrupted_purge_finishes_when_run_again() {
    let env = delivered();
    let binary = placed_by_the_installer(&env, &env.root().join("installed-bin"));
    // Claude refuses to uninstall: the run stops after detaching the rest.
    let claude_state = env.root().join("fake-state").join("claude");
    FakeHarness::new(&env.fake_bin, "claude")
        .version_line("9.9.9 (Fake Claude)")
        .on_prefix(
            ["plugin", "uninstall"],
            Action::Refuse {
                reason: "interrupted".to_owned(),
            },
        )
        .on_prefix(
            ["plugin"],
            Action::VendorMarketplace {
                state_dir: claude_state.clone(),
                vendor: MarketplaceVendor::Claude,
            },
        )
        .build();

    let first = uninstall_with(&env, &binary, &["--yes", "--purge"]);

    assert!(!first.status.success(), "{}", said(&first));
    assert!(binary.exists(), "the binary went before the purge was done");
    assert!(env.uze_home.exists());
    assert!(
        vendor_state(&env, "codex").0.trim().is_empty(),
        "{}",
        said(&first)
    );

    FakeHarness::new(&env.fake_bin, "claude")
        .version_line("9.9.9 (Fake Claude)")
        .on_prefix(
            ["plugin"],
            Action::VendorMarketplace {
                state_dir: claude_state,
                vendor: MarketplaceVendor::Claude,
            },
        )
        .build();
    let second = uninstall_with(&env, &binary, &["--yes", "--purge"]);

    assert!(second.status.success(), "{}", said(&second));
    assert!(!binary.exists());
    assert!(!env.uze_home.exists());
    assert!(vendor_state(&env, "claude").0.trim().is_empty());
}

/// A plugin its harness would not uninstall still reads its marketplace
/// and its build: both stay, the run fails, and the home is kept.
#[test]
fn a_plugin_that_would_not_detach_keeps_its_marketplace_and_its_build() {
    let env = delivered();
    let claude_state = env.root().join("fake-state").join("claude");
    FakeHarness::new(&env.fake_bin, "claude")
        .version_line("9.9.9 (Fake Claude)")
        .on_prefix(
            ["plugin", "uninstall"],
            Action::Refuse {
                reason: "interrupted".to_owned(),
            },
        )
        .on_prefix(
            ["plugin"],
            Action::VendorMarketplace {
                state_dir: claude_state,
                vendor: MarketplaceVendor::Claude,
            },
        )
        .build();
    let build = env.uze_home.join("store/claude/store-tie@test");
    assert!(build.is_dir(), "Claude was never given a build");

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(!output.status.success(), "{}", said(&output));
    assert!(env.uze_home.exists());
    let (installed, _) = vendor_state(&env, "claude");
    assert!(installed.contains("store-tie"), "{installed}");
    assert!(
        !registered(&env, "claude").is_empty(),
        "the marketplace of a plugin still installed was withdrawn: {}",
        said(&output)
    );
    assert!(
        build.is_dir(),
        "the build a plugin still reads was removed: {}",
        said(&output)
    );
}

/// A run that forgot every receipt of a harness and then failed to
/// withdraw its marketplace leaves the next run that marketplace to
/// withdraw, though no receipt names the harness any more.
#[test]
fn a_marketplace_left_by_a_run_that_forgot_its_receipts_is_withdrawn_by_the_next() {
    let env = delivered();
    let claude_state = env.root().join("fake-state").join("claude");
    let claude = |refusing: bool| {
        let mut fake =
            FakeHarness::new(&env.fake_bin, "claude").version_line("9.9.9 (Fake Claude)");
        if refusing {
            fake = fake.on_prefix(
                ["plugin", "marketplace", "remove"],
                Action::Refuse {
                    reason: "interrupted".to_owned(),
                },
            );
        }
        fake.on_prefix(
            ["plugin"],
            Action::VendorMarketplace {
                state_dir: claude_state.clone(),
                vendor: MarketplaceVendor::Claude,
            },
        )
        .build();
    };
    claude(true);
    let first = uninstall(&env, &["--yes", "--purge"]);
    assert!(!first.status.success(), "{}", said(&first));
    assert!(!registered(&env, "claude").is_empty(), "{}", said(&first));

    claude(false);
    let second = uninstall(&env, &["--yes", "--purge"]);

    assert!(second.status.success(), "{}", said(&second));
    assert!(!env.uze_home.exists());
    assert_eq!(
        registered(&env, "claude"),
        Vec::<(String, String)>::new(),
        "{}",
        said(&second)
    );
}

// --- Uninstall refuses while UZE runs --------------------------------------

#[test]
fn a_live_agent_stops_uninstall_and_is_named() {
    let mut engine = crate::engine::Engine::start("  delivery: merge\n");
    engine.launch(uze_testkit::fake_harness::Steps::new());
    let before = home_entries(engine.env());

    let output = uninstall(engine.env(), &["--yes"]);

    assert!(!output.status.success());
    assert!(said(&output).contains("1 agent"), "{}", said(&output));
    assert_eq!(home_entries(engine.env()), before);
}

#[test]
fn inside_a_pane_it_changes_nothing_and_says_where_to_run_it() {
    let env = delivered();
    let before = snapshot(env.root());

    let output = env
        .command(uze_bin())
        .args(["uninstall", "--yes"])
        .env(uze_terminal::launch::PANE_VARIABLE, "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        said(&output).contains("outside the workspace"),
        "{}",
        said(&output)
    );
    assert_eq!(
        changed(&before, &snapshot(env.root())),
        Vec::<PathBuf>::new()
    );
}

// --- Uninstall never touches a project's work ------------------------------

/// A project with one isolated agent whose checkout holds a commit its
/// target lacks, and the context link a channel made beside it.
fn project_with_unlanded_work(env: &TestEnvironment) -> (PathBuf, PathBuf) {
    let _guard = env.apply();
    let git = |cwd: &Path, args: &[&str]| {
        let output = env
            .command("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", said(&output));
    };
    git(&env.project, &["init", "--quiet", "-b", "main", "."]);
    git(&env.project, &["config", "user.name", "Operator"]);
    git(
        &env.project,
        &["config", "user.email", "operator@uze.invalid"],
    );
    fs::write(env.project.join("README.md"), "# app\n").unwrap();
    git(&env.project, &["add", "."]);
    git(&env.project, &["commit", "--quiet", "-m", "init"]);
    let app = uze_application::UzeApplication::new(
        uze_application::UzeHome::at(&env.uze_home),
        Vec::new(),
    );
    let placed = app
        .workspace()
        .place_new_agent(
            &env.project,
            Some(uze_application::PlacementKind::Isolated),
            "claude-code",
            &[],
        )
        .expect("a checkout is placed");
    let slot = placed.cwd.clone();
    fs::write(slot.join("work.txt"), "unlanded\n").unwrap();
    git(&slot, &["add", "."]);
    git(&slot, &["commit", "--quiet", "-m", "feat: unlanded"]);
    let context = env.uze_home.join("runtime/projects/app/agents/one");
    fs::create_dir_all(&context).unwrap();
    let link = env.project.join(".worktrees").join(format!(
        "{}~uze",
        slot.file_name().unwrap().to_string_lossy()
    ));
    uze_platform::fs::symlink(&context, &link).unwrap();
    (slot, link)
}

#[test]
fn a_checkout_with_unlanded_commits_is_left_and_named() {
    let env = delivered();
    let (slot, _) = project_with_unlanded_work(&env);
    let head = fs::read_to_string(slot.join("work.txt")).unwrap();

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    assert_eq!(fs::read_to_string(slot.join("work.txt")).unwrap(), head);
    let report = said(&output);
    assert!(
        report.contains(&slot.file_name().unwrap().to_string_lossy().into_owned())
            && report.contains("1 commit not landed"),
        "{report}"
    );
}

#[test]
fn a_context_link_into_the_home_is_removed() {
    let env = delivered();
    let (_, link) = project_with_unlanded_work(&env);

    let output = uninstall(&env, &["--yes"]);

    assert!(output.status.success(), "{}", said(&output));
    assert!(link.symlink_metadata().is_err(), "{}", said(&output));
}

// --- Harness binaries are the person's -------------------------------------

#[test]
fn a_harness_setup_provisioned_stays_and_is_named() {
    let env = delivered();
    let provisioning = env.uze_home.join("state/provisioning.json");
    let mut registry: serde_json::Value = fs::read(&provisioning)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({"harnesses": {}}));
    registry["harnesses"]["opencode"] = serde_json::json!({
        "action": "INSTALL",
        "status": "VERIFIED",
        "method": "official-installer",
        "version": "9.9.9",
        "recorded_at_unix_secs": 0,
    });
    fs::write(&provisioning, serde_json::to_vec_pretty(&registry).unwrap()).unwrap();
    let opencode = env
        .fake_bin
        .join(uze_platform::executable::file_name("opencode2"));

    let output = uninstall(&env, &["--yes", "--purge"]);

    assert!(output.status.success(), "{}", said(&output));
    assert!(opencode.is_file());
    assert!(said(&output).contains("OpenCode"), "{}", said(&output));
}

// --- The binary is removed only where UZE placed it ------------------------

#[test]
fn a_binary_installed_by_cargo_is_left_with_the_command_that_removes_it() {
    let env = TestEnvironment::isolated();
    let cargo_home = env.root().join("cargo");
    let binary = cargo_home
        .join("bin")
        .join(uze_platform::executable::file_name("uze"));
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    // Written through a process of its own, so no sibling test's fork holds
    // a write descriptor to it when it runs (ETXTBSY).
    uze_platform::executable::install(&binary, &fs::read(uze_bin()).unwrap()).unwrap();

    let output = env
        .command(&binary)
        .args(["uninstall", "--yes"])
        .env("CARGO_HOME", &cargo_home)
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", said(&output));
    assert!(binary.is_file());
    assert!(
        said(&output).contains("cargo uninstall uze"),
        "{}",
        said(&output)
    );
}

#[test]
fn a_binary_the_installer_placed_is_removed() {
    let env = delivered();
    let binary = placed_by_the_installer(&env, &env.root().join("installed-bin"));

    let output = uninstall_with(&env, &binary, &["--yes"]);

    assert!(output.status.success(), "{}", said(&output));
    assert!(!binary.exists());
    assert!(env.uze_home.join("state/install.json").is_file());
}
