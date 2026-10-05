//! Placing a new agent: in a slot of its own, isolated after the fact, or resumed where it left off.

use super::*;

impl Workspace<'_> {
    /// Where a newly created agent starts, decided before its harness does.
    ///
    /// The kind is the space's: a slot is acquired for a new task in the
    /// repository `pane_cwd` belongs to, prepared as the project's policy
    /// says, and the primary checkout is never assigned to an agent placed
    /// that way; or the agent is recorded in the space's own directory,
    /// on whatever branch it is on, which needs no repository at all. `occupied` names the checkout directories a live pane still
    /// sits in; none of them is reused, whatever its task record says — a
    /// delivered task's agent is still there until its tab closes. A slot
    /// that cannot be acquired — no repository, no branch, no commit to
    /// branch from, the cap reached, Git refusing — is an error and starts
    /// nothing: the operator asked for isolation, and an agent that lands
    /// in their own tree instead is the one thing a notice cannot undo.
    #[tracing::instrument(name = "workspace.place_new_agent", skip_all, fields(pane_cwd = %pane_cwd.display(), ?kind, harness))]
    pub fn place_new_agent(
        &self,
        pane_cwd: &Path,
        kind: Option<PlacementKind>,
        harness: &str,
        occupied: &[PathBuf],
    ) -> Result<AgentPlacement> {
        // The project decides, unless the caller asked for one kind by
        // name: `Isolate` is an action on an agent, and a launch is not
        // where that question is asked any more.
        let context = self.repository_context(pane_cwd);
        let declared = context
            .as_ref()
            .map(|(_, policy)| policy.default)
            .unwrap_or_default();
        let kind = kind.unwrap_or(if declared.is_isolated() {
            PlacementKind::Isolated
        } else {
            PlacementKind::InPlace
        });
        match kind {
            PlacementKind::Isolated => self.place_in_slot(pane_cwd, harness, occupied),
            PlacementKind::InPlace => {
                let root = canonical(pane_cwd);
                let agent = self.record_in_the_root(&root, harness)?;
                let view = context.as_ref().and_then(|(primary, policy)| {
                    self.placed_view(primary, agent.id.as_str(), policy)
                });
                // Before the agent starts, and only in the primary checkout:
                // the agent reads the region the project declares now. A pane
                // standing in a slot is not the primary, and a slot's file is
                // its branch's.
                let in_the_primary = context
                    .as_ref()
                    .is_some_and(|(primary, _)| canonical(primary) == root);
                let warnings = if in_the_primary {
                    self.policy_region_warnings(&root)
                } else {
                    Vec::new()
                };
                Ok(AgentPlacement {
                    project: root.clone(),
                    cwd: root,
                    placement: Placement::InPlace { id: agent.id },
                    warnings,
                    view,
                })
            }
        }
    }

    pub(super) fn place_in_slot(
        &self,
        pane_cwd: &Path,
        harness: &str,
        occupied: &[PathBuf],
    ) -> Result<AgentPlacement> {
        let refused = |reason: String| UzeError::AgentPlacement(reason);
        let primary = worktree::primary_checkout(pane_cwd)
            .ok_or_else(|| refused("not inside a Git working tree".to_owned()))?;
        let policy = self
            .policy(&primary)
            .map_err(|error| refused(error.to_string()))?;
        let target = policy
            .target
            .clone()
            .or_else(|| checkout::current_branch(&primary))
            .ok_or_else(|| refused("the primary checkout is not on a branch".to_owned()))?;
        // Before anything is branched from it: an agent placed on a target
        // nobody fetched starts behind every merge of the day, and hears
        // about it as conflicts in a request already opened.
        let sync = landing::sync_target(&primary, &target);
        let base_tip = checkout::tip_of(&primary, &target);
        if base_tip.is_empty() {
            return Err(refused("no commit to branch from".to_owned()));
        }
        // Whatever the lock's answer, this is what was taken from Git
        // before it: the slot has to be given back when the record of it
        // cannot be written.
        let mut taken: Option<Agent> = None;
        let recorded = task::locked(&self.0.home, &primary, |store| {
            checkout::reconcile(&primary, store, &target);
            let mut agent = Agent::isolated(
                harness,
                None,
                Base::Ref(target.clone()),
                base_tip.clone(),
                target.clone(),
            );
            let isolation = agent
                .isolation_mut()
                .expect("an agent built isolated carries its isolation");
            let acquired = match checkout::acquire(
                &primary,
                store,
                isolation,
                &base_tip,
                policy.slots,
                &checkout::Presence::observe_with(occupied),
            ) {
                Ok(acquired) => acquired,
                // Nothing was taken, and the reconciliation above is
                // still worth writing back.
                Err(refusal) => return Ok(Err(refusal.to_string())),
            };
            isolation.checkout = Some(acquired.id.clone());
            let task = agent.clone();
            store.upsert(agent);
            taken = Some(task.clone());
            Ok(Ok((task, acquired)))
        });

        let (task, acquired) = match recorded {
            Ok(Ok(placed)) => placed,
            Ok(Err(refusal)) => return Err(refused(refusal)),
            // A slot nothing records is worse than no slot: nothing would
            // ever park it, `collect_slot_garbage` reads its directory as
            // unowned, and the agent is told it is isolated. Give the slot
            // and its empty branch back, and refuse the launch.
            Err(error) => {
                let Some(isolation) = taken.as_ref().and_then(Agent::isolation) else {
                    return Err(refused(format!(
                        "the agents recorded here could not be read: {error}"
                    )));
                };
                let _ = checkout::discard(&primary, isolation);
                return Err(refused(format!(
                    "the agent's task could not be recorded: {error}"
                )));
            }
        };

        // Outside the document's lock on purpose: the project's `setup` is
        // the one unbounded thing a launch runs, and every other mutation
        // would wait behind it.
        let isolation = task
            .isolation()
            .expect("an agent placed in a slot carries its isolation");
        let mut warnings = sync
            .concern(&isolation.target)
            .into_iter()
            .collect::<Vec<_>>();
        warnings.extend(checkout::materialize(&primary, &acquired.path, &policy));
        let view = self.placed_view(&primary, task.id.as_str(), &policy);
        Ok(AgentPlacement {
            project: primary.clone(),
            cwd: acquired.path,
            placement: Placement::Isolated {
                task: task.id,
                checkout: acquired.id,
                branch: acquired.branch,
                reused: !acquired.created,
            },
            warnings,
            view,
        })
    }

    /// Gives one agent a checkout of its own, keeping everything that
    /// makes it that agent: its identity, its harness, and the
    /// conversation it is in.
    ///
    /// The branch is cut from where the agent stood — the root's current
    /// `HEAD`, not the target — because "isolate" promises the same work
    /// somewhere of its own rather than a fresh start, and changes
    /// carried into the checkout only mean anything against the base they
    /// were made on.
    ///
    /// A process cannot be moved between directories, so what this
    /// answers is a placement: the caller relaunches the agent there,
    /// resuming its conversation, and closes the tab it took over from.
    /// Nothing is written to the operator's own working tree, whatever
    /// `carry` says.
    #[tracing::instrument(name = "workspace.isolate", skip_all, fields(cwd = %cwd.display(), agent = %agent_id, ?carry), err)]
    pub fn isolate(
        &self,
        cwd: &Path,
        agent_id: &str,
        carry: Carry,
        occupied: &[PathBuf],
    ) -> Result<AgentPlacement> {
        let refused = |reason: String| UzeError::AgentPlacement(reason);
        let (primary, policy) = self
            .repository_context(cwd)
            .ok_or_else(|| refused("not inside a Git working tree".to_owned()))?;
        let target = target_of(&primary, &policy);
        let base = checkout::current_branch(&primary).unwrap_or_else(|| target.clone());
        let base_tip = checkout::tip_of(&primary, "HEAD");
        if base_tip.is_empty() {
            return Err(refused("no commit to branch from".to_owned()));
        }

        let mut taken: Option<Isolation> = None;
        let recorded = task::locked(&self.0.home, &primary, |store| {
            checkout::reconcile(&primary, store, &target);
            let Some(agent) = store.agent_mut(agent_id) else {
                return Ok(Err("this agent is not one UZE launched here".to_owned()));
            };
            if agent.is_isolated() {
                return Ok(Err(
                    "this agent already has a checkout of its own".to_owned()
                ));
            }
            let id = agent.id.clone();
            let mut isolation = Isolation::cut(
                &id,
                Base::Ref(base.clone()),
                base_tip.clone(),
                target.clone(),
            );
            let acquired = match checkout::acquire(
                &primary,
                store,
                &isolation,
                &base_tip,
                policy.slots,
                &checkout::Presence::observe_with(occupied),
            ) {
                Ok(acquired) => acquired,
                Err(refusal) => return Ok(Err(refusal.to_string())),
            };
            isolation.checkout = Some(acquired.id.clone());
            taken = Some(isolation.clone());
            let branch = isolation.branch.clone();
            store
                .agent_mut(agent_id)
                .expect("the agent was just read under this lock")
                .isolation = Some(isolation);
            Ok(Ok((id, acquired, branch)))
        });

        let (id, acquired, branch) = match recorded {
            Ok(Ok(isolated)) => isolated,
            Ok(Err(refusal)) => return Err(refused(refusal)),
            // The slot was taken from Git before the record could be
            // written, so it is given back: an isolation nothing records
            // is a directory nobody owns and an agent told it is isolated
            // when it is not.
            Err(error) => {
                if let Some(isolation) = &taken {
                    let _ = checkout::discard(&primary, isolation);
                }
                return Err(refused(format!(
                    "the agent's isolation could not be recorded: {error}"
                )));
            }
        };

        // Outside the lock, like every other unbounded step: the
        // project's `setup` runs here, and so does the copy.
        let mut warnings = checkout::materialize(&primary, &acquired.path, &policy);
        if carry == Carry::CopyOfChanges
            && let Err(reason) = checkout::carry_changes(&primary, &acquired.path)
        {
            warnings.push(reason);
        }
        let view = self.placed_view(&primary, id.as_str(), &policy);
        Ok(AgentPlacement {
            project: primary.clone(),
            cwd: acquired.path,
            placement: Placement::Isolated {
                task: id,
                checkout: acquired.id,
                branch,
                reused: !acquired.created,
            },
            warnings,
            view,
        })
    }

    /// Puts a checkout back under a task that lost its own — removed
    /// outside UZE, or swept as idle — so a new agent can continue from
    /// where its branch stands. The task is live again in the slot this
    /// acquires; `occupied` is what [`Self::place_new_agent`] takes. A
    /// task that still has its checkout is answered with that checkout.
    #[tracing::instrument(name = "workspace.resume_task", skip_all, fields(cwd = %cwd.display(), task_id = %task_id), err)]
    pub fn resume_task(
        &self,
        cwd: &Path,
        task_id: &str,
        occupied: &[PathBuf],
    ) -> Result<AgentPlacement> {
        let (primary, policy) = self
            .repository_context(cwd)
            .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
        let target = target_of(&primary, &policy);
        let mut acquired_slot = None;
        let mut placement = task::locked(&self.0.home, &primary, |store| {
            checkout::reconcile(&primary, store, &target);
            let snapshot = store.clone();
            let agent = task_mut(store, task_id)
                .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
            let id = agent.id.clone();
            let Agent {
                state, isolation, ..
            } = agent;
            let task = isolation
                .as_mut()
                .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
            if let (Some(existing), Some(checkout)) =
                (landing::slot_path(&primary, task), task.checkout.clone())
            {
                return Ok(AgentPlacement {
                    project: primary.clone(),
                    cwd: existing,
                    placement: Placement::Isolated {
                        task: id,
                        checkout,
                        branch: task.branch.clone(),
                        reused: true,
                    },
                    warnings: Vec::new(),
                    view: None,
                });
            }
            let acquired = checkout::resume(
                &primary,
                &snapshot,
                task,
                policy.slots,
                &checkout::Presence::observe_with(occupied),
            )
            .map_err(|error| UzeError::ResumeFailed(error.to_string()))?;
            task.checkout = Some(acquired.id.clone());
            *state = WorkState::Running;
            let placement = Placement::Isolated {
                task: id,
                checkout: acquired.id.clone(),
                branch: acquired.branch.clone(),
                reused: !acquired.created,
            };
            acquired_slot = Some(acquired.clone());
            Ok(AgentPlacement {
                project: primary.clone(),
                cwd: acquired.path,
                placement,
                warnings: Vec::new(),
                view: None,
            })
        })?;
        // Preparing the checkout runs the project's `setup`; it waits for
        // nobody and nobody waits behind it.
        if let Some(acquired) = acquired_slot {
            placement.warnings = checkout::materialize(&primary, &acquired.path, &policy);
        }
        placement.view = self.placed_view(&primary, placement.placement.agent().as_str(), &policy);
        Ok(placement)
    }
}
