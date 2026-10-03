//! The authoring scaffold's budget, named by `src/command_performance.rs`
//! for `agent plugin create`.
//!
//! Its own test target for the reason `uze-application`'s `performance.rs`
//! gives: a wall clock inside the crate's lib-test binary measures the
//! path plus every sibling test sharing its thread pool, several of which
//! spawn Git. Cargo runs test targets one after another, so a target
//! holding nothing but this measures what it claims to.

use std::{fs, time::Duration, time::Instant};

use uze_core::authoring::{ScaffoldCapabilities, scaffold_marketplace, scaffold_plugin};

/// A backstop well above file writes plus one manifest read, and well
/// below anything that reaches a network or a harness.
const BUDGET: Duration = Duration::from_millis(250);
const ATTEMPTS: usize = 3;

#[test]
fn authoring_scaffold_meets_the_budget() {
    // Git is isolated: the scaffold's own Git work is not what is measured
    // here, and its identity is asked inside a world, not on the
    // developer's machine.
    let mut environment = uze_testkit::env::scope();
    let configuration = uze_testkit::temp::scratch("authoring-budget-gitconfig").join("gitconfig");
    fs::write(
        &configuration,
        "[user]\n\tname = T\n\temail = t@example.invalid\n",
    )
    .unwrap();
    environment.set("GIT_CONFIG_GLOBAL", &configuration);
    environment.set("GIT_CONFIG_SYSTEM", &configuration);

    let best = (0..ATTEMPTS)
        .map(|attempt| {
            let root = uze_testkit::temp::scratch(&format!("authoring-scaffold-budget-{attempt}"));
            let market = scaffold_marketplace("tools", None, &root.join("market")).unwrap();
            let started = Instant::now();
            scaffold_plugin(&market, "greet", None, &ScaffoldCapabilities::default()).unwrap();
            let elapsed = started.elapsed();
            fs::remove_dir_all(&root).expect("teardown");
            elapsed
        })
        .min()
        .expect("at least one attempt");

    if std::env::var_os("LLVM_PROFILE_FILE").is_some() {
        eprintln!("instrumented for coverage, so the clock is not held to the budget");
        return;
    }
    assert!(
        best < BUDGET,
        "a plugin scaffold is file writes plus one marketplace manifest read: best of {ATTEMPTS} took {best:?}"
    );
}
