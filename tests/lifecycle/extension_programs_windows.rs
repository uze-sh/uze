//! A declared extension's program on Windows, consented to and enabled,
//! run by the runner through this build's own `uze __confine` in an
//! AppContainer (`confinement-parity`). The program is the system's
//! `cmd.exe`, pinned at consent, so each source is one reach tried. The
//! path arrives as `UZE_IN_PATH`, which `cmd` expands itself: an argument
//! substituted from the input would have to follow `--`, which `cmd`'s own
//! commands do not take.

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
wants "exec:cmd"
surface "board" slot="view" view="board.uze" source="read"
source "read" {
    run "cmd" "/d" "/c" "type %UZE_IN_PATH%" timeout="10s"
    output "lines"
    fields "line"
}
source "scratch" {
    run "cmd" "/d" "/c" "echo kept> %TMPDIR%\\note && type %TMPDIR%\\note" timeout="10s"
    output "lines"
    fields "line"
}
source "write" {
    run "cmd" "/d" "/c" "echo planted> %UZE_IN_PATH%" timeout="10s"
    output "lines"
    fields "line"
}
"#;

struct World {
    app: UzeApplication,
    probe: ExtensionIdentity,
    checkout: PathBuf,
    outside: PathBuf,
    home: UzeHome,
    _env: TestEnvironment,
}

impl World {
    fn new(label: &str) -> Self {
        let env = TestEnvironment::isolated();
        let plugin = env.root().join("plugin");
        let extension = plugin.join("extensions/probe");
        fs::create_dir_all(&extension).unwrap();
        fs::write(
            plugin.join("plugin.json"),
            r#"{"name": "probe", "description": "Probe."}"#,
        )
        .unwrap();
        fs::write(extension.join("extension.kdl"), MANIFEST).unwrap();
        fs::write(
            extension.join("board.uze"),
            "<view>\nStack {\n    Text \"{line}\"\n}\n</view>\n",
        )
        .unwrap();
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
        let home = UzeHome::at(&env.uze_home);
        let app = UzeApplication::new(home.clone(), Vec::new());
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
        fs::create_dir_all(&checkout).unwrap();
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
            .consent(
                &probe,
                &app.extensions().digest(&probe).unwrap(),
                Asker::Operator,
            )
            .expect("the operator consents");
        app.extensions()
            .enable(
                &probe,
                &uze_application::project_root_of(&checkout),
                Asker::Operator,
            )
            .expect("the operator enables it here");
        Self {
            app,
            probe,
            checkout,
            outside,
            home,
            _env: env,
        }
    }

    fn ask(&self, runner: &ExtensionRunner, source: &str, path: &Path) -> Answer {
        runner.answer(
            &self.app.extensions(),
            &Question {
                identity: self.probe.clone(),
                declaration: source.to_owned(),
                asking: Asking::Refresh,
                input: serde_json::json!({ "path": path.display().to_string() }),
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

fn confining() -> ExtensionRunner {
    let availability = uze_application::confinement();
    assert!(availability.is_available(), "{}", availability.summary());
    ExtensionRunner::confining_through(Some(PathBuf::from(env!("CARGO_BIN_EXE_uze"))))
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
fn a_confined_program_reads_the_checkout_and_nothing_outside_it() {
    let world = World::new("read");
    let runner = confining();
    assert_eq!(
        lines(&world.ask(&runner, "read", &world.checkout.join("README.md"))),
        ["# project"]
    );
    let refusal = failed(&world.ask(&runner, "read", &world.outside.join("id_ed25519")));
    assert!(refusal.contains("denied"), "{refusal}");
}

#[test]
fn a_confined_program_writes_nowhere_but_its_scratch() {
    let world = World::new("write");
    let runner = confining();
    assert_eq!(
        lines(&world.ask(&runner, "scratch", &world.checkout)),
        ["kept"],
        "its own scratch is the one place it writes"
    );
    for target in [
        world.checkout.join("README.md"),
        world.outside.join("planted"),
        world.home.state_dir().join("planted"),
    ] {
        let refusal = failed(&world.ask(&runner, "write", &target));
        assert!(
            refusal.contains("denied"),
            "{}: {refusal}",
            target.display()
        );
    }
    assert_eq!(
        fs::read_to_string(world.checkout.join("README.md")).unwrap(),
        "# project\n"
    );
    assert!(!world.outside.join("planted").exists());
}
