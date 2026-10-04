use super::*;
use uze_testkit::git::Repository;

const TARGET: &str = "main";

#[test]
fn an_exclude_file_that_is_not_text_keeps_every_byte_it_had() {
    let repository = repository("exclude-not-text");
    let primary = repository.root();
    let exclude = primary.join(".git/info/exclude");
    fs::create_dir_all(exclude.parent().unwrap()).unwrap();
    let not_text = b"secrets/\n\xff\xfe\n";
    fs::write(&exclude, not_text).unwrap();

    exclude_isolation_directory(primary).unwrap();

    let written = fs::read(&exclude).unwrap();
    assert!(written.starts_with(not_text));
    assert!(written.ends_with(format!("/{WORKTREES_DIRECTORY}/\n").as_bytes()));
}

/// A repository whose `.gitignore` already ignores `target/`, the way a
/// Rust project does — the artifact reuse exists to preserve.
fn repository(label: &str) -> Repository {
    let repository = Repository::new(label);
    repository.commit_file(".gitignore", "target/\n");
    repository
}

fn task(label: &str) -> Agent {
    let mut agent = Agent::isolated(
        "claude",
        Some(label),
        Base::Ref(TARGET.into()),
        String::new(),
        TARGET.into(),
    );
    agent.label = label.to_owned();
    agent
}

/// The branch half of an agent these tests built isolated.
fn isolation(agent: &mut Agent) -> &mut Isolation {
    agent.isolation_mut().expect("the agent was built isolated")
}

/// Acquires a slot for a fresh task and records it as occupied.
fn launch(repository: &Repository, store: &mut AgentStore, label: &str) -> (Agent, Acquired) {
    let primary = repository.root();
    let mut agent = task(label);
    let base = tip_of(primary, TARGET);
    isolation(&mut agent).base_commit = base.clone();
    let acquired = acquire(
        primary,
        store,
        isolation(&mut agent),
        &base,
        None,
        &nobody(),
    )
    .unwrap();
    isolation(&mut agent).checkout = Some(acquired.id.clone());
    store.upsert(agent.clone());
    (agent, acquired)
}

fn nobody() -> Presence {
    Presence::Known(Vec::new())
}

/// A pool that keeps no free slot, so every free one is collectable.
fn keep_nothing() -> Pool {
    Pool {
        spare: 0,
        idle: Duration::ZERO,
    }
}

fn set_state(store: &mut AgentStore, id: &AgentId, state: WorkState) {
    store.get_mut(id).unwrap().state = state;
}

/// The whole reason reuse ever happens: an agent that closed without
/// delivering anything must not keep its slot.
#[test]
fn an_agent_that_left_an_empty_checkout_frees_its_slot_for_the_next() {
    let repository = repository("slots-release-empty");
    let mut store = AgentStore::default();

    let (first, slot) = launch(&repository, &mut store, "first");
    let target = TARGET.to_owned();
    let state = release(
        repository.root(),
        store.get_mut(&first.id).unwrap(),
        &target,
    );
    assert_eq!(state, SlotState::Free);
    assert_eq!(
        store.get(&first.id).unwrap().state,
        WorkState::Closed,
        "it ended with nothing; nothing of it reached the target"
    );
    assert_eq!(
        slots(repository.root(), &store, &nobody())[0].state,
        SlotState::Free,
        "nothing is in front of it and it holds nothing"
    );

    let (_, reused) = launch(&repository, &mut store, "second");
    assert!(!reused.created, "the freed slot is taken, not a new one");
    assert_eq!(reused.path, slot.path);
}

#[test]
fn an_agent_that_left_work_behind_parks_its_slot_instead_of_freeing_it() {
    let repository = repository("slots-release-work");
    let mut store = AgentStore::default();

    let (committed, slot) = launch(&repository, &mut store, "committed");
    fs::write(slot.path.join("feature.rs"), b"fn f() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "undelivered"]);
    assert_eq!(
        release(
            repository.root(),
            store.get_mut(&committed.id).unwrap(),
            TARGET,
        ),
        SlotState::Parked
    );

    let (dirty, other) = launch(&repository, &mut store, "dirty");
    fs::write(other.path.join("draft.rs"), b"unsaved").unwrap();
    assert_eq!(
        release(repository.root(), store.get_mut(&dirty.id).unwrap(), TARGET,),
        SlotState::Parked
    );

    let (_, third) = launch(&repository, &mut store, "third");
    assert!(
        third.created,
        "a parked slot is never offered to a new agent"
    );
    assert!(
        other.path.join("draft.rs").is_file(),
        "every file of a parked checkout is preserved"
    );
}

/// The forge most of these projects deliver to squashes what it
/// merges, so the branch's own commits never appear in the target.
/// Read by reachability, such a slot is parked for as long as the
/// repository lives and the pool never has a free slot in it again.
#[test]
fn a_squash_merged_branch_frees_its_slot_and_is_pruned() {
    let repository = repository("slots-squash");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (delivered, slot) = launch(&repository, &mut store, "squashed");
    fs::write(slot.path.join("feature.rs"), b"fn f() {}").unwrap();
    fs::write(slot.path.join("feature_test.rs"), b"fn t() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "the feature"]);
    fs::write(slot.path.join("feature.rs"), b"fn f() -> u8 { 1 }").unwrap();
    repository.git_in(&slot.path, &["commit", "-qam", "and its fix"]);
    // What a forge's squash button leaves behind: one commit of the
    // target's own, carrying the branch's whole diff.
    repository.git(&["merge", "--squash", &delivered.isolation().unwrap().branch]);
    repository.git(&["commit", "-qm", "the feature (#7)"]);
    assert!(
        commits_ahead(primary, TARGET, &delivered.isolation().unwrap().branch) > 0,
        "none of the branch's commits is reachable from the target"
    );

    assert_eq!(
        release(
            repository.root(),
            store.get_mut(&delivered.id).unwrap(),
            TARGET,
        ),
        SlotState::Free,
        "its work is in the target, under the forge's own commit"
    );
    let (_, reused) = launch(&repository, &mut store, "next");
    assert!(!reused.created, "the freed slot is taken, not a new one");
    assert_eq!(reused.path, slot.path);
    assert_eq!(
        prune_integrated_branches(primary, &store, TARGET),
        vec![delivered.isolation().unwrap().branch.clone()],
        "and the branch it left behind is safe to remove"
    );
}

/// The other button: every commit kept, each under a new identity.
#[test]
fn a_rebase_merged_branch_frees_its_slot() {
    let repository = repository("slots-rebase-merge");
    let mut store = AgentStore::default();

    let (delivered, slot) = launch(&repository, &mut store, "rebased");
    fs::write(slot.path.join("feature.rs"), b"fn f() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "the feature"]);
    // The target moves first, so replaying the commit gives it a new
    // identity — exactly what a rebase merge does.
    repository.commit_file("unrelated.rs", "");
    repository.git(&["cherry-pick", &delivered.isolation().unwrap().branch]);

    assert_eq!(
        release(
            repository.root(),
            store.get_mut(&delivered.id).unwrap(),
            TARGET,
        ),
        SlotState::Free,
        "the same patch is in the target under another commit"
    );
}

#[test]
fn a_free_slot_is_reused_and_ignored_artifacts_survive() {
    let repository = repository("slots-reuse");
    let mut store = AgentStore::default();

    let (first, slot) = launch(&repository, &mut store, "first");
    assert!(slot.created);
    fs::create_dir_all(slot.path.join("target")).unwrap();
    fs::write(slot.path.join("target").join("cache"), b"warm").unwrap();
    fs::write(slot.path.join("feature.rs"), b"fn f() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "feature"]);
    repository.git(&["merge", "--ff-only", &first.isolation().unwrap().branch]);
    set_state(&mut store, &first.id, WorkState::Integrated);

    let (second, reused) = launch(&repository, &mut store, "second");
    assert!(
        !reused.created,
        "the free slot is taken before a directory is created"
    );
    assert_eq!(reused.path, slot.path);
    assert_eq!(
        repository.branch_of(&reused.path),
        second.isolation().unwrap().branch
    );
    assert_eq!(
        fs::read(reused.path.join("target").join("cache")).unwrap(),
        b"warm",
        "ignored artifacts are the point of reuse"
    );
    assert!(
        reused.path.join("feature.rs").exists(),
        "delivered work is in the base"
    );
    assert!(!is_dirty(&reused.path));
}

#[test]
fn a_previous_tasks_edits_never_reach_the_next() {
    let repository = repository("slots-no-leak");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (first, slot) = launch(&repository, &mut store, "first");
    fs::write(slot.path.join("only-on-branch.rs"), b"").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "kept on the branch"]);
    // Declared done by the operator without reaching the target — handoff.
    set_state(&mut store, &first.id, WorkState::Integrated);

    let (_, reused) = launch(&repository, &mut store, "second");
    assert_eq!(reused.path, slot.path);
    assert!(
        !reused.path.join("only-on-branch.rs").exists(),
        "the tree is at the base, not at the previous branch"
    );
    assert!(
        commits_ahead(primary, TARGET, &first.isolation().unwrap().branch) == 1,
        "the previous branch keeps its commit"
    );
}

#[test]
fn a_checkout_holding_work_is_parked_with_every_file_preserved() {
    let repository = repository("slots-park");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (_, slot) = launch(&repository, &mut store, "abandoned");
    fs::write(slot.path.join("half-done.rs"), b"unfinished").unwrap();
    // The agent is gone and nobody recorded the task: the shape of a
    // crash, or of a checkout from before any of this was recorded.
    let mut forgotten = AgentStore::default();

    let report = reconcile(primary, &mut forgotten, TARGET);
    assert_eq!(report.adopted.len(), 1);
    let adopted = forgotten.get(&report.adopted[0]).unwrap();
    assert_eq!(adopted.state, WorkState::Parked);
    assert_eq!(adopted.isolation().unwrap().checkout, Some(slot.id.clone()));
    assert_eq!(
        fs::read(slot.path.join("half-done.rs")).unwrap(),
        b"unfinished"
    );

    let (_, fresh) = launch(&repository, &mut forgotten, "next");
    assert!(fresh.created, "a parked slot is never offered");
    assert_ne!(fresh.path, slot.path);
    assert_eq!(
        fs::read(slot.path.join("half-done.rs")).unwrap(),
        b"unfinished"
    );
}

#[test]
fn an_unintegrated_branch_outlives_its_directory() {
    let repository = repository("slots-idle");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (first, slot) = launch(&repository, &mut store, "handed-off");
    fs::write(slot.path.join("work.rs"), b"").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "work"]);
    set_state(&mut store, &first.id, WorkState::Integrated);
    assert_eq!(slots(primary, &store, &nobody())[0].state, SlotState::Free);

    let removed = trim_free_slots(primary, &store, keep_nothing(), &nobody());
    assert_eq!(removed, vec![slot.id]);
    assert!(!slot.path.exists());
    assert!(
        branch_exists(primary, &first.isolation().unwrap().branch),
        "the branch is never the directory's cost"
    );
    assert_eq!(
        commits_ahead(primary, TARGET, &first.isolation().unwrap().branch),
        1
    );
}

#[test]
fn commits_made_on_a_detached_head_park_the_slot_instead_of_freeing_it() {
    let repository = repository("slots-detached-commits");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "detached");
    repository.git_in(&slot.path, &["checkout", "--quiet", "--detach"]);
    fs::write(slot.path.join("work.rs"), b"").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "only here"]);
    let mut forgotten = AgentStore::default();
    reconcile(primary, &mut forgotten, TARGET);

    assert_eq!(
        slots(primary, &forgotten, &nobody())[0].state,
        SlotState::Parked
    );
    assert!(trim_free_slots(primary, &forgotten, keep_nothing(), &nobody()).is_empty());
    assert!(slot.path.join("work.rs").exists());
}

#[test]
fn a_detached_head_on_a_branched_commit_leaves_the_slot_free() {
    let repository = repository("slots-detached-clean");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "detached");
    repository.git_in(&slot.path, &["checkout", "--quiet", "--detach"]);
    let mut forgotten = AgentStore::default();
    reconcile(primary, &mut forgotten, TARGET);

    assert_eq!(
        slots(primary, &forgotten, &nobody())[0].state,
        SlotState::Free
    );
}

#[test]
fn a_parked_slot_is_never_removed_for_being_idle() {
    let repository = repository("slots-idle-parked");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "abandoned");
    fs::write(slot.path.join("dirty"), b"").unwrap();
    let mut forgotten = AgentStore::default();
    reconcile(primary, &mut forgotten, TARGET);
    assert!(trim_free_slots(primary, &forgotten, keep_nothing(), &nobody()).is_empty());
    assert!(slot.path.join("dirty").exists());
}

#[test]
fn an_integrated_branch_is_pruned_and_an_unintegrated_one_is_not() {
    let repository = repository("slots-prune-branches");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (delivered, slot) = launch(&repository, &mut store, "delivered");
    fs::write(slot.path.join("a.rs"), b"").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "a"]);
    repository.git(&["merge", "--ff-only", &delivered.isolation().unwrap().branch]);
    set_state(&mut store, &delivered.id, WorkState::Integrated);

    let (kept, second) = launch(&repository, &mut store, "kept");
    fs::write(second.path.join("b.rs"), b"").unwrap();
    repository.git_in(&second.path, &["add", "."]);
    repository.git_in(&second.path, &["commit", "-qm", "b"]);
    set_state(&mut store, &kept.id, WorkState::Integrated);
    // The reused slot moved on to a third branch, so neither of the
    // two above is checked out anywhere.
    let (_, _) = launch(&repository, &mut store, "third");
    assert_eq!(second.path, slot.path);

    let removed = prune_integrated_branches(primary, &store, TARGET);
    assert_eq!(removed, vec![delivered.isolation().unwrap().branch.clone()]);
    assert!(!branch_exists(
        primary,
        &delivered.isolation().unwrap().branch
    ));
    assert!(
        branch_exists(primary, &kept.isolation().unwrap().branch),
        "commits the target lacks are never deleted"
    );
}

/// `worktrees.target` is authored and committed, so a clone that never
/// fetched it — a single-branch clone, a gitflow `develop`, a typo —
/// hands every predicate here a name Git cannot resolve. Answered
/// "nothing ahead", that made every branch collectable and every slot
/// free: `branch -D` and `reset --hard` over an agent's committed work.
#[test]
fn a_target_this_clone_does_not_have_collects_nothing_and_frees_no_slot() {
    let repository = repository("slots-missing-target");
    let primary = repository.root();
    let mut store = AgentStore::default();
    const MISSING: &str = "develop";

    let (parked, slot) = launch(&repository, &mut store, "parked");
    fs::write(slot.path.join("a.rs"), b"").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "a"]);
    set_state(&mut store, &parked.id, WorkState::Parked);
    for task in store.agents.iter_mut() {
        task.isolation_mut().unwrap().target = MISSING.to_owned();
    }
    assert!(tip_of(primary, MISSING).is_empty(), "the target is absent");

    let collected = collect(primary, &store, MISSING, keep_nothing(), &nobody());
    assert_eq!(collected, Collected::default(), "nothing may be removed");
    assert!(branch_exists(primary, &parked.isolation().unwrap().branch));
    assert!(slot.path.join("a.rs").is_file());
    assert_eq!(
        slots(primary, &store, &nobody())[0].state,
        SlotState::Parked,
        "a slot measured against a target that does not resolve holds work"
    );
}

#[test]
fn a_new_directory_appears_only_when_none_is_free_and_the_cap_holds() {
    let repository = repository("slots-cap");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (first, a) = launch(&repository, &mut store, "a");
    let (_, b) = launch(&repository, &mut store, "b");
    assert!(a.created && b.created && a.path != b.path);

    let blocked = task("c");
    let error = acquire(
        primary,
        &store,
        blocked.isolation().unwrap(),
        &tip_of(primary, TARGET),
        Some(2),
        &nobody(),
    )
    .unwrap_err();
    assert!(
        matches!(error, AcquireError::CapReached { cap: 2 }),
        "{error}"
    );

    set_state(&mut store, &first.id, WorkState::Integrated);
    let reused = acquire(
        primary,
        &store,
        blocked.isolation().unwrap(),
        &tip_of(primary, TARGET),
        Some(2),
        &nobody(),
    )
    .unwrap();
    assert!(!reused.created);
    assert_eq!(reused.path, a.path);
}

#[test]
fn prune_runs_after_adoption_and_an_orphaned_task_keeps_its_branch() {
    let repository = repository("slots-prune-order");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (task, slot) = launch(&repository, &mut store, "vanished");
    fs::write(slot.path.join("c.rs"), b"").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "c"]);
    fs::remove_dir_all(&slot.path).unwrap();
    assert!(
        repository
            .git(&["worktree", "list", "--porcelain"])
            .contains(&task.isolation().unwrap().branch),
        "the registry entry is still there before reconciliation"
    );

    let report = reconcile(primary, &mut store, TARGET);
    assert_eq!(report.orphaned, vec![task.id.clone()]);
    let task = store.get(&task.id).unwrap();
    assert_eq!(task.state, WorkState::Parked);
    assert_eq!(task.isolation().unwrap().checkout, None);
    assert!(branch_exists(primary, &task.isolation().unwrap().branch));
    assert!(
        !repository
            .git(&["worktree", "list", "--porcelain"])
            .contains(&task.isolation().unwrap().branch),
        "the stale entry is pruned once every directory was looked at"
    );
}

/// The agent whose work was delivered mid-session keeps going, and its
/// next commit is work like any other. Reconciliation is where a slot
/// is acquired from, so a task left `Integrated` here reads as a free
/// directory — offered to the next agent while this one is still
/// writing in it.
#[test]
fn an_agent_that_commits_after_its_delivery_is_live_again() {
    let repository = repository("slots-revive");
    let primary = repository.root();
    let mut store = AgentStore::default();

    let (first, slot) = launch(&repository, &mut store, "first");
    fs::write(slot.path.join("delivered.rs"), b"fn a() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "delivered"]);
    repository.git(&["merge", "--ff-only", &first.isolation().unwrap().branch]);
    set_state(&mut store, &first.id, WorkState::Integrated);

    // Nothing new yet: delivered is the truth, and the slot is free
    // for the next agent.
    let report = reconcile(primary, &mut store, TARGET);
    assert!(report.revived.is_empty(), "{report:?}");
    assert_eq!(store.get(&first.id).unwrap().state, WorkState::Integrated);
    assert_eq!(slots(primary, &store, &nobody())[0].state, SlotState::Free);

    fs::write(slot.path.join("after.rs"), b"fn b() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "after the delivery"]);

    let report = reconcile(primary, &mut store, TARGET);
    assert_eq!(report.revived, vec![first.id.clone()]);
    let task = store.get(&first.id).unwrap();
    assert_eq!(task.state, WorkState::Running, "live again, and re-read");
    assert_eq!(
        slots(primary, &store, &nobody())[0].state,
        SlotState::Occupied {
            task: first.id.clone()
        },
        "and its slot is not free for another agent while it works"
    );
}

/// The agent that delivered a task is still in its checkout until its
/// tab closes. The task record says done, and read alone it would hand
/// the directory to the next agent under the feet of the last — which
/// is exactly what the panes are consulted for.
#[test]
fn a_slot_a_pane_still_sits_in_is_neither_reused_nor_removed_after_delivery() {
    let repository = repository("slots-pane-inside");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (first, slot) = launch(&repository, &mut store, "first");
    set_state(&mut store, &first.id, WorkState::Integrated);
    let inside = vec![slot.path.join("src")];

    assert_eq!(
        slots(primary, &store, &nobody())[0].state,
        SlotState::Free,
        "the record alone reads as free"
    );
    assert_eq!(
        slots(primary, &store, &Presence::Known(inside.clone()))[0].state,
        SlotState::Occupied {
            task: first.id.clone()
        },
        "a pane inside keeps it the last agent's"
    );

    let second = task("second");
    let acquired = acquire(
        primary,
        &store,
        second.isolation().unwrap(),
        &tip_of(primary, TARGET),
        None,
        &Presence::Known(inside.clone()),
    )
    .unwrap();
    assert!(
        acquired.created,
        "the next agent gets a directory of its own"
    );
    assert_ne!(acquired.path, slot.path);
    // The new directory belongs to no recorded task and reads as idle,
    // so an immediate sweep may take it; the pane's own must survive.
    let removed = trim_free_slots(
        primary,
        &store,
        keep_nothing(),
        &Presence::Known(inside.clone()),
    );
    assert!(
        !removed.contains(&slot.id) && slot.path.is_dir(),
        "the directory is not swept out from under the pane: {removed:?}"
    );
}

/// A checkout removed outside UZE takes the uncommitted work with it
/// and nothing else: the branch keeps every commit, and resuming the
/// task puts a slot back under that branch exactly where it stands.
#[test]
fn a_task_whose_checkout_was_removed_resumes_on_its_own_branch() {
    let repository = repository("slots-resume");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (mut task, slot) = launch(&repository, &mut store, "first");
    fs::write(slot.path.join("kept.rs"), b"fn kept() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "kept"]);
    fs::write(slot.path.join("lost.rs"), b"fn lost() {}").unwrap();
    fs::remove_dir_all(&slot.path).unwrap();

    let report = reconcile(primary, &mut store, TARGET);
    assert_eq!(report.orphaned, vec![task.id.clone()]);
    task = store.get(&task.id).unwrap().clone();
    assert_eq!(task.state, WorkState::Parked, "a commit the target lacks");
    assert_eq!(task.isolation().unwrap().checkout, None);

    let resumed = resume(primary, &store, task.isolation().unwrap(), None, &nobody()).unwrap();
    assert_eq!(resumed.branch, task.isolation().unwrap().branch);
    assert_eq!(
        current_branch(&resumed.path).as_deref(),
        Some(task.isolation().unwrap().branch.as_str())
    );
    assert!(resumed.path.join("kept.rs").is_file(), "the commit is back");
    assert!(
        !resumed.path.join("lost.rs").exists(),
        "the uncommitted file is not"
    );
    assert_eq!(
        commits_ahead(primary, TARGET, &task.isolation().unwrap().branch),
        1,
        "nothing was reset past the branch's own tip"
    );
}

#[test]
fn a_legacy_checkout_is_adopted_under_its_branch_name() {
    let repository = repository("slots-legacy");
    let primary = repository.root();
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "agent/agent-2",
        ".worktrees/agent-2",
        "HEAD",
    ]);
    let mut store = AgentStore::default();
    let report = reconcile(primary, &mut store, TARGET);
    assert_eq!(report.adopted.len(), 1);
    let adopted = store.get(&report.adopted[0]).unwrap();
    assert_eq!(adopted.label, "agent-2");
    assert_eq!(
        adopted.isolation().unwrap().branch,
        "agent/agent-2",
        "no branch is renamed: it may have been pushed"
    );
    assert_eq!(
        adopted.isolation().unwrap().checkout,
        Some(CheckoutId::adopted("agent-2"))
    );
    assert_eq!(
        adopted.state,
        WorkState::Closed,
        "clean and nothing ahead: free to reuse, and no delivery to claim"
    );
    assert_eq!(slots(primary, &store, &nobody())[0].state, SlotState::Free);
}

/// Adoption takes the branch as Git has it. A slot UZE made, found on a
/// branch somebody named after its task's record was lost, keeps that
/// name — final, like any chosen one — and its label reads from it
/// rather than from the slot's identifier.
#[test]
fn a_checkout_on_a_named_branch_is_adopted_under_its_name() {
    let repository = repository("slots-legacy-named");
    let primary = repository.root();
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feat/keymap",
        ".worktrees/k3y4ap",
        "HEAD",
    ]);
    let slot = primary.join(".worktrees/k3y4ap");
    record::write(&slot, &CheckoutRecord::made_at(&slot)).unwrap();
    let mut store = AgentStore::default();
    let report = reconcile(primary, &mut store, TARGET);
    assert_eq!(report.adopted.len(), 1);
    let adopted = store.get(&report.adopted[0]).unwrap();
    assert_eq!(
        adopted.isolation().unwrap().branch,
        "feat/keymap",
        "no branch is renamed"
    );
    assert!(adopted.is_named(), "a name nobody generated stays final");
    assert_eq!(
        adopted.label, "keymap",
        "the label is the name, not the slot"
    );
    assert_eq!(
        adopted.isolation().unwrap().checkout,
        Some(CheckoutId::adopted("k3y4ap"))
    );
}

/// The squash probe is written only to be compared, and evaluation asks
/// the question of every parked task on every pass. Dated by the clock,
/// each answer left a new object behind; pinned, asking again writes
/// nothing the first question did not.
#[test]
fn asking_twice_whether_a_squash_landed_writes_nothing_new() {
    let repository = repository("slots-squash-probe");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (task, slot) = launch(&repository, &mut store, "probed");
    // Two commits on one file, so no single commit's patch is in the
    // target and only the squash probe can answer.
    fs::write(slot.path.join("feature.rs"), b"fn f() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "the feature"]);
    fs::write(slot.path.join("feature.rs"), b"fn f() -> u8 { 1 }").unwrap();
    repository.git_in(&slot.path, &["commit", "-qam", "and its fix"]);
    repository.git(&["merge", "--squash", &task.isolation().unwrap().branch]);
    repository.git(&["commit", "-qm", "the feature (#7)"]);
    assert!(is_integrated(
        primary,
        TARGET,
        &task.isolation().unwrap().branch
    ));
    let before = repository.git(&["count-objects"]);

    // Past the clock's resolution: a probe dated by it would differ.
    std::thread::sleep(Duration::from_millis(1100));
    assert!(is_integrated(
        primary,
        TARGET,
        &task.isolation().unwrap().branch
    ));

    assert_eq!(
        repository.git(&["count-objects"]),
        before,
        "the second probe is the first one"
    );
}

/// The answer is remembered per pair of commits, so a branch asked
/// about before it landed is answered again once the target moves —
/// by a squash, which only patch identity can see.
#[test]
fn a_branch_asked_about_before_it_landed_is_answered_as_landed() {
    let repository = repository("slots-landed-later");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (task, slot) = launch(&repository, &mut store, "later");
    let branch = task.isolation().unwrap().branch.clone();
    fs::write(slot.path.join("feature.rs"), b"fn f() {}").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "the feature"]);
    fs::write(slot.path.join("feature.rs"), b"fn f() -> u8 { 1 }").unwrap();
    repository.git_in(&slot.path, &["commit", "-qam", "and its fix"]);
    assert!(!is_integrated(primary, TARGET, &branch));

    repository.git(&["merge", "--squash", &branch]);
    repository.git(&["commit", "-qm", "the feature (#8)"]);

    assert!(is_integrated(primary, TARGET, &branch));
}

/// Falling closed is not an answer to keep: a target the clone lacks
/// is asked about again, and answered once it appears.
#[test]
fn a_target_that_appears_later_is_asked_about_again() {
    let repository = repository("slots-target-later");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (task, _slot) = launch(&repository, &mut store, "early");
    let branch = task.isolation().unwrap().branch.clone();
    assert!(!is_integrated(primary, "release", &branch));

    repository.git(&["branch", "release", &branch]);

    assert!(is_integrated(primary, "release", &branch));
}

/// A worktree UZE did not create is none of its business. A harness
/// isolating on its own — Claude Code keeps its worktrees under the
/// repository's `.claude/worktrees` — is never adopted as a slot, never
/// offered to an agent, never swept as idle, and its branch is never
/// pruned, however clean it is and however little it holds.
#[test]
fn a_worktree_uze_did_not_create_is_never_its_to_touch() {
    let repository = repository("slots-foreign");
    let primary = repository.root();
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "worktree-agent-x",
        ".claude/worktrees/agent-x",
        "HEAD",
    ]);
    let foreign = primary.join(".claude/worktrees/agent-x");
    let mut store = AgentStore::default();

    let report = reconcile(primary, &mut store, TARGET);
    assert!(report.adopted.is_empty(), "never adopted");
    assert!(slots(primary, &store, &nobody()).is_empty(), "never a slot");

    let (_, placed) = launch(&repository, &mut store, "next");
    assert!(placed.created, "never offered to an agent");

    let collected = collect(primary, &store, TARGET, keep_nothing(), &nobody());
    assert!(foreign.is_dir(), "never swept as idle");
    assert!(
        !collected
            .branches
            .iter()
            .any(|branch| branch == "worktree-agent-x"),
        "its branch is never pruned"
    );
    assert!(branch_exists(primary, "worktree-agent-x"));
}

/// The isolation directory is shared: a person, or an agent giving its
/// subagents checkouts of their own, adds worktrees there by hand. One
/// that was just added is clean and holds nothing the target lacks,
/// which is exactly what a free slot looks like, so its name is what
/// keeps the next agent from resetting it out from under its owner.
#[test]
fn a_checkout_added_by_hand_beside_the_slots_is_never_taken_as_one() {
    let repository = repository("slots-hand-made");
    let primary = repository.root();
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "agent/hardening-integrations",
        &format!("{WORKTREES_DIRECTORY}/hardening-integrations"),
        "HEAD",
    ]);
    let hand_made = primary
        .join(WORKTREES_DIRECTORY)
        .join("hardening-integrations");
    let mut store = AgentStore::default();

    let report = reconcile(primary, &mut store, TARGET);
    assert!(report.adopted.is_empty(), "never adopted");
    assert!(slots(primary, &store, &nobody()).is_empty(), "never a slot");

    let (_, placed) = launch(&repository, &mut store, "next");
    assert!(placed.created, "never offered to an agent");
    assert_ne!(placed.path, hand_made);
    assert_eq!(
        repository
            .git_in(&hand_made, &["branch", "--show-current"])
            .trim(),
        "agent/hardening-integrations",
        "its branch is where its owner left it"
    );

    collect(primary, &store, TARGET, keep_nothing(), &nobody());
    assert!(hand_made.is_dir(), "never swept as idle");
}

#[test]
fn only_the_numbered_names_before_slots_are_legacy() {
    assert!(CheckoutId::adopted("agent-2").is_legacy());
    for chosen in ["hardening-integrations", "abc123", "agent-", "agent-x"] {
        assert!(!CheckoutId::adopted(chosen).is_legacy(), "{chosen}");
    }
}

#[test]
fn the_isolation_directory_is_excluded_without_touching_the_primary_tree() {
    let repository = repository("slots-exclude");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let gitignore_before = fs::read_to_string(primary.join(".gitignore")).unwrap();
    launch(&repository, &mut store, "a");
    launch(&repository, &mut store, "b");
    assert!(
        repository.git(&["status", "--porcelain"]).is_empty(),
        "the primary is untouched"
    );
    assert_eq!(
        fs::read_to_string(primary.join(".gitignore")).unwrap(),
        gitignore_before
    );
    let exclude = fs::read_to_string(primary.join(".git/info/exclude")).unwrap();
    assert_eq!(
        exclude.matches(WORKTREES_DIRECTORY).count(),
        1,
        "idempotent: {exclude}"
    );
}

#[test]
fn a_repository_without_a_commit_cannot_host_a_slot() {
    let repository = Repository::empty("slots-unborn");
    let store = AgentStore::default();
    let error = acquire(
        repository.root(),
        &store,
        task("x").isolation().unwrap(),
        "HEAD",
        None,
        &nobody(),
    )
    .unwrap_err();
    assert!(matches!(error, AcquireError::Git(_)), "{error}");
}

#[test]
fn a_linked_file_is_a_symlink_and_a_missing_target_only_warns() {
    let repository = repository("slots-materialize");
    let primary = repository.root();
    fs::write(primary.join(".env"), "SECRET=1\n").unwrap();
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "materialize");

    let warnings = materialize(
        primary,
        &slot.path,
        &[PathBuf::from(".env"), PathBuf::from(".env.local")],
        &[],
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains(".env.local"));
    let linked = slot.path.join(".env");
    assert!(
        fs::symlink_metadata(&linked)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&linked).unwrap(), "SECRET=1\n");
    assert!(
        materialize(primary, &slot.path, &[PathBuf::from(".env")], &[]).is_empty(),
        "idempotent"
    );
}

#[test]
fn a_failing_setup_warns_with_its_last_line_and_a_passing_one_is_silent() {
    let repository = repository("slots-setup");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "setup");
    let warnings = materialize(
        primary,
        &slot.path,
        &[],
        &[uze_core::shell::ShellCommand::spelled(
            "echo preparing; echo 'no such tool: pnpm' >&2; exit 3",
            "'preparing'; [Console]::Error.WriteLine('no such tool: pnpm'); exit 3",
        )],
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("no such tool: pnpm"), "{warnings:?}");
    let prepare = uze_core::shell::ShellCommand::spelled(
        "touch prepared",
        "New-Item prepared -ItemType File | Out-Null",
    );
    assert!(materialize(primary, &slot.path, &[], &[prepare]).is_empty());
    assert!(
        slot.path.join("prepared").exists(),
        "setup runs in the checkout"
    );
}

/// The counts a pull and a push would move, read against whatever the
/// branch tracks — and nothing at all when it tracks nothing.
#[test]
fn upstream_divergence_counts_both_directions_and_needs_an_upstream() {
    let repository = repository("upstream-divergence");
    let primary = repository.root();
    assert_eq!(upstream_divergence(primary), None, "no upstream, no answer");

    repository.git(&["branch", "upstream"]);
    repository.git(&["branch", "--set-upstream-to=upstream"]);
    assert_eq!(
        upstream_divergence(primary),
        Some(UpstreamDivergence::default()),
        "in sync"
    );

    repository.commit_file("mine.txt", "pushable\n");
    repository.git(&["checkout", "--quiet", "upstream"]);
    repository.commit_file("theirs.txt", "pullable\n");
    repository.commit_file("more.txt", "pullable too\n");
    repository.git(&["checkout", "--quiet", TARGET]);
    assert_eq!(
        upstream_divergence(primary),
        Some(UpstreamDivergence {
            behind: 2,
            ahead: 1
        })
    );

    repository.git(&["checkout", "--quiet", "--detach"]);
    assert_eq!(
        upstream_divergence(primary),
        None,
        "detached: nothing tracks"
    );
}
