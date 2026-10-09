//! Reconciling the recorded agents with the checkouts Git actually has.

use super::*;

/// What reconciling the isolation directory against `store` found.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reconciliation {
    /// Tasks created for checkouts nobody had recorded.
    pub adopted: Vec<AgentId>,
    /// Tasks whose checkout is gone, now marked from where their branch stands.
    pub orphaned: Vec<AgentId>,
    /// Delivered tasks whose agent kept working: their branch carries
    /// commits the target does not, in a checkout still registered.
    pub revived: Vec<AgentId>,
}

/// Brings `store` in line with the isolation directory: records the
/// checkouts UZE can show it made, adopts recorded checkouts without a task
/// (parked when they hold work), marks tasks without a checkout from where
/// their branch stands, and prunes Git's registry only after every
/// directory has been looked at.
#[tracing::instrument(name = "checkout.reconcile", level = "debug", skip_all)]
pub fn reconcile(primary: &Path, store: &mut AgentStore, target: &str) -> Reconciliation {
    let mut report = Reconciliation::default();
    let registered = isolated_checkouts(primary);
    record_on_sight(primary, &registered, store);
    restore_parents(primary, &registered, store);

    for (path, branch) in &registered {
        let id = CheckoutId::adopted(&slot_name(path));
        if let Some(owner_id) = store.slot_owner(&id).map(|agent| agent.id.clone()) {
            let revived = store
                .get_mut(&owner_id)
                .filter(|agent| agent.is_isolated() && agent.parent.is_none())
                .is_some_and(|agent| {
                    let state = agent.state.clone();
                    let isolation = agent.isolation_mut().expect("filtered to isolated");
                    // An agent that keeps working after a delivery is working
                    // again, and its slot is not free while it does: `Integrated`
                    // is only ever reached with the branch's commits already in
                    // the target (the one outcome `merge` completion produces;
                    // handoff and pr leave a task `Ready`), so a commit the target
                    // does not have, in a checkout Git still registers, is new
                    // work. Reading it here is what a slot is acquired against —
                    // `declared_done` otherwise hands the directory to the next
                    // agent while this one is still writing in it. Only the
                    // current owner revives; a slot already handed over answers
                    // for whoever holds it now.
                    if state != WorkState::Integrated
                        || is_integrated(primary, target, &isolation.branch)
                    {
                        return false;
                    }
                    // The request was the delivered work's; this is new work.
                    isolation.forget_request();
                    agent.state = WorkState::Running;
                    true
                });
            if revived {
                report.revived.push(owner_id);
            }
            continue;
        }
        // A checkout without UZE's record is somebody else's, whatever it
        // is called: adopting it would make it a slot the next agent resets.
        if !matches!(record::read(primary, path), Recorded::Ours(_)) {
            continue;
        }
        let holds_work = holds_uncommitted_work(path)
            || branch
                .as_deref()
                .is_some_and(|branch| !is_integrated(primary, target, branch));
        // A generated branch is labelled by the identifier it carries; one
        // somebody named is labelled by that name, never by the slot's id.
        let label = match branch.as_deref() {
            Some(branch) => branch
                .strip_prefix(BRANCH_PREFIX)
                .map_or_else(|| label_of(branch), str::to_owned),
            None => id.as_str().to_owned(),
        };
        // Adopted, so nothing says which harness ran here: the directory
        // is the only evidence, and it does not carry one.
        let mut agent = Agent::in_the_root("");
        let mut isolation = Isolation::cut(
            &agent.id,
            Base::Ref(target.to_owned()),
            tip_of(primary, target),
            target.to_owned(),
        );
        agent.label = label;
        if let Some(branch) = branch {
            isolation.branch = branch.clone();
        }
        isolation.checkout = Some(id);
        // Nobody recorded this checkout, so nobody recorded a delivery
        // from it either: empty means it ended with nothing, not that its
        // work reached the target.
        agent.state = if holds_work {
            WorkState::Parked
        } else {
            WorkState::Closed
        };
        agent.isolation = Some(isolation);
        report.adopted.push(agent.id.clone());
        store.upsert(agent);
    }

    for agent in store.agents.iter_mut() {
        let id = agent.id.clone();
        let Some(checkout) = agent
            .isolation()
            .and_then(|isolation| isolation.checkout.clone())
        else {
            continue;
        };
        if registered
            .iter()
            .any(|(path, _)| slot_name(path) == checkout.as_str())
        {
            continue;
        }
        end_without_checkout(primary, target, agent);
        report.orphaned.push(id);
    }

    // A slot outlives the tasks that ran in it, and each went on naming it.
    // Only the newest stands there now; an earlier one still reading as
    // live answered every question about "the task in this checkout" as
    // well — an evaluation renamed it after the slot's current branch, so a
    // task long gone carried the new agent's name, and discarding it would
    // have deleted the new agent's branch.
    let owners = store.slot_owners();
    for agent in store.agents.iter_mut() {
        let handed_over = !owners.contains(&agent.id);
        let holds_a_slot = agent
            .isolation()
            .is_some_and(|isolation| isolation.checkout.is_some());
        if holds_a_slot && handed_over {
            end_without_checkout(primary, target, agent);
        }
    }

    let _ = git(primary, &["worktree", "prune"]);
    report
}

/// Records the checkouts UZE can show it made but that carry no record:
/// the slot of an agent it launched — one with a harness, which an adoption
/// by inference never had — and the `agent-<n>` of the builds before slots.
/// A standing rule rather than a one-time step, because an older build on
/// the same machine goes on making slots it does not record.
pub(super) fn record_on_sight(
    primary: &Path,
    registered: &[(PathBuf, Option<String>)],
    store: &AgentStore,
) {
    for (path, _) in registered {
        if record::read(primary, path) != Recorded::Absent {
            continue;
        }
        let id = CheckoutId::adopted(&slot_name(path));
        let launched_here = store.agents.iter().any(|agent| {
            !agent.harness.is_empty()
                && agent
                    .isolation()
                    .is_some_and(|isolation| isolation.checkout.as_ref() == Some(&id))
        });
        if (launched_here || id.is_legacy())
            && let Err(reason) = record::write(primary, path, &CheckoutRecord::made_at(path))
        {
            tracing::warn!(checkout = %path.display(), %reason, "could not record a checkout UZE made");
        }
    }
}

/// Gives a subagent's checkout back its agent when the task store lost it:
/// an older build knows nothing of `parent` and drops it on its next save.
/// Only the holder still starting at the record's split point is the child
/// the record describes; one an older build placed there since is not.
pub(super) fn restore_parents(
    primary: &Path,
    registered: &[(PathBuf, Option<String>)],
    store: &mut AgentStore,
) {
    for (path, _) in registered {
        let Recorded::Ours(CheckoutRecord {
            parent: Some(parent),
            split_at: Some(split_at),
            ..
        }) = record::read(primary, path)
        else {
            continue;
        };
        let id = CheckoutId::adopted(&slot_name(path));
        let Some(holder) = store.slot_owner(&id).map(|agent| agent.id.clone()) else {
            continue;
        };
        if let Some(agent) = store.get_mut(&holder)
            && agent.parent.is_none()
            && agent
                .isolation()
                .is_some_and(|isolation| isolation.base_commit == split_at)
        {
            agent.parent = Some(parent);
        }
    }
}

/// Ends a task that no longer has a checkout of its own, by what its
/// branch still holds.
pub(super) fn end_without_checkout(primary: &Path, target: &str, agent: &mut Agent) {
    let Some(isolation) = agent.isolation_mut() else {
        return;
    };
    isolation.checkout = None;
    let branch = isolation.branch.clone();
    // A delivery already recorded stays recorded: its branch has nothing
    // of its own left precisely because the target has it all.
    if branch_exists(primary, &branch) && !is_integrated(primary, target, &branch) {
        agent.state = WorkState::Parked;
    } else if agent.state != WorkState::Integrated {
        agent.state = WorkState::Closed;
    }
}
