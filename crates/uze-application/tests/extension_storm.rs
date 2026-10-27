//! D28's storm: one `agent.turn.finished` reaching 15 extensions on 5
//! agents of one checkout, answered under 50 ms because the answers are
//! shared (`extension-platform` D28).
//!
//! Its own test target, for the reason `performance.rs` gives: a clock
//! inside a lib-test binary measures its siblings too. The storm is what
//! the workspace asks of the runner: each extension's lane, on the pool of
//! two threads, refreshes its source once and its four other bands ask the
//! same question, answered from what the refresh remembered.
//!
//! The claim is exact and the same everywhere: fifteen runs, not
//! seventy-five. The clock is a backstop beside it, held to D28's 50 ms
//! in an optimized build. Unoptimized, every question still reads and
//! digests the extension's files (what consent is judged against), and
//! that SHA-256 runs several times slower: about 65 ms on a 4-vCPU VM, so
//! an unoptimized build is held to four times the target. The storm runs
//! three times and the fastest is held: what the path can do, which a
//! loaded machine only adds to.
//!
//! Those clocks were measured on Linux and macOS, and hold there. Elsewhere
//! the digest's reads cost what the platform's filesystem charges (a
//! Windows runner's fastest unoptimized storm took 217 ms), so the clock
//! there is a backstop against a storm an order of magnitude slower, and
//! the run count is the claim.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uze_application::{Asker, ExtensionIdentity, ExtensionRunner, UzeApplication};
use uze_contract::{Answer, Asking, Question};

const EXTENSIONS: usize = 15;
const AGENTS: usize = 5;
const POOL: usize = 2;
const TARGET: Duration = Duration::from_millis(50);
/// What an unoptimized build is held to, for the reason above.
const UNOPTIMIZED: u32 = 4;
/// Where D28's clocks were measured, as `uze_platform::target::OS` names it.
const MEASURED_ON: &[&str] = &["linux", "macos"];
/// The backstop wherever they were not.
const UNMEASURED: Duration = Duration::from_secs(1);

/// A Git checkout outside the system's temporary directory, which an
/// extension may never read: beside the build's own output.
fn checkout(label: &str) -> PathBuf {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/extension-checkouts")
        .join(format!("{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&checkout);
    std::fs::create_dir_all(checkout.join(".planning")).unwrap();
    std::fs::write(checkout.join(".planning/STATE.md"), "one\ntwo\nthree").unwrap();
    uze_testkit::git::isolated_git_in(&checkout, &["init", "-q"]);
    uze_platform::path::canonical(&checkout).unwrap()
}

/// Fifteen plugins carrying the `gsd` fixture's band, installed from one
/// linked marketplace, each consented to and enabled in `checkout`.
fn fifteen_bands(scratch: &Path, checkout: &Path) -> (UzeApplication, Vec<ExtensionIdentity>) {
    let gsd = uze_testkit::fixtures::canonical("gsd");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(gsd.join("plugin.json")).unwrap()).unwrap();
    let mut plugins = Vec::new();
    let mut listed = Vec::new();
    for index in 0..EXTENSIONS {
        let name = format!("gsd{index}");
        let copy = scratch.join("plugins").join(&name);
        uze_testkit::fixtures::copy_tree(&gsd, &copy);
        let mut renamed = manifest.clone();
        renamed["name"] = name.clone().into();
        std::fs::write(copy.join("plugin.json"), renamed.to_string()).unwrap();
        listed.push(serde_json::json!({
            "name": name, "source": format!("./plugins/{name}"), "description": "GSD.",
        }));
        plugins.push((name, copy));
    }
    let market = scratch.join("market");
    let marketplace = serde_json::json!({ "name": "studio", "plugins": listed });
    uze_testkit::marketplace::stage(&market, &marketplace.to_string(), &plugins);
    let app = UzeApplication::new(uze_core::UzeHome::at(scratch.join("uze")), Vec::new());
    app.marketplace()
        .add(&market.display().to_string())
        .unwrap();
    app.marketplace().link("studio", &market).unwrap();
    let project = uze_application::project_root_of(checkout);
    let identities = plugins
        .iter()
        .map(|(name, _)| {
            app.marketplace()
                .install_plugin(&format!("{name}@studio"), &uze_application::AlwaysTrust)
                .unwrap();
            let identity = ExtensionIdentity {
                marketplace: "studio".to_owned(),
                plugin: name.clone(),
                id: "gsd-band".to_owned(),
            };
            app.extensions()
                .consent(&identity, Asker::Operator)
                .unwrap();
            app.extensions()
                .enable(&identity, &project, Asker::Operator)
                .unwrap();
            identity
        })
        .collect();
    (app, identities)
}

fn question(identity: &ExtensionIdentity, checkout: &Path, asking: Asking) -> Question {
    Question {
        identity: identity.clone(),
        declaration: "state".to_owned(),
        asking,
        input: serde_json::json!({}),
        checkout: Some(checkout.to_path_buf()),
    }
}

/// One storm: every extension's lane on the pool, its first band's
/// question refreshed and the four others' answered from it.
fn storm(
    app: &UzeApplication,
    runner: &ExtensionRunner,
    identities: &[ExtensionIdentity],
    checkout: &Path,
) -> Duration {
    let lanes = Arc::new(Mutex::new(identities.to_vec()));
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..POOL {
            let lanes = Arc::clone(&lanes);
            scope.spawn(move || {
                let extensions = app.extensions();
                loop {
                    let Some(identity) = lanes.lock().unwrap().pop() else {
                        return;
                    };
                    for agent in 0..AGENTS {
                        let asking = if agent == 0 {
                            Asking::Refresh
                        } else {
                            Asking::Source
                        };
                        let answer =
                            runner.answer(&extensions, &question(&identity, checkout, asking));
                        assert!(matches!(answer, Answer::Answered { .. }), "{answer:?}");
                    }
                }
            });
        }
    });
    started.elapsed()
}

#[test]
fn a_turn_finishing_on_five_agents_costs_fifteen_shared_runs_under_fifty_ms() {
    let scratch = uze_testkit::temp::scratch("extension-storm");
    let checkout = checkout("extension-storm");
    let (app, identities) = fifteen_bands(&scratch, &checkout);
    let runner = ExtensionRunner::new();
    // The session has answered each before: its scripts are compiled.
    storm(&app, &runner, &identities, &checkout);
    let before = runner.runs();
    let storms: Vec<Duration> = (0..3)
        .map(|_| storm(&app, &runner, &identities, &checkout))
        .collect();
    assert_eq!(
        runner.runs() - before,
        3 * EXTENSIONS as u64,
        "five agents of one checkout share each extension's run"
    );
    let best = storms.iter().min().copied().unwrap_or_default();
    let ceiling = if !MEASURED_ON.contains(&uze_platform::target::OS) {
        UNMEASURED
    } else if cfg!(debug_assertions) {
        TARGET * UNOPTIMIZED
    } else {
        TARGET
    };
    assert!(
        best < ceiling,
        "the fastest of {storms:?} is over {ceiling:?}"
    );
    let _ = std::fs::remove_dir_all(&scratch);
    let _ = std::fs::remove_dir_all(&checkout);
}
