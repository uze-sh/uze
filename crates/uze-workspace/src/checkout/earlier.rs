//! What reconciling checkouts does only because an earlier build left
//! them (the `earlier-releases` rule): the `agent-<n>` checkouts the builds
//! before slots made, the `parent` an older build drops on its save, and a
//! store an earlier build wrote with several agents naming one slot. Each
//! runs where it always ran, on every reconcile.

use uze_document::earlier::{Earlier, Lasts};

use super::*;

/// This module's entries. Reconciling runs only when a project's workspace
/// does, so none is bounded yet.
pub(crate) fn earlier() -> Vec<Earlier> {
    vec![
        Earlier {
            name: "the agent-<n> checkouts the builds before slots made",
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
        Earlier {
            name: "the parent an older build drops from a subagent's agent",
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
        Earlier {
            name: "several agents an earlier build left naming one slot",
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
    ]
}

/// Whether `id` is the `agent-<n>` the builds before slots gave their
/// checkouts, which carry no record of their own.
pub(super) fn made_before_slots(id: &CheckoutId) -> bool {
    id.as_str().strip_prefix("agent-").is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// A slot outlives the tasks that ran in it, and a store an earlier build
/// wrote can still have several naming one. The holder is the one whose
/// branch the checkout has — a fact of Git's, where the store's order is
/// only what was recorded — and otherwise the one recorded last; every
/// other one ends by what its own branch and shelf hold. An earlier one
/// still reading as live answered every question about "the task in this
/// checkout" as well — an evaluation renamed it after the slot's current
/// branch, and discarding it would have deleted the new agent's branch.
pub(super) fn end_all_but_each_slots_holder(
    primary: &Path,
    registered: &[(PathBuf, Option<String>)],
    store: &mut AgentStore,
    target: &str,
) {
    for (path, branch) in registered {
        let id = CheckoutId::adopted(&slot_name(path));
        let naming: Vec<AgentId> = store
            .agents
            .iter()
            .filter(|agent| {
                agent
                    .isolation()
                    .is_some_and(|isolation| isolation.checkout.as_ref() == Some(&id))
            })
            .map(|agent| agent.id.clone())
            .collect();
        if naming.len() < 2 {
            continue;
        }
        let holder = naming
            .iter()
            .find(|candidate| {
                store
                    .agent(candidate.as_str())
                    .and_then(Agent::isolation)
                    .is_some_and(|isolation| Some(&isolation.branch) == branch.as_ref())
            })
            .or(naming.last())
            .cloned();
        for agent in store
            .agents
            .iter_mut()
            .filter(|agent| naming.contains(&agent.id) && Some(&agent.id) != holder.as_ref())
        {
            end_without_checkout(primary, target, agent);
        }
    }
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
        if (launched_here || made_before_slots(&id))
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
