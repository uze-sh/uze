//! `uze agent report` from outside an agent's launch: refused, said on
//! stderr, and never a failed hook.

use std::process::Command;

use uze_testkit::process::{IsolatedHome, NotLaunchedByUze};

#[test]
fn a_report_from_no_agent_takes_nothing_and_still_exits_well() {
    let root = uze_testkit::temp::scratch("cli-agent-report");
    let output = Command::new(env!("CARGO_BIN_EXE_uze"))
        .current_dir(&root)
        .env("UZE_HOME", root.join("uze"))
        .isolated_home(&root)
        .not_launched_by_uze()
        .env("PATH", uze_testkit::process::system_path())
        .args([
            "agent",
            "report",
            "turn.finished",
            r#"{"type":"agent-turn-complete"}"#,
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "a hook must never fail on it");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(said.contains("not an agent UZE launched"), "{said}");
    assert!(
        !root.join("uze/runtime/agents").exists(),
        "nothing was journaled"
    );
    let _ = std::fs::remove_dir_all(root);
}
