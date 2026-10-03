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
            let released = self.release_abandoned_tasks(cwd, held, echoed);
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

    /// Ends every task no pane is in front of any more, and says what
    /// became of each slot.
    ///
    /// `occupied` names the checkout directories a live pane still sits in
    /// and `echoed` the agents live tabs were launched for. A task in
    /// neither has no agent: its slot goes back to the pool when it holds
    /// nothing, and is parked for the operator when it holds work. Delivery
    /// is not the only way a task ends — most end by the operator closing
    /// the tab — and a slot nobody ever released is a slot no new agent can
    /// reuse. Two questions because they have two answers: the directory
    /// is what keeps a slot from being handed on, and the launch is what
    /// still names the task once the directory is gone from under it.
    #[tracing::instrument(name = "workspace.release_abandoned_tasks", skip_all, fields(cwd = %cwd.display()))]
    pub fn release_abandoned_tasks(
        &self,
        cwd: &Path,
        occupied: &[PathBuf],
        echoed: &[String],
    ) -> Vec<ReleasedTask> {
        let Some((primary, policy)) = self.repository_context(cwd) else {
            return Vec::new();
        };
        let target = target_of(&primary, &policy);
        let mut released = Vec::new();
        let recorded = task::locked(&self.0.home, &primary, |store| {
            for agent in store.agents.iter_mut() {
                let id = agent.id.clone();
                let label = agent.label.clone();
                // A delivery in flight owns the agent until it answers. A
                // subagent's checkout carries no pane of its own, and ends
                // with its agent below rather than here.
                if !is_agents_turn(&agent.state) || agent.parent.is_some() {
                    continue;
                }
                let Some(task) = agent.isolation() else {
                    continue;
                };
                let in_its_slot = landing::slot_path(&primary, task)
                    .is_some_and(|slot| occupied.iter().any(|pane| pane.starts_with(&slot)));
                let launched_for = echoed.iter().any(|echo| echo == id.as_str());
                if in_its_slot || launched_for {
                    continue;
                }
                let slot = checkout::release(&primary, agent, &target);
                tracing::info!(agent = %id.as_str(), ?slot, "an agent no pane holds was released");
                released.push(ReleasedTask {
                    id: id.as_str().to_owned(),
                    label,
                    parked: slot == checkout::SlotState::Parked,
                });
            }
            for parent in released.iter_mut() {
                if release_children(&primary, store, &parent.id) {
                    parent.parked = true;
                }
            }
            Ok(())
        });
        // A release is a record and nothing else — `checkout::release`
        // touches no directory — so one that was not written did not
        // happen, and reporting it would have the collection below act on
        // a slot the next pass still reads as taken.
        if recorded.is_err() {
            return Vec::new();
        }
        released
    }

    /// Takes out the safe removals: an `agent/` branch whose every commit
    /// is already in the target, and the directory of every free slot the
    /// project's pool does not keep — its branch kept. Nothing holding work
    /// is ever touched here, and nothing somebody is working in; that is
    /// the operator's alone.
    #[tracing::instrument(name = "workspace.collect_slot_garbage", skip_all, fields(cwd = %cwd.display()))]
    pub(super) fn collect_slot_garbage(&self, cwd: &Path, occupied: &[PathBuf]) -> Vec<String> {
        let Some(repository) = self.repository(cwd) else {
            return Vec::new();
        };
        let target = repository.target();
        let pool = checkout::Pool::declared_by(self.policy(&repository.primary).ok().as_ref());
        let collected = checkout::collect(
            &repository.primary,
            &repository.store,
            &target,
            pool,
            &checkout::Presence::observe_with(occupied),
        );
        let collected: Vec<String> = collected
            .branches
            .into_iter()
            .chain(collected.slots.into_iter().map(|slot| slot.to_string()))
            .collect();
        if !collected.is_empty() {
            tracing::info!(?collected, "merged branches and spare slots were removed");
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
            checkout::discard(&primary, &task).map_err(UzeError::Discard)?;
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
