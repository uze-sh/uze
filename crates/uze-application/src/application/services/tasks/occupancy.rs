//! Which checkouts are occupied, and releasing what nobody is working in any more.

use super::*;

impl Workspace<'_> {
    /// One pass of "who is actually sitting in which slot", across every
    /// repository the workspace can see.
    ///
    /// `look_in` names the directories worth reconsidering — the checkout
    /// a pane just left, plus (on a client's first pass, when nothing has
    /// vanished yet because nothing was ever seen) every open space's own
    /// root. `held` is every checkout a live pane still sits in, and it
    /// governs both halves: a task no pane is in front of ends, and a
    /// directory a pane *is* in is never collected, whatever its record
    /// says — the agent that delivered a task is still there until its
    /// tab closes.
    ///
    /// The sequencing is the point, and it is domain rather than
    /// presentation: several directories resolve to one repository and it
    /// must be reconciled once, not once per pane; a release must precede
    /// the collection that acts on it; and only the removals that cannot
    /// lose work are ever taken. A caller that got any of that wrong would
    /// hand one agent's slot to another.
    #[tracing::instrument(name = "workspace.reconcile_occupancy", skip_all)]
    pub fn reconcile_occupancy(
        &self,
        look_in: &[PathBuf],
        held: &[PathBuf],
        echoed: &[String],
    ) -> Reconciliation {
        let mut reconciliation = Reconciliation::default();
        let mut seen = BTreeSet::new();
        for cwd in look_in {
            let Some(primary) = self.primary_of(cwd) else {
                continue;
            };
            if !seen.insert(primary) {
                continue;
            }
            let (released, waiting) = self.release_abandoned(cwd, held, echoed);
            if waiting {
                reconciliation.waiting.push(cwd.clone());
            }
            if !released.is_empty() {
                reconciliation.changed.push(cwd.clone());
                reconciliation.released.extend(released);
            }
            // Only ever the removals that cannot lose work, and only from
            // the path that just changed what "in use" means.
            self.collect_slot_garbage(cwd, held);
        }
        // Agents in the root are keyed by that root, whether or not it is a
        // repository, so this pass is per root and needs no Git. A slot's
        // directory is never a root of its own.
        let mut roots = BTreeSet::new();
        for root in look_in {
            if worktree::isolated_checkout(root).is_some() || !roots.insert(canonical(root)) {
                continue;
            }
            if !self.end_abandoned_agents(root, echoed).is_empty() {
                reconciliation.changed.push(root.clone());
            }
        }
        reconciliation
    }

    /// Records an agent launched into `root` itself — the space's own
    /// directory, on whatever branch it is on. No repository is needed:
    /// an agent that is not isolated is keyed by the directory it works
    /// in, which is every directory.
    pub(super) fn record_in_the_root(&self, root: &Path, harness: &str) -> Result<Agent> {
        task::locked(&self.0.home, root, |store| {
            let agent = Agent::in_the_root(harness);
            store.upsert(agent.clone());
            Ok(agent)
        })
    }

    /// Ends every live agent of `root` that no live tab was launched for,
    /// and answers the identifiers it ended. An agent a tab still echoes
    /// stays live whatever else is true. Needs no repository, and writes
    /// nothing where nothing was ever recorded. Only the agents working
    /// in the root: an isolated one is ended by its slot's own sweep,
    /// which knows what its checkout still holds.
    #[tracing::instrument(name = "workspace.end_abandoned_agents", skip_all, fields(root = %root.display()))]
    pub fn end_abandoned_agents(&self, root: &Path, echoed: &[String]) -> Vec<String> {
        let root = canonical(root);
        if !task::store_path(&self.0.home, &root).exists() {
            return Vec::new();
        }
        task::locked(&self.0.home, &root, |store| {
            let mut ended = Vec::new();
            for agent in store.agents.iter_mut().filter(|agent| !agent.is_isolated()) {
                if !agent.is_live() || echoed.iter().any(|id| id == agent.id.as_str()) {
                    continue;
                }
                agent.end();
                tracing::info!(agent = %agent.id.as_str(), "an agent no tab runs any more was ended");
                ended.push(agent.id.as_str().to_owned());
            }
            Ok(ended)
        })
        .unwrap_or_default()
    }

    /// Ends every task no pane is in front of any more, and gives its
    /// checkout back to the pool with its work kept.
    ///
    /// `occupied` names the checkout directories a live pane still sits in
    /// and `echoed` the agents live tabs were launched for. A task in
    /// neither has no agent here: its uncommitted work goes on a shelf, its
    /// commits stay on its branch, and its checkout is free. Delivery is not
    /// the only way a task ends — most end by the operator closing the tab
    /// — and a slot nobody ever released is a slot no new agent can reuse.
    /// Two questions because they have two answers: the directory is what
    /// keeps a slot from being handed on, and the launch is what still names
    /// the task once the directory is gone from under it. Neither is the
    /// last word: the release reads the process table before it resets
    /// anything, since another client's agent is in neither list.
    ///
    /// An ended task still naming a checkout is released too: one whose
    /// work could not be kept last time, or one an earlier build left holding it.
    /// One release per hold of the task lock, so a pass with many to do —
    /// the first after an upgrade — never keeps a placement waiting behind
    /// all of them.
    #[tracing::instrument(name = "workspace.release_abandoned_tasks", skip_all, fields(cwd = %cwd.display()))]
    pub fn release_abandoned_tasks(
        &self,
        cwd: &Path,
        occupied: &[PathBuf],
        echoed: &[String],
    ) -> Vec<ReleasedTask> {
        self.release_abandoned(cwd, occupied, echoed).0
    }

    /// [`Self::release_abandoned_tasks`], and whether any of them was
    /// found in use and left as it was.
    fn release_abandoned(
        &self,
        cwd: &Path,
        occupied: &[PathBuf],
        echoed: &[String],
    ) -> (Vec<ReleasedTask>, bool) {
        let Some((primary, policy)) = self.repository_context(cwd) else {
            return (Vec::new(), false);
        };
        let target = target_of(&primary, &policy);
        let presence = checkout::Presence::observe_with(occupied);
        let mut released = Vec::new();
        let mut in_use = false;
        let mut looked_at: Vec<AgentId> = Vec::new();
        let mut reconciled = false;
        loop {
            let step = task::locked(&self.0.home, &primary, |store| {
                if !std::mem::replace(&mut reconciled, true) {
                    checkout::reconcile(&primary, store, &target);
                }
                let Some(index) = store.agents.iter().position(|agent| {
                    !looked_at.contains(&agent.id)
                        && is_abandoned(&primary, store, agent, occupied, echoed)
                }) else {
                    return Ok(None);
                };
                let agent = &mut store.agents[index];
                let id = agent.id.clone();
                let label = agent.label.clone();
                let was_live = checkout::is_live(&agent.state);
                let outcome = checkout::release(&primary, agent, &target, &presence);
                let mut unfinished = agent.state == WorkState::Shelved;
                let parent = agent.parent.clone();
                // A subagent released after its agent: its work reaches the
                // target only through that agent, which is unfinished too.
                if unfinished
                    && let Some(parent) = parent.and_then(|parent| task_mut(store, parent.as_str()))
                {
                    parent.state = WorkState::Shelved;
                }
                if outcome != checkout::Released::InUse
                    && release_children(&primary, store, id.as_str(), &presence)
                {
                    unfinished = true;
                }
                tracing::info!(agent = %id.as_str(), ?outcome, "an agent no pane holds was released");
                Ok(Some((id, label, outcome, was_live, unfinished)))
            });
            // A release that was not recorded did not happen, as far as the
            // next pass can tell: stop rather than report it.
            let Ok(Some((id, label, outcome, was_live, unfinished))) = step else {
                break;
            };
            looked_at.push(id.clone());
            in_use |= outcome == checkout::Released::InUse;
            let ended_now = was_live && outcome != checkout::Released::InUse;
            let freed_now = matches!(outcome, checkout::Released::Freed { shelved: true });
            if ended_now || freed_now {
                released.push(ReleasedTask {
                    id: id.as_str().to_owned(),
                    label,
                    unfinished,
                });
            }
        }
        (released, in_use)
    }

    /// Takes out the safe removals: the directory of every free slot gone
    /// idle, its branch kept; a branch whose every commit is already in the
    /// target and no shelf stands on; and a shelf the target already has.
    /// Nothing holding work is ever touched here, and nothing somebody is
    /// working in; that is the operator's alone.
    #[tracing::instrument(name = "workspace.collect_slot_garbage", skip_all, fields(cwd = %cwd.display()))]
    pub(super) fn collect_slot_garbage(&self, cwd: &Path, occupied: &[PathBuf]) -> Vec<String> {
        let Some((primary, policy)) = self.repository_context(cwd) else {
            return Vec::new();
        };
        let target = target_of(&primary, &policy);
        let pool = checkout::Pool::declared_by(Some(&policy));
        // Under the document's lock, which a placement holds from choosing
        // a slot to recording it: read outside it, a slot just taken for a
        // new agent was still free in the copy, and was collected from
        // under it.
        let collected = task::locked(&self.0.home, &primary, |store| {
            Ok(checkout::collect(
                &primary,
                store,
                &target,
                pool,
                &checkout::Presence::observe_with(occupied),
            ))
        })
        .unwrap_or_default();
        checkout::empty_trash(&primary);
        let collected: Vec<String> = collected
            .branches
            .into_iter()
            .chain(collected.slots.into_iter().map(|slot| slot.to_string()))
            .chain(
                collected
                    .shelves
                    .into_iter()
                    .map(|task| format!("shelf {task}")),
            )
            .collect();
        if !collected.is_empty() {
            tracing::info!(
                ?collected,
                "merged branches, idle slots and delivered shelves were removed"
            );
        }
        collected
    }

    /// The operator declares a handed-off task done: its slot is free and
    /// its branch stays.
    #[tracing::instrument(name = "workspace.finish_task", skip_all, fields(cwd = %cwd.display(), task_id = %task_id), err)]
    pub fn finish_task(&self, cwd: &Path, task_id: &str) -> Result<()> {
        let (primary, _) = self
            .repository_context(cwd)
            .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
        task::locked(&self.0.home, &primary, |store| {
            let agent = task_mut(store, task_id)
                .filter(|agent| agent.is_isolated())
                .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
            agent.state = WorkState::Integrated;
            Ok(())
        })
    }

    /// The one path that deletes work, taken only by the operator on a
    /// named task: the checkout and the branch go, the record goes with them.
    #[tracing::instrument(name = "workspace.discard_task", skip_all, fields(cwd = %cwd.display(), task_id = %task_id), err)]
    pub fn discard_task(&self, cwd: &Path, task_id: &str) -> Result<()> {
        let (primary, _) = self
            .repository_context(cwd)
            .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
        task::locked(&self.0.home, &primary, |store| {
            let agent = task_mut(store, task_id)
                .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
            let agent_id = agent.id.clone();
            let task = agent
                .isolation()
                .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?
                .clone();
            checkout::discard(&primary, agent_id.as_str(), &task).map_err(UzeError::Discard)?;
            store
                .agents
                .retain(|recorded| recorded.id.as_str() != task_id);
            // The one place a task stops existing, and therefore the one
            // place its conversations stop being reachable. Finishing is
            // deliberately not such a place: an integrated task whose agent
            // kept working is revived by reconciliation, and it would come
            // back without the conversation it never left.
            conversation::forget(&self.0.home, &primary, &agent_id);
            Ok(())
        })
    }
}
