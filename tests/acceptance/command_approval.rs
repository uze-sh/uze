//! A project's commands are approved by a person, and only by a person:
//! the verb refuses an agent UZE launched and anything that is not a
//! terminal, and withdrawing an approval takes the record away (ADR-055).

use std::path::{Path, PathBuf};

use uze_core::UzeHome;
use uze_testkit::temp::TestEnvironment;
use uze_workspace::{approval, declaration};

use crate::util::uze_bin;

fn declaring_commands(env: &TestEnvironment) -> PathBuf {
    std::fs::write(
        env.project.join("agents.yaml"),
        "workspace:\n  setup: touch pwned\n  gate: make test\n",
    )
    .unwrap();
    env.project.clone()
}

/// Every approval UZE has recorded, read from the disk rather than asked
/// of UZE.
fn approvals_on_disk(uze_home: &Path) -> Vec<PathBuf> {
    let Ok(projects) = std::fs::read_dir(uze_home.join("state/projects")) else {
        return Vec::new();
    };
    projects
        .filter_map(Result::ok)
        .map(|project| project.path().join("approved-commands.json"))
        .filter(|record| record.is_file())
        .collect()
}

#[test]
fn an_agent_cannot_approve_its_projects_commands() {
    let env = TestEnvironment::isolated();
    let root = declaring_commands(&env);

    let refused = env
        .command(uze_bin())
        .current_dir(&root)
        .env(uze_terminal::launch::AGENT_IDENTITY_VARIABLE, "an-agent")
        .args(["workspace", "allow"])
        .output()
        .expect("uze must run");

    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("an agent cannot approve"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(approvals_on_disk(&env.uze_home).is_empty());
    assert!(!root.join("pwned").exists());
}

#[test]
fn approving_needs_a_person_at_a_terminal() {
    let env = TestEnvironment::isolated();
    let root = declaring_commands(&env);

    let refused = env
        .command(uze_bin())
        .current_dir(&root)
        .env_remove(uze_terminal::launch::AGENT_IDENTITY_VARIABLE)
        .stdin(std::process::Stdio::null())
        .args(["workspace", "allow"])
        .output()
        .expect("uze must run");

    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("a person at a terminal"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(approvals_on_disk(&env.uze_home).is_empty());
}

#[test]
fn revoking_takes_the_recorded_approval_away() {
    let env = TestEnvironment::isolated();
    let root = declaring_commands(&env);
    // Laid down as data: this tier asserts against the machine, and the
    // verb that records an approval needs a person to answer it.
    let home = UzeHome::at(&env.uze_home);
    let policy = declaration::policy(&root).unwrap();
    approval::approve(&home, &root, &approval::ProjectCommands::of(&policy)).unwrap();
    assert_eq!(approvals_on_disk(&env.uze_home).len(), 1);

    let output = env
        .command(uze_bin())
        .current_dir(&root)
        .args(["workspace", "revoke"])
        .output()
        .expect("uze must run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(approvals_on_disk(&env.uze_home).is_empty());
}
