use super::*;
use crate::task::{Agent, AgentStore, Base, WorkState};

/// The prefix is no longer the whole answer. A named task's branch left
/// `agent/` behind, and a branch nobody can find is a branch nobody
/// collects — so the store's own names are searched beside the prefix.
#[test]
fn a_named_branch_is_collected_once_its_work_is_in_the_target() {
    let repository = uze_testkit::git::Repository::new("collect-named");
    let primary = repository.root();
    let base = repository.commit_file("seed.rs", "");

    let mut task = Agent::isolated(
        "claude",
        None,
        Base::Ref("main".into()),
        base.clone(),
        "main".into(),
    );
    repository.git(&["branch", &task.isolation().unwrap().branch]);
    repository.git(&[
        "branch",
        "--move",
        &task.isolation().unwrap().branch,
        "fix/named-work",
    ]);
    task.take_name("fix/named-work".to_owned());
    task.state = WorkState::Integrated;
    let mut store = AgentStore::default();
    store.upsert(task);

    let removed = prune_integrated_branches(primary, &store, "main");

    assert_eq!(
        removed,
        vec!["fix/named-work".to_owned()],
        "a branch the store names is collectable even outside the prefix"
    );
    assert!(!branch_exists(primary, "fix/named-work"));
}

/// And a live task's branch is never collected, named or not — the
/// rule that protects work has not moved.
#[test]
fn a_live_named_branch_is_left_alone() {
    let repository = uze_testkit::git::Repository::new("collect-live");
    let primary = repository.root();
    let base = repository.commit_file("seed.rs", "");

    let mut task = Agent::isolated(
        "claude",
        None,
        Base::Ref("main".into()),
        base,
        "main".into(),
    );
    repository.git(&["branch", "fix/still-working"]);
    task.take_name("fix/still-working".to_owned());
    task.state = WorkState::Running;
    let mut store = AgentStore::default();
    store.upsert(task);

    assert!(prune_integrated_branches(primary, &store, "main").is_empty());
    assert!(branch_exists(primary, "fix/still-working"));
}
