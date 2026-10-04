//! Acceptance A8: runtime shim active — UZE's own shims dir precedes the
//! real executable on PATH, yet internal harness resolution must reach the
//! real binary and never recurse into the shim.

use uze_core::UzeHome;
use uze_testkit::fake_harness::{Action, FakeHarness};
use uze_testkit::temp::TestEnvironment;

use crate::util::uze_bin;

/// A poisoned shim: it records every invocation (so a recursion is provable)
/// and answers with a bogus version. Sits in UZE's own shims dir, ahead of
/// the real-looking fake on PATH — the exact hazard after `uze setup`.
fn poison_shim(env: &TestEnvironment, name: &str, poison_version: &str) -> FakeHarness {
    let shims = UzeHome::at(&env.uze_home).shims_dir();
    FakeHarness::new(&shims, name)
        .version_line(poison_version)
        .build()
}

#[test]
fn runtime_shim_active_internal_calls_resolve_real_executable_without_recursion() {
    let env = TestEnvironment::isolated();

    // Real-looking harness binaries (the "real" side of the boundary).
    for (name, version) in [
        ("claude", "9.9.9 (Real Claude)"),
        ("codex", "codex-cli 9.9.9"),
        ("opencode2", "opencode2 v9.9.9"),
        ("agy", "agy 9.9.9"),
    ] {
        let _ = FakeHarness::new(&env.fake_bin, name)
            .version_line(version)
            .build();
    }
    // Poisoned UZE shims for every vendor, first on PATH.
    let poisoned: Vec<_> = [
        ("claude", "POISON (should never be read) 0.0.1"),
        ("codex", "codex-cli POISON"),
        ("opencode", "opencode2 vPOISON"),
        ("agy", "1.0.0-POISON"),
    ]
    .into_iter()
    .map(|(name, poison)| (name, poison_shim(&env, name, poison)))
    .collect();

    // Run the real uze with PATH = shims : fake_bin : system bins. The
    // shims must stay ahead of everything else (the real hazard).
    let shims = UzeHome::at(&env.uze_home).shims_dir();
    let path = uze_testkit::process::path_with(&[&shims, &env.fake_bin]);
    let output = env
        .command(uze_bin())
        .env("PATH", &path)
        .args(["doctor"])
        .output()
        .expect("uze doctor must run");
    // Fake harnesses leave doctor something to find, and a finding fails
    // the command; what matters here is that it ran and whom it asked.
    assert!(
        output.status.code().is_some_and(|code| code <= 1),
        "doctor did not run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("POISON"),
        "doctor must never read the poisoned shim output, got: {stdout}"
    );
    // The fake "real" binaries must have been invoked: detection went
    // through them, not the shims (doctor's text does not print versions).
    for name in ["claude", "codex", "opencode2", "agy"] {
        let log = env
            .fake_bin
            .join(".invocations")
            .join(format!("{name}.log"));
        assert!(
            std::fs::read_to_string(&log).is_ok(),
            "the real-looking fake {name} was never invoked — doctor fell back to the shim"
        );
    }
    for name in ["Claude Code", "Codex", "OpenCode", "Antigravity"] {
        assert!(
            stdout.contains(name),
            "doctor must detect {name} through the real-looking fake, got: {stdout}"
        );
    }

    // No recursion: none of the shims ever ran.
    for (name, shim) in poisoned {
        assert!(
            shim.invocations().is_empty(),
            "shim {name} was invoked — internal harness resolution recursed into the runtime shim"
        );
    }
}

/// The persistent terminal workspace (`uze-terminal`) recognizes an agent
/// pane by reading `UZE_SHIM_NAME` back out of the launched process's live
/// environment — necessary because a harness is free to overwrite its own
/// `comm` (Claude Code sets its process title to its version string). This
/// exercises the actual dispatch a shim symlink invocation takes
/// (`src/shim.rs::run` → `exec_or_die`), not just the internal detection
/// path the test above covers, and checks the one thing that dispatch must
/// hand the real binary: its own invoked name, in its environment.
#[test]
fn shim_dispatch_stamps_its_own_invoked_name_into_the_real_binarys_environment() {
    let env = TestEnvironment::isolated();
    let shims = UzeHome::at(&env.uze_home).shims_dir();
    std::fs::create_dir_all(&shims).unwrap();
    let shim_entry = shims.join(uze_platform::executable::file_name("claude"));
    uze_platform::fs::symlink(uze_bin(), &shim_entry).unwrap();

    // The "real" claude, further down PATH than the shim entry — dumps its
    // own live environment so the assertion can see exactly what the shim
    // exec'd it with.
    let real_dir = env.root().join("real-bin");
    FakeHarness::new(&real_dir, "claude")
        .on([""], Action::PrintEnvironment)
        .build();

    let path = uze_testkit::process::path_with(&[&shims, &real_dir]);
    let output = env
        .command(&shim_entry)
        .env("PATH", &path)
        .output()
        .expect("shim-launched claude must run");
    assert!(
        output.status.success(),
        "shim-launched claude failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line == "UZE_SHIM_NAME=claude"),
        "the real claude must see UZE_SHIM_NAME=claude in its environment, got: {stdout}"
    );
}
