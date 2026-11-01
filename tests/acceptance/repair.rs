//! Acceptance: `uze doctor --repair`, the damage matrix of change
//! `doctor-repair` (`specs/machine-repair/spec.md`). Each test damages one
//! subject of the health catalogue on a machine UZE set up, runs the real
//! binary against the fake harnesses, and asserts what the machine holds
//! afterwards: healthy, or the finding left for the person or the network.
//! A second run repairs nothing.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};

use uze_testkit::{fixtures, temp::TestEnvironment};

use crate::util::{install_fake_harnesses, uze_bin};

const PACKAGE: &str = "store-tie@test";

/// A machine UZE set up for every fake harness, with `store-tie` delivered
/// from a marketplace.
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

fn repair(env: &TestEnvironment) -> Output {
    env.command(uze_bin())
        .args(["doctor", "--repair"])
        .output()
        .expect("uze runs")
}

fn repair_json(env: &TestEnvironment) -> (Option<i32>, serde_json::Value) {
    let output = env
        .command(uze_bin())
        .args(["doctor", "--repair", "--format", "json"])
        .output()
        .expect("uze runs");
    let report = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", said(&output)));
    (output.status.code(), report)
}

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The outcome each entry of a JSON repair report names, with what it is
/// about.
fn outcomes(report: &serde_json::Value) -> Vec<(String, String, String)> {
    report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["finding"]["subject"].as_str().unwrap().to_owned(),
                entry["finding"]["about"].as_str().unwrap().to_owned(),
                entry["outcome"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn registry(env: &TestEnvironment) -> serde_json::Value {
    serde_json::from_slice(&fs::read(env.uze_home.join("state/packages.json")).unwrap()).unwrap()
}

fn recorded_digest(env: &TestEnvironment, id: &str) -> String {
    registry(env)["packages"][id]["digest"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn package_bytes(env: &TestEnvironment, id: &str) -> PathBuf {
    let (name, marketplace) = id.split_once('@').unwrap();
    env.uze_home
        .join("sources/plugins")
        .join(marketplace)
        .join(name)
}

fn fake_state(env: &TestEnvironment, vendor: &str) -> PathBuf {
    env.root().join("fake-state").join(vendor)
}

fn registered(env: &TestEnvironment, vendor: &str) -> Vec<(String, String)> {
    fs::read_to_string(fake_state(env, vendor).join("marketplaces"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, root) = line.split_once('\t')?;
            Some((name.to_owned(), root.to_owned()))
        })
        .collect()
}

/// Every file under `root` with its bytes.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(_) => files.push((path.clone(), fs::read(&path).unwrap_or_default())),
                Err(_) => {}
            }
        }
    }
    files.sort();
    files
}

/// Asserts a second run finds the machine as the first left it.
fn a_second_run_repairs_nothing(env: &TestEnvironment) {
    let (_, again) = repair_json(env);
    assert!(
        !outcomes(&again)
            .iter()
            .any(|(_, _, outcome)| outcome == "repaired"),
        "{again:#}"
    );
}

#[test]
fn a_healthy_machine_is_left_as_it_is() {
    let env = delivered();
    let before = snapshot(&env.uze_home.join("sources"));

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(
        said(&output).contains("nothing to repair"),
        "{}",
        said(&output)
    );
    assert_eq!(snapshot(&env.uze_home.join("sources")), before);
}

#[test]
fn deleted_sources_come_back_with_the_digest_recorded_at_ingest() {
    let env = delivered();
    let digest = recorded_digest(&env, PACKAGE);
    let bundled = registry(&env)["packages"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|id| id.as_str() != PACKAGE)
        .cloned()
        .collect::<Vec<_>>();
    fs::remove_dir_all(env.uze_home.join("sources")).unwrap();

    let doctor = env
        .command(uze_bin())
        .arg("doctor")
        .output()
        .expect("uze runs");
    assert!(
        said(&doctor).contains("uze doctor --repair"),
        "plain doctor names the repair: {}",
        said(&doctor)
    );

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(package_bytes(&env, PACKAGE).join("plugin.json").is_file());
    assert_eq!(recorded_digest(&env, PACKAGE), digest, "no version changed");
    for id in bundled {
        assert!(
            package_bytes(&env, &id).is_dir(),
            "the bundled {id} is back from the binary's snapshot"
        );
    }
    a_second_run_repairs_nothing(&env);
}

#[test]
fn deleted_sources_and_stores_are_acquired_then_built_again() {
    let env = delivered();
    fs::remove_dir_all(env.uze_home.join("sources")).unwrap();
    fs::remove_dir_all(env.uze_home.join("store")).unwrap();

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(package_bytes(&env, PACKAGE).is_dir());
    for vendor in ["claude", "codex"] {
        assert!(
            env.uze_home
                .join("store")
                .join(vendor)
                .join(PACKAGE)
                .is_dir(),
            "{vendor}: {}",
            said(&output)
        );
    }
    a_second_run_repairs_nothing(&env);
}

/// The source moved on: its bytes are not the ones installed, so nothing
/// is kept, the builds go on delivering, and the person is told how to
/// update.
#[test]
fn a_source_that_moved_on_is_left_for_an_update() {
    let env = delivered();
    let mut packages = registry(&env);
    packages["packages"][PACKAGE]["digest"] = serde_json::json!("sha256:moved-on");
    fs::write(
        env.uze_home.join("state/packages.json"),
        serde_json::to_vec_pretty(&packages).unwrap(),
    )
    .unwrap();
    fs::remove_dir_all(package_bytes(&env, PACKAGE)).unwrap();
    let build = env.uze_home.join("store/claude").join(PACKAGE);
    let built = snapshot(&build);

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    assert!(
        said(&output).contains(&format!("uze update {PACKAGE}")),
        "{}",
        said(&output)
    );
    assert!(
        !package_bytes(&env, PACKAGE).exists(),
        "no bytes were added"
    );
    assert_eq!(snapshot(&build), built, "the build keeps delivering");
}

/// Offline: the marketplace's clone and its source are both out of reach,
/// so the package waits on the network while every delivery keeps working.
#[test]
fn offline_the_package_waits_on_the_network_and_deliveries_keep_working() {
    let env = delivered();
    fs::remove_dir_all(package_bytes(&env, PACKAGE)).unwrap();
    fs::remove_dir_all(env.uze_home.join("cache/marketplaces")).unwrap();
    fs::rename(env.root().join("market"), env.root().join("market-away")).unwrap();
    let build = env.uze_home.join("store/claude").join(PACKAGE);
    let built = snapshot(&build);

    let (status, report) = repair_json(&env);

    assert_eq!(status, Some(2), "{report:#}");
    assert!(
        outcomes(&report).contains(&(
            "package_bytes".to_owned(),
            PACKAGE.to_owned(),
            "needs_network".to_owned()
        )),
        "{report:#}"
    );
    assert_eq!(snapshot(&build), built);
}

/// Codex's marketplace left under the runtime tree, as an earlier release
/// registered it, is moved to its vendor store and its plugin stays.
#[test]
fn a_codex_marketplace_registered_elsewhere_is_repointed() {
    let env = delivered();
    let earlier = env.uze_home.join("runtime/attachments/codex/generated");
    fs::create_dir_all(&earlier).unwrap();
    fs::write(
        fake_state(&env, "codex").join("marketplaces"),
        format!("uze-store\t{}\n", earlier.display()),
    )
    .unwrap();

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(
        registered(&env, "codex"),
        [(
            "uze-store".to_owned(),
            env.uze_home.join("store/codex").display().to_string()
        )]
    );
    let installed = fs::read_to_string(fake_state(&env, "codex").join("installed")).unwrap();
    assert!(installed.contains("store-tie@uze-store"), "{installed}");
    a_second_run_repairs_nothing(&env);
}

#[test]
fn a_deleted_launcher_is_placed_again() {
    let env = delivered();
    let shims = env.uze_home.join("shims");
    let placed: Vec<_> = fs::read_dir(&shims)
        .map(|entries| entries.flatten().map(|entry| entry.file_name()).collect())
        .unwrap_or_default();
    if placed.is_empty() {
        eprintln!("skipped: setup placed no launcher for the fake harnesses");
        return;
    }
    fs::remove_dir_all(&shims).unwrap();

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    for name in placed {
        assert!(shims.join(&name).exists(), "{name:?}: {}", said(&output));
    }
    a_second_run_repairs_nothing(&env);
}

/// A socket a stopped server left is cleared through the terminal runtime.
// Unix only: the endpoint is a socket file there; a named pipe leaves none.
#[cfg(unix)]
#[test]
fn a_stale_workspace_endpoint_is_cleared() {
    let env = delivered();
    let socket = {
        let mut scope = uze_testkit::env::scope();
        scope.set("UZE_HOME", &env.uze_home);
        uze_terminal::socket_path().unwrap()
    };
    fs::create_dir_all(socket.parent().unwrap()).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(!socket.exists(), "{}", said(&output));
    a_second_run_repairs_nothing(&env);
}

// --- What repair never does ------------------------------------------------

/// A file someone edited in a build is theirs: kept, and named.
#[test]
fn an_edited_build_is_left_and_reported() {
    let env = delivered();
    let build = env.uze_home.join("store/claude").join(PACKAGE);
    let edited = fs::read_dir(&build)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_file())
        .or_else(|| {
            snapshot(&build)
                .into_iter()
                .map(|(path, _)| path)
                .find(|path| path.extension().is_some_and(|ext| ext == "md"))
        })
        .expect("a file in the build");
    fs::write(&edited, "edited by hand\n").unwrap();

    let (status, report) = repair_json(&env);

    assert_eq!(status, Some(1), "{report:#}");
    assert_eq!(fs::read_to_string(&edited).unwrap(), "edited by hand\n");
    assert!(
        outcomes(&report)
            .iter()
            .any(|(subject, about, outcome)| subject == "builds"
                && about == PACKAGE
                && outcome == "needs_human"),
        "{report:#}"
    );
}

/// The receipt ledger is the only proof of what UZE delivered: one that
/// does not read is left as it is, no delivery changes, and the person is
/// told.
#[test]
fn an_unreadable_ledger_is_left_untouched() {
    let env = delivered();
    let ledger = env.uze_home.join("state/attachments.json");
    fs::write(&ledger, "{ not a ledger").unwrap();
    let vendors = snapshot(&env.root().join("fake-state"));

    let (status, report) = repair_json(&env);

    assert_eq!(status, Some(1), "{report:#}");
    assert_eq!(fs::read_to_string(&ledger).unwrap(), "{ not a ledger");
    assert_eq!(snapshot(&env.root().join("fake-state")), vendors);
    assert!(
        outcomes(&report)
            .iter()
            .any(|(subject, about, outcome)| subject == "records"
                && about == "ledger"
                && outcome == "needs_human"),
        "{report:#}"
    );
}

/// A project, its checkouts and its branches are the person's: repair
/// never writes into one.
#[test]
fn a_project_is_never_touched() {
    let env = delivered();
    let project = env.root().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("agents.yaml"), "version: 1\n").unwrap();
    fs::write(project.join("AGENTS.md"), "# Mine\n").unwrap();
    fs::remove_dir_all(env.uze_home.join("sources")).unwrap();
    let before = snapshot(&project);

    let output = env
        .command(uze_bin())
        .current_dir(&project)
        .args(["doctor", "--repair"])
        .output()
        .expect("uze runs");

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(snapshot(&project), before);
}

/// After an upgrade the bundled plugin's snapshot is this binary's own
/// version of it: its bytes come back from the binary, offline, and its
/// recorded digest becomes the snapshot's rather than holding them back.
#[test]
fn the_bundled_plugin_comes_back_from_the_binary_with_its_digest() {
    let env = delivered();
    let bundled = "uze@uze";
    let mut packages = registry(&env);
    packages["packages"][bundled]["digest"] = serde_json::json!("sha256:an-earlier-release");
    fs::write(
        env.uze_home.join("state/packages.json"),
        serde_json::to_vec_pretty(&packages).unwrap(),
    )
    .unwrap();
    fs::remove_dir_all(package_bytes(&env, bundled)).unwrap();
    fs::rename(env.root().join("market"), env.root().join("market-away")).unwrap();

    let output = repair(&env);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(package_bytes(&env, bundled).join("plugin.json").is_file());
    assert_ne!(recorded_digest(&env, bundled), "sha256:an-earlier-release");
    a_second_run_repairs_nothing(&env);
}

// --- The home against its schema ------------------------------------------

/// The entries of a JSON repair report's comparison of the home, by what
/// each is and whether it was removed.
fn home_entries(report: &serde_json::Value) -> Vec<(String, String, bool)> {
    report["home"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["finding"].as_str().unwrap().to_owned(),
                entry["path"].as_str().unwrap().to_owned(),
                entry["removed"].as_bool().unwrap(),
            )
        })
        .collect()
}

/// A file of an earlier release that nothing reads is gone after a
/// repair, which runs no bootstrap to have removed it first.
#[test]
fn a_file_of_an_earlier_release_is_removed() {
    let env = delivered();
    let retired = env.uze_home.join("state/profiles.json");
    fs::write(&retired, "{}").unwrap();

    let (code, report) = repair_json(&env);

    assert_eq!(code, Some(0), "{report:#}");
    assert!(!retired.exists(), "{report:#}");
    a_second_run_repairs_nothing(&env);
}

/// Something no part of this build declares, in the records, the bytes or
/// the operator's own files, may be a newer build's: reported, never
/// deleted, however often repair runs.
#[test]
fn an_unknown_entry_is_reported_and_never_deleted() {
    let env = delivered();
    let unknown = [
        env.uze_home.join("state/from-a-newer-build.json"),
        env.uze_home.join("sources/next-layout/flow"),
        env.uze_home.join("notes.txt"),
    ];
    for path in &unknown {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "kept").unwrap();
    }

    for _ in 0..2 {
        let (code, report) = repair_json(&env);
        assert_eq!(code, Some(0), "{report:#}");
        let found = home_entries(&report);
        for path in &unknown {
            assert_eq!(fs::read_to_string(path).unwrap(), "kept");
        }
        for name in ["from-a-newer-build.json", "next-layout", "notes.txt"] {
            assert!(
                found
                    .iter()
                    .any(|(finding, path, removed)| finding == "unknown"
                        && path.ends_with(name)
                        && !removed),
                "{name}: {found:?}"
            );
        }
    }
}

/// A record the ladder set aside may be the person's lost work: kept, with
/// its size, until they pass `--discard-leftovers`.
#[test]
fn a_set_aside_record_is_kept_until_the_person_discards_it() {
    let env = delivered();
    let set_aside = env.uze_home.join("state/hosts.json.unreadable-1700000000");
    fs::write(&set_aside, "the person's lost hosts").unwrap();

    let (code, report) = repair_json(&env);
    assert_eq!(code, Some(0), "{report:#}");
    assert!(set_aside.exists(), "{report:#}");
    let entry = report["home"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["finding"] == "set_aside")
        .unwrap_or_else(|| panic!("{report:#}"));
    assert!(entry["bytes"].as_u64().is_some_and(|bytes| bytes > 0));
    let text = said(&repair(&env));
    assert!(
        text.contains("uze doctor --repair --discard-leftovers"),
        "{text}"
    );

    let output = env
        .command(uze_bin())
        .args(["doctor", "--repair", "--discard-leftovers"])
        .output()
        .expect("uze runs");
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(!set_aside.exists(), "{}", said(&output));
}

/// A projection is judged by whether its project is still there, never by
/// its age: one untouched for months but still used is kept, and one whose
/// project is gone is collected.
#[test]
fn a_projection_in_use_is_kept_however_old() {
    let env = delivered();
    let home = uze_application::UzeHome::at(&env.uze_home);
    let used = env.root().join("used-project");
    let gone = env.root().join("gone-project");
    fs::create_dir_all(&used).unwrap();
    fs::create_dir_all(&gone).unwrap();
    let projection = |root: &Path| {
        let root = fs::canonicalize(root).unwrap();
        uze_core::harness_runtime::prepare_projection(&home, "codex", &root).unwrap()
    };
    let in_use = projection(&used);
    let left = projection(&gone);
    fs::remove_dir_all(&gone).unwrap();
    let months_ago =
        std::time::SystemTime::now() - std::time::Duration::from_secs(120 * 24 * 60 * 60);
    for dir in [&in_use, in_use.parent().unwrap()] {
        fs::File::open(dir)
            .unwrap()
            .set_modified(months_ago)
            .unwrap();
    }

    let (code, report) = repair_json(&env);

    assert_eq!(code, Some(0), "{report:#}");
    assert!(in_use.is_dir(), "a projection in use was collected");
    assert!(!left.exists(), "a projection of a gone project was kept");
}

/// While a workspace runs, its terminal server's state — record, lock,
/// operator key, endpoint — is live: repair, plain `doctor` and the
/// collectors leave every byte of it as it is, even what would otherwise
/// be a leftover, an empty directory or unknown.
// Unix only: the server's endpoint is a socket file there.
#[cfg(unix)]
#[test]
fn the_terminal_runtime_is_untouched_while_a_workspace_runs() {
    let env = delivered();
    let git = |args: &[&str]| {
        let output = env
            .command("git")
            .current_dir(&env.project)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", said(&output));
    };
    git(&["init", "--quiet", "-b", "main", "."]);
    let socket = {
        let mut scope = uze_testkit::env::scope();
        scope.set("UZE_HOME", &env.uze_home);
        uze_terminal::socket_path().unwrap()
    };
    let mut server = env
        .command(uze_bin())
        .args(["terminal", "serve", "--project"])
        .arg(&env.project)
        .env("SHELL", "/bin/sh")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the real uze binary serves a terminal");
    let started = std::time::Instant::now();
    while std::os::unix::net::UnixStream::connect(&socket).is_err() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "the server never answered at {}",
            socket.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let terminal = env.uze_home.join("state/terminal");
    fs::create_dir_all(terminal.join("empty")).unwrap();
    fs::write(terminal.join("workspace.json.unreadable-1700000000"), "x").unwrap();
    fs::write(terminal.join("unknown"), "x").unwrap();
    let before = snapshot(&terminal);

    for args in [
        &["doctor", "--repair", "--discard-leftovers"][..],
        &["doctor"][..],
    ] {
        let output = env.command(uze_bin()).args(args).output().unwrap();
        assert!(output.status.code().is_some(), "{}", said(&output));
    }

    let after = snapshot(&terminal);
    let answers = std::os::unix::net::UnixStream::connect(&socket).is_ok();
    let _ = server.kill();
    let _ = server.wait();
    assert!(answers, "the running server's endpoint was taken");
    assert_eq!(after, before);
    assert!(terminal.join("empty").is_dir());
}
