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
/// (shelved when they hold work), marks tasks without a checkout from where
/// their branch stands, and prunes Git's registry only after every
/// directory has been looked at.
#[tracing::instrument(name = "checkout.reconcile", level = "debug", skip_all)]
pub fn reconcile(primary: &Path, store: &mut AgentStore, target: &str) -> Reconciliation {
    let mut report = Reconciliation::default();
    let registered = isolated_checkouts(primary);
    super::earlier::record_on_sight(primary, &registered, store);
    super::earlier::restore_parents(primary, &registered, store);

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
                    // the target (the one outcome a `merge` delivery produces;
                    // a `push` leaves a task `Ready`), so a commit the target
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
            || holds_unkept_commits(primary, path, target)
            || branch
                .as_deref()
                .is_some_and(|branch| !is_integrated(primary, target, branch));
        // A released slot belongs to nobody by design: clean, detached, its
        // agent's work kept elsewhere. One still on a branch, or holding
        // work, stands for a task whose record was lost, and is recorded so
        // a release can keep what it holds and free it.
        if !holds_work && branch.is_none() {
            continue;
        }
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
            WorkState::Shelved
        } else {
            WorkState::Closed
        };
        agent.isolation = Some(isolation);
        // Ended as it is adopted: nothing launched it, so nothing is at
        // work in it until a pane is seen there — and born live, the slot
        // read as taken by an agent until a release pass happened by.
        agent.end();
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

    super::earlier::end_all_but_each_slots_holder(primary, &registered, store, target);
    report.adopted.extend(adopt_shelves(primary, store, target));

    record::settle_moves(primary);
    let _ = git(primary, &["worktree", "prune"]);
    report
}

/// Ends a task that no longer has a checkout of its own, by what its
/// branch and its shelf still hold.
pub(super) fn end_without_checkout(primary: &Path, target: &str, agent: &mut Agent) {
    let Some(isolation) = agent.isolation_mut() else {
        return;
    };
    if let Some(checkout) = isolation.checkout.take() {
        isolation.last_checkout = Some(checkout);
    }
    settle_without_checkout(primary, target, agent);
}

/// Records a shelved task for every shelf no recorded task holds, under
/// the task the shelf names: a store that was lost or written by a build
/// that dropped the task still lists the work.
pub(super) fn adopt_shelves(primary: &Path, store: &mut AgentStore, target: &str) -> Vec<AgentId> {
    let mut adopted = Vec::new();
    for found in shelf::list(primary) {
        if store.agent(&found.task).is_some() {
            continue;
        }
        let id = AgentId::adopted(&found.task);
        let mut agent = Agent::in_the_root("");
        agent.id = id.clone();
        agent.label = if found.label.is_empty() {
            found.task.clone()
        } else {
            found.label.clone()
        };
        let mut isolation = Isolation::cut(
            &id,
            Base::Ref(target.to_owned()),
            tip_of(primary, &format!("{}^1", found.commit)),
            target.to_owned(),
        );
        if !found.branch.is_empty() {
            isolation.branch = found.branch.clone();
        }
        agent.isolation = Some(isolation);
        agent.state = WorkState::Shelved;
        adopted.push(id);
        store.upsert(agent);
    }
    adopted
}
