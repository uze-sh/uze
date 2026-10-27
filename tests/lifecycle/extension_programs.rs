//! A declared extension's program, installed from a marketplace nobody
//! links, consented to and enabled, run by the runner through this build's
//! own `uze __confine` on the real kernel (`extension-programs-and-
//! confinement`, tasks 5.3). The program is one packaged `sh` script that
//! does what its first argument names, so each test is one reach tried
//! and, where this machine confines, refused.

use std::fs;
use std::path::{Path, PathBuf};

use uze_application::{Asker, ExtensionIdentity, ExtensionRunner, UzeApplication};
use uze_contract::{Answer, Asking, Question};
use uze_core::{UzeHome, trust::AlwaysTrust};
use uze_testkit::temp::TestEnvironment;

const MARKET: &str = "proving";

const MANIFEST: &str = r#"protocol 1
id "probe"
name "Probe"
description "Tries one reach at a time"
wants "exec:sh"
surface "board" slot="view" view="board.kdl" source="probe"
source "probe" {
    run "./bin/probe" "--" "{do}" "{arg}" timeout="2s"
    output "lines"
    fields "line"
}
"#;

const PROBE: &str = r#"#!/bin/sh
# Does what $2 says ($1 is the `--`), with $3 as its argument.
case "$2" in
    env) env ;;
    answer) echo "hello from the probe" ;;
    write) echo x > "$3" && echo wrote ;;
    read) cat "$3" ;;
    scratch) echo x > "$TMPDIR/note" && cat "$TMPDIR/note" ;;
    signal) kill -0 "$3" && echo signalled ;;
    hang) sleep 30 & echo "$!" > "$3.pid"; wait ;;
    flood) yes flood ;;
    escape) setsid sleep 30 && echo escaped ;;
    fill) head -c 100000000 /dev/zero > "$TMPDIR/fill" && echo filled ;;
    socket) python3 -I -c 'import socket; socket.socket()' && echo opened ;;
    unix) python3 -I -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1])' "$3" && echo connected ;;
    *) echo "unknown $2" >&2; exit 2 ;;
esac
"#;

struct World {
    env: TestEnvironment,
    app: UzeApplication,
    probe: ExtensionIdentity,
    checkout: PathBuf,
    outside: PathBuf,
}

impl World {
    fn new(label: &str) -> Self {
        let env = TestEnvironment::isolated();
        let plugin = env.root().join("plugin");
        let extension = plugin.join("extensions/probe");
        fs::create_dir_all(extension.join("bin")).unwrap();
        fs::write(
            plugin.join("plugin.json"),
            r#"{"name": "probe", "description": "Probe."}"#,
        )
        .unwrap();
        fs::write(extension.join("extension.kdl"), MANIFEST).unwrap();
        fs::write(
            extension.join("board.kdl"),
            "Stack {\n    Text \"{line}\"\n}\n",
        )
        .unwrap();
        fs::write(extension.join("bin/probe"), PROBE).unwrap();
        let market = env.root().join("market");
        let manifest = serde_json::json!({
            "name": MARKET,
            "plugins": [{ "name": "probe", "source": "./plugins/probe", "description": "Probe." }],
        });
        uze_testkit::marketplace::stage(
            &market,
            &manifest.to_string(),
            &[("probe".to_owned(), plugin)],
        );
        let app = UzeApplication::new(UzeHome::at(&env.uze_home), Vec::new());
        app.marketplace()
            .add(&market.display().to_string())
            .expect("the marketplace registers");
        app.marketplace()
            .install_plugin(&format!("probe@{MARKET}"), &AlwaysTrust)
            .expect("the probe plugin installs");
        // A checkout outside the system's temporary directory, which no
        // extension may read: Cargo's own scratch for integration tests.
        let base = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("extension-programs-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let checkout = base.join("project");
        fs::create_dir_all(checkout.join(".git/hooks")).unwrap();
        fs::write(checkout.join("README.md"), "# project\n").unwrap();
        uze_testkit::git::commit_everything_in(&checkout);
        let outside = base.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("id_ed25519"), "PRIVATE KEY\n").unwrap();
        let probe = app
            .extensions()
            .installed()
            .into_iter()
            .find(|extension| extension.identity.id == "probe")
            .expect("the probe is installed")
            .identity;
        app.extensions()
            .consent(&probe, Asker::Operator)
            .expect("the operator consents");
        // Enabled in the project the runner finds the checkout in, as the
        // workspace enables it.
        app.extensions()
            .enable(
                &probe,
                &uze_application::project_root_of(&checkout),
                Asker::Operator,
            )
            .expect("the operator enables it here");
        Self {
            env,
            app,
            probe,
            checkout,
            outside,
        }
    }

    fn ask(&self, runner: &ExtensionRunner, what: &str, arg: &str) -> Answer {
        runner.answer(
            &self.app.extensions(),
            &Question {
                identity: self.probe.clone(),
                declaration: "probe".to_owned(),
                asking: Asking::Refresh,
                input: serde_json::json!({ "do": what, "arg": arg }),
                checkout: Some(self.checkout.clone()),
            },
        )
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.checkout.parent().unwrap());
    }
}

/// This build's binary as the confinement helper, when the machine
/// confines; `None`, with the reason printed, when it does not.
fn confining() -> Option<ExtensionRunner> {
    let availability = uze_application::confinement();
    if !availability.is_available() {
        eprintln!(
            "skipped: this machine does not confine programs ({})",
            availability.summary()
        );
        return None;
    }
    Some(ExtensionRunner::confining_through(Some(PathBuf::from(
        env!("CARGO_BIN_EXE_uze"),
    ))))
}

fn lines(answer: &Answer) -> Vec<String> {
    match answer {
        Answer::Answered { envelope, .. } => envelope
            .data
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["line"].as_str().unwrap_or_default().to_owned())
            .collect(),
        other => panic!("not answered: {other:?}"),
    }
}

fn failed(answer: &Answer) -> String {
    match answer {
        Answer::Failed { reason, .. } => reason.clone(),
        other => panic!("not a failure: {other:?}"),
    }
}

#[test]
fn a_confined_program_answers_from_its_own_scratch_and_inherits_no_secret() {
    let world = World::new("answer");
    let Some(runner) = confining() else { return };
    assert_eq!(
        lines(&world.ask(&runner, "answer", "")),
        ["hello from the probe"]
    );
    assert_eq!(lines(&world.ask(&runner, "scratch", "")), ["x"]);
    let seen = lines(&world.ask(&runner, "env", ""));
    assert!(
        seen.iter()
            .all(|line| !line.starts_with("CARGO") && !line.starts_with("RUST")),
        "the test process's environment leaked: {seen:?}"
    );
    assert!(seen.iter().any(|line| line == "UZE_IN_DO=env"), "{seen:?}");
    assert!(
        seen.iter().any(|line| line.starts_with("TMPDIR=")),
        "{seen:?}"
    );
}

#[test]
fn a_confined_program_writes_nowhere_but_its_scratch() {
    let world = World::new("write");
    let Some(runner) = confining() else { return };
    let hook = world.checkout.join(".git/hooks/pre-commit");
    for target in [
        hook.clone(),
        world.checkout.join("README.md"),
        world.outside.join("planted"),
        UzeHome::at(&world.env.uze_home).state_dir().join("planted"),
    ] {
        let reason = failed(&world.ask(&runner, "write", &target.display().to_string()));
        assert!(
            reason.contains("Permission denied"),
            "{}: {reason}",
            target.display()
        );
    }
    assert!(!hook.exists());
    assert_eq!(
        fs::read_to_string(world.checkout.join("README.md")).unwrap(),
        "# project\n"
    );
}

#[test]
fn a_confined_program_reads_the_checkout_and_nothing_outside_it() {
    let world = World::new("read");
    let Some(runner) = confining() else { return };
    assert_eq!(
        lines(&world.ask(
            &runner,
            "read",
            &world.checkout.join("README.md").display().to_string()
        )),
        ["# project"]
    );
    let key = world.outside.join("id_ed25519").display().to_string();
    assert!(failed(&world.ask(&runner, "read", &key)).contains("Permission denied"));
    let environ = format!("/proc/{}/environ", std::process::id());
    assert!(failed(&world.ask(&runner, "read", &environ)).contains("Permission denied"));
}

#[test]
fn a_confined_program_signals_nothing_outside_its_run() {
    let world = World::new("signal");
    let Some(runner) = confining() else { return };
    let reason = failed(&world.ask(&runner, "signal", &std::process::id().to_string()));
    assert!(reason.contains("exited"), "{reason}");
}

#[test]
fn a_confined_program_opens_no_socket() {
    let world = World::new("socket");
    let Some(runner) = confining() else { return };
    if uze_platform::executable::on_path("python3").is_none() {
        eprintln!("skipped: no python3 to open a socket with");
        return;
    }
    assert!(failed(&world.ask(&runner, "socket", "")).contains("PermissionError"));
    let bus = std::env::var("XDG_RUNTIME_DIR").map_or_else(
        |_| "/run/dbus/system_bus_socket".to_owned(),
        |dir| format!("{dir}/bus"),
    );
    assert!(failed(&world.ask(&runner, "unix", &bus)).contains("PermissionError"));
}

#[test]
fn a_hang_and_its_children_end_at_the_time_limit() {
    let world = World::new("hang");
    let Some(runner) = confining() else { return };
    let marker = world.checkout.parent().unwrap().join("scratch-free");
    // The marker sits where the program cannot write, so the hang's pid
    // is read back through its scratch instead: it is the time limit
    // that ends it, whatever it wrote.
    let reason = failed(&world.ask(&runner, "hang", &marker.display().to_string()));
    assert!(reason.contains("timed out after 2s"), "{reason}");
}

/// No process of a run can leave the group the runner ends, so a fork
/// bomb's children go with it.
#[test]
fn a_confined_program_cannot_leave_its_process_group() {
    let world = World::new("escape");
    let Some(runner) = confining() else { return };
    let reason = failed(&world.ask(&runner, "escape", ""));
    assert!(reason.contains("Operation not permitted"), "{reason}");
}

#[test]
fn a_flood_of_output_or_of_scratch_is_cut_off() {
    let world = World::new("flood");
    let Some(runner) = confining() else { return };
    assert!(failed(&world.ask(&runner, "flood", "")).contains("more than 1024 KiB on stdout"));
    let reason = failed(&world.ask(&runner, "fill", ""));
    assert!(
        reason.contains("File size limit exceeded") || reason.contains("signal: 25"),
        "{reason}"
    );
}

#[test]
fn a_helper_that_cannot_confine_runs_nothing() {
    let world = World::new("unenforced");
    if !uze_application::confinement().is_available() {
        eprintln!("skipped: the record is accepted unconfined on this machine");
        return;
    }
    let helper = world.checkout.parent().unwrap().join("broken-helper");
    fs::write(&helper, "#!/bin/sh\necho 'cannot apply' >&2\nexit 125\n").unwrap();
    uze_platform::executable::make_runnable(&helper).unwrap();
    let runner = ExtensionRunner::confining_through(Some(helper));
    match world.ask(&runner, "answer", "") {
        Answer::Withheld { reason } => {
            assert!(
                reason.contains("did not apply") && reason.contains("nothing ran"),
                "{reason}"
            )
        }
        other => panic!("ran anyway: {other:?}"),
    }
}
