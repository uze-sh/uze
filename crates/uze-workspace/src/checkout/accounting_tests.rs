//! Which checkouts are UZE's, what counts as work in one, and how many free
//! ones are kept — against real Git.

use std::time::{Duration, SystemTime};

use uze_testkit::git::Repository;

use super::*;

const TARGET: &str = "main";

fn repository(label: &str) -> Repository {
    let repository = Repository::new(label);
    repository.commit_file(".gitignore", "target/\n");
    repository
}

fn nobody() -> Presence {
    Presence::Known(Vec::new())
}

fn launch(repository: &Repository, store: &mut AgentStore, label: &str) -> (Agent, Acquired) {
    let primary = repository.root();
    let mut agent = Agent::isolated(
        "a-harness",
        Some(label),
        Base::Ref(TARGET.into()),
        tip_of(primary, TARGET),
        TARGET.into(),
    );
    agent.label = label.to_owned();
    let isolation = agent.isolation_mut().unwrap();
    let base = isolation.base_commit.clone();
    let acquired = acquire(primary, store, isolation, &base, None, &nobody()).unwrap();
    isolation.checkout = Some(acquired.id.clone());
    store.upsert(agent.clone());
    (agent, acquired)
}

/// Ends `agent` the way the occupancy sweep does, and records it.
fn end(repository: &Repository, store: &mut AgentStore, agent: &Agent) -> SlotState {
    let mut ended = store.get(&agent.id).unwrap().clone();
    let state = release(repository.root(), &mut ended, TARGET);
    store.upsert(ended);
    state
}

fn record_file(slot: &Path) -> PathBuf {
    let pointer = fs::read_to_string(slot.join(".git")).unwrap();
    let admin = pointer.trim().strip_prefix("gitdir: ").unwrap();
    slot.join(admin).join(crate::worktree::CHECKOUT_RECORD_FILE)
}

fn age(directory: &Path, by: Duration) {
    uze_platform::fs::set_modified(directory, SystemTime::now() - by).unwrap();
}

#[test]
fn a_made_checkout_is_recorded_where_it_lives() {
    let repository = repository("record-made");
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "made");

    assert!(matches!(record::read(&slot.path), Recorded::Ours(_)));
    assert!(
        record_file(&slot.path).starts_with(repository.root().join(".git/worktrees")),
        "the record lives in Git's administrative directory, not in the tree"
    );
    assert!(
        repository
            .git_in(&slot.path, &["status", "--porcelain"])
            .is_empty()
    );
}

#[test]
fn the_record_outlives_lost_state() {
    let repository = repository("record-outlives");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (agent, _) = launch(&repository, &mut store, "gone");
    end(&repository, &mut store, &agent);

    let mut lost = AgentStore::default();
    let report = reconcile(primary, &mut lost, TARGET);
    assert_eq!(report.adopted.len(), 1, "the slot is still known as UZE's");
    assert_eq!(slots(primary, &lost, &nobody())[0].state, SlotState::Free);
}

#[test]
fn the_record_goes_with_the_worktree() {
    let repository = repository("record-goes");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (_, removed) = launch(&repository, &mut store, "removed");
    let (_, pruned) = launch(&repository, &mut store, "pruned");
    let removed_record = record_file(&removed.path);
    let pruned_record = record_file(&pruned.path);

    repository.git(&[
        "worktree",
        "remove",
        "--force",
        &removed.path.to_string_lossy(),
    ]);
    fs::remove_dir_all(&pruned.path).unwrap();
    repository.git(&["worktree", "prune"]);

    assert!(!removed_record.exists());
    assert!(!pruned_record.exists());
    assert!(linked_worktrees(primary).is_empty());
}

/// `git worktree repair` points a copied checkout at the original's
/// administrative directory; the record there names the original.
#[test]
fn a_copied_checkout_does_not_inherit_the_record() {
    let repository = repository("record-copied");
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "original");
    let copy = repository.root().join("copy");
    fs::create_dir_all(&copy).unwrap();
    fs::copy(slot.path.join(".git"), copy.join(".git")).unwrap();

    assert_eq!(record::read(&copy), Recorded::Absent);
    assert!(matches!(record::read(&slot.path), Recorded::Ours(_)));
}

#[test]
fn a_record_from_a_newer_build_is_neither_reused_removed_nor_written_over() {
    let repository = repository("record-newer");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (agent, slot) = launch(&repository, &mut store, "newer");
    end(&repository, &mut store, &agent);
    let newer = format!(
        "{{\"schema_version\": 99, \"path\": {:?}}}",
        slot.path.to_string_lossy()
    );
    fs::write(record_file(&slot.path), &newer).unwrap();

    let (_, next) = launch(&repository, &mut store, "next");
    assert!(next.created, "never offered to an agent");
    let removed = trim_free_slots(
        primary,
        &store,
        Pool {
            spare: 0,
            idle: Duration::ZERO,
        },
        &nobody(),
    );
    assert!(!removed.contains(&slot.id), "never removed");
    assert!(record::write(&slot.path, &CheckoutRecord::made_at(&slot.path)).is_err());
    assert_eq!(fs::read_to_string(record_file(&slot.path)).unwrap(), newer);
}

/// An older build made this slot and wrote no record; the task store still
/// names the agent it launched there.
#[test]
fn a_launched_agents_slot_is_recorded_on_sight() {
    let repository = repository("record-on-sight");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (agent, slot) = launch(&repository, &mut store, "older");
    fs::remove_file(record_file(&slot.path)).unwrap();

    reconcile(primary, &mut store, TARGET);

    assert!(matches!(record::read(&slot.path), Recorded::Ours(_)));
    assert_eq!(
        store.slot_owner(&slot.id).map(|owner| owner.id.clone()),
        Some(agent.id)
    );
}

/// The builds before recording adopted any worktree under the isolation
/// directory into the task store, with no harness. That adoption is not
/// evidence UZE made it.
#[test]
fn an_earlier_builds_inference_is_not_inherited() {
    let repository = repository("record-inferred");
    let primary = repository.root();
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "agent/parser",
        ".worktrees/parser",
        "HEAD",
    ]);
    let hand_made = primary.join(".worktrees/parser");
    let mut inferred = Agent::in_the_root("");
    let mut isolation = Isolation::cut(
        &inferred.id,
        Base::Ref(TARGET.into()),
        tip_of(primary, TARGET),
        TARGET.into(),
    );
    isolation.branch = "agent/parser".to_owned();
    isolation.checkout = Some(CheckoutId::adopted("parser"));
    inferred.isolation = Some(isolation);
    inferred.state = WorkState::Closed;
    let mut store = AgentStore::default();
    store.upsert(inferred);

    reconcile(primary, &mut store, TARGET);

    assert_eq!(record::read(&hand_made), Recorded::Absent);
    assert!(slots(primary, &store, &nobody()).is_empty());
}

/// An older build saved the task store without `parent`; the checkout's
/// own record still says whose child it is, and at which commit it split.
#[test]
fn a_child_whose_agent_an_older_build_forgot_is_given_it_back() {
    let repository = repository("record-parent");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (agent, _) = launch(&repository, &mut store, "agent");
    let (child, slot) = launch(&repository, &mut store, "child");
    let split_at = child.isolation().unwrap().base_commit.clone();
    record::write(
        &slot.path,
        &CheckoutRecord {
            path: slot.path.clone(),
            parent: Some(agent.id.clone()),
            split_at: Some(split_at),
        },
    )
    .unwrap();
    assert_eq!(store.get(&child.id).unwrap().parent, None);

    reconcile(primary, &mut store, TARGET);

    assert_eq!(store.get(&child.id).unwrap().parent, Some(agent.id));
}

/// A released child's branch is done once its agent's branch carries it,
/// long before anything reaches the project's target.
#[test]
fn a_joined_childs_branch_is_pruned_against_its_agents_branch() {
    let repository = repository("prune-child");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (agent, parent) = launch(&repository, &mut store, "agent");
    let (mut child, slot) = launch(&repository, &mut store, "child");
    fs::write(slot.path.join("child.rs"), "fn child() {}\n").unwrap();
    repository.git_in(&slot.path, &["add", "."]);
    repository.git_in(&slot.path, &["commit", "-qm", "child"]);
    repository.git_in(&parent.path, &["merge", "-q", "--ff-only", &slot.branch]);
    child.parent = Some(agent.id.clone());
    child.isolation_mut().unwrap().target = parent.branch.clone();
    child.state = WorkState::Closed;
    store.upsert(child);
    repository.git_in(&slot.path, &["switch", "-q", "--detach"]);

    let pruned = prune_integrated_branches(primary, &store, TARGET);

    assert_eq!(pruned, vec![slot.branch.clone()]);
    assert!(
        branch_exists(primary, &parent.branch),
        "the agent's own branch stays"
    );
}

/// A harness isolating on its own from inside a slot, a worktree added by
/// hand in the isolation directory, and one somewhere else entirely.
#[test]
fn every_worktree_is_accounted_for_by_owner() {
    let repository = repository("account-owners");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let (_, slot) = launch(&repository, &mut store, "agent");
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "worktree-agent-x",
        &slot
            .path
            .join(".harness/worktrees/agent-x")
            .to_string_lossy(),
        "HEAD",
    ]);
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "by-hand",
        ".worktrees/by-hand",
        "HEAD",
    ]);
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "elsewhere",
        "../elsewhere",
        "HEAD",
    ]);

    let owners: Vec<(String, Owner)> = account(primary, &[("a-harness", ".harness/worktrees")])
        .into_iter()
        .map(|checkout| (checkout.branch.unwrap_or_default(), checkout.owner))
        .collect();

    assert!(owners.contains(&(slot.branch.clone(), Owner::Agent)));
    assert!(owners.contains(&(
        "worktree-agent-x".to_owned(),
        Owner::Harness {
            harness: "a-harness".to_owned()
        }
    )));
    assert!(owners.contains(&("by-hand".to_owned(), Owner::Operator)));
    assert!(owners.contains(&("elsewhere".to_owned(), Owner::Operator)));
    assert_eq!(owners.len(), 4, "the primary is never listed");
}

/// A lock with the same pins and fewer or more entries is what `install`
/// leaves; a moved pin is what `update` leaves, and that is somebody's work.
mod derived_content {
    use super::*;

    const LOCK: &str = "version: 1\nmarketplaces:\n  m:\n    git: https://example.invalid/m\n    revision: 02d2c9831eb619821c574622a78bbc5f6189485a\nplugins:\n  a:\n    marketplace: m\n    integrity: sha256:aaaa\n";

    fn launched_with(
        label: &str,
        files: &[(&str, &str)],
    ) -> (Repository, AgentStore, Agent, Acquired) {
        let repository = repository(label);
        for (name, contents) in files {
            repository.commit_file(name, contents);
        }
        let mut store = AgentStore::default();
        let (agent, slot) = launch(&repository, &mut store, label);
        (repository, store, agent, slot)
    }

    #[test]
    fn a_lock_that_only_gained_an_entry_leaves_the_slot_free() {
        let (repository, mut store, agent, slot) =
            launched_with("derived-lock", &[("agents.lock", LOCK)]);
        let grown = format!("{LOCK}  b:\n    marketplace: m\n    integrity: sha256:bbbb\n");
        fs::write(slot.path.join("agents.lock"), grown).unwrap();

        assert_eq!(end(&repository, &mut store, &agent), SlotState::Free);
        let (_, next) = launch(&repository, &mut store, "next");
        assert_eq!(next.path, slot.path);
        assert_eq!(
            fs::read_to_string(next.path.join("agents.lock")).unwrap(),
            LOCK,
            "the next agent starts from the base's lock"
        );
    }

    #[test]
    fn a_moved_pin_parks_the_slot() {
        let (repository, mut store, agent, slot) =
            launched_with("derived-pin", &[("agents.lock", LOCK)]);
        fs::write(
            slot.path.join("agents.lock"),
            LOCK.replace("sha256:aaaa", "sha256:cccc"),
        )
        .unwrap();

        assert_eq!(end(&repository, &mut store, &agent), SlotState::Parked);
    }

    const INSTRUCTIONS: &str = "# Project\n\nWritten by a person.\n\n<!-- uze:begin project:policy/1 -->\nold\n<!-- uze:end project:policy/1 -->\n";

    #[test]
    fn a_reprojected_region_leaves_the_slot_free() {
        let (repository, mut store, agent, slot) =
            launched_with("derived-region", &[("AGENTS.md", INSTRUCTIONS)]);
        fs::write(
            slot.path.join("AGENTS.md"),
            INSTRUCTIONS.replace("policy/1 -->\nold", "policy/1 -->\nnew\nlonger"),
        )
        .unwrap();

        assert_eq!(end(&repository, &mut store, &agent), SlotState::Free);
    }

    /// A free slot still holding a reprojected region is handed on even
    /// when the target changed that same file since: the edit never parked
    /// it, so it must not stand between the next agent and its base either.
    #[test]
    fn a_free_slot_holding_a_region_edit_is_reused_onto_a_target_that_changed_the_file() {
        let (repository, mut store, agent, slot) =
            launched_with("derived-region-moved", &[("AGENTS.md", INSTRUCTIONS)]);
        fs::write(
            slot.path.join("AGENTS.md"),
            INSTRUCTIONS.replace("policy/1 -->\nold", "policy/1 -->\nnew"),
        )
        .unwrap();
        assert_eq!(end(&repository, &mut store, &agent), SlotState::Free);
        let moved = INSTRUCTIONS.replace("Written by a person.", "Edited on the target.");
        repository.commit_file("AGENTS.md", &moved);

        let (_, next) = launch(&repository, &mut store, "next");

        assert_eq!(next.path, slot.path);
        assert_eq!(
            fs::read_to_string(next.path.join("AGENTS.md")).unwrap(),
            moved,
            "the next agent starts from its base, not from the last one's edit"
        );
    }

    #[test]
    fn a_hand_edit_beside_a_region_parks_the_slot() {
        let (repository, mut store, agent, slot) =
            launched_with("derived-hand-edit", &[("AGENTS.md", INSTRUCTIONS)]);
        fs::write(
            slot.path.join("AGENTS.md"),
            INSTRUCTIONS.replace("Written by a person.", "Rewritten by an agent."),
        )
        .unwrap();

        assert_eq!(end(&repository, &mut store, &agent), SlotState::Parked);
    }
}

mod spares {
    use super::*;

    fn ended_slots(repository: &Repository, store: &mut AgentStore, count: usize) -> Vec<Acquired> {
        let launched: Vec<(Agent, Acquired)> = (0..count)
            .map(|index| launch(repository, store, &format!("agent-{index}")))
            .collect();
        launched
            .into_iter()
            .map(|(agent, slot)| {
                end(repository, store, &agent);
                slot
            })
            .collect()
    }

    #[test]
    fn closing_agents_leaves_spares_for_the_next_ones() {
        let repository = repository("spares-reused");
        let primary = repository.root();
        let mut store = AgentStore::default();
        let working: Vec<(Agent, Acquired)> = (0..3)
            .map(|index| launch(&repository, &mut store, &format!("working-{index}")))
            .collect();
        let closed = ended_slots(&repository, &mut store, 2);

        assert!(trim_free_slots(primary, &store, Pool::default(), &nobody()).is_empty());
        let (_, fourth) = launch(&repository, &mut store, "fourth");
        let (_, fifth) = launch(&repository, &mut store, "fifth");
        assert!(!fourth.created && !fifth.created, "placed in the spares");
        assert_eq!(
            linked_worktrees(primary).len(),
            working.len() + closed.len()
        );
    }

    #[test]
    fn spares_beyond_the_declared_number_are_removed() {
        let repository = repository("spares-trimmed");
        let primary = repository.root();
        let mut store = AgentStore::default();
        let closed = ended_slots(&repository, &mut store, 5);
        for (index, slot) in closed.iter().enumerate() {
            age(&slot.path, Duration::from_secs(60 * (5 - index as u64)));
        }

        let removed = trim_free_slots(primary, &store, Pool::default(), &nobody());

        assert_eq!(removed.len(), 3);
        let kept: std::collections::BTreeSet<PathBuf> = linked_worktrees(primary)
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(
            kept,
            [closed[3].path.clone(), closed[4].path.clone()].into(),
            "the two most recently used stay"
        );
        assert!(
            closed
                .iter()
                .all(|slot| branch_exists(primary, &slot.branch)),
            "every branch is kept"
        );
    }

    #[test]
    fn an_idle_project_gives_its_disk_back() {
        let repository = repository("spares-idle");
        let primary = repository.root();
        let mut store = AgentStore::default();
        let closed = ended_slots(&repository, &mut store, 1);
        age(&closed[0].path, Duration::from_secs(4 * 24 * 60 * 60));

        assert_eq!(
            trim_free_slots(primary, &store, Pool::default(), &nobody()),
            vec![closed[0].id.clone()]
        );
    }

    #[test]
    fn work_is_never_trimmed() {
        let repository = repository("spares-work");
        let primary = repository.root();
        let mut store = AgentStore::default();
        let (_, working) = launch(&repository, &mut store, "working");
        let (parked, holding) = launch(&repository, &mut store, "parked");
        fs::write(holding.path.join("draft.rs"), "fn main() {}\n").unwrap();
        end(&repository, &mut store, &parked);

        let nothing_spare = Pool {
            spare: 0,
            idle: Duration::ZERO,
        };
        assert!(trim_free_slots(primary, &store, nothing_spare, &nobody()).is_empty());
        assert!(working.path.is_dir() && holding.path.is_dir());
    }

    #[test]
    fn a_declared_policy_sets_both_numbers() {
        let policy = crate::worktree::WorktreePolicy {
            spare: Some(4),
            idle_days: Some(1),
            ..Default::default()
        };
        assert_eq!(
            Pool::declared_by(Some(&policy)),
            Pool {
                spare: 4,
                idle: Duration::from_secs(24 * 60 * 60),
            }
        );
        assert_eq!(Pool::declared_by(None), Pool::default());
    }
}

mod in_use {
    use super::*;

    #[test]
    fn a_process_inside_a_free_looking_slot_keeps_it() {
        let repository = repository("in-use-kept");
        let primary = repository.root();
        let mut store = AgentStore::default();
        let (agent, slot) = launch(&repository, &mut store, "left");
        end(&repository, &mut store, &agent);
        let somebody = Presence::Known(vec![slot.path.join("src")]);

        assert!(matches!(
            slots(primary, &store, &somebody)[0].state,
            SlotState::Occupied { .. }
        ));
        let nothing_spare = Pool {
            spare: 0,
            idle: Duration::ZERO,
        };
        assert!(trim_free_slots(primary, &store, nothing_spare, &somebody).is_empty());
        assert_eq!(slots(primary, &store, &nobody())[0].state, SlotState::Free);
    }

    #[test]
    fn a_process_table_nobody_could_read_holds_every_slot() {
        let repository = repository("in-use-unknown");
        let primary = repository.root();
        let mut store = AgentStore::default();
        let (agent, _) = launch(&repository, &mut store, "left");
        end(&repository, &mut store, &agent);

        assert_ne!(
            slots(primary, &store, &Presence::Unknown)[0].state,
            SlotState::Free
        );
        let (_, next) = acquire_for(&repository, &store, &Presence::Unknown);
        assert!(next.created);
    }

    fn acquire_for(
        repository: &Repository,
        store: &AgentStore,
        presence: &Presence,
    ) -> (Agent, Acquired) {
        let primary = repository.root();
        let agent = Agent::isolated(
            "a-harness",
            Some("next"),
            Base::Ref(TARGET.into()),
            tip_of(primary, TARGET),
            TARGET.into(),
        );
        let isolation = agent.isolation().unwrap();
        let acquired = acquire(
            primary,
            store,
            isolation,
            &isolation.base_commit,
            None,
            presence,
        )
        .unwrap();
        (agent, acquired)
    }
}
