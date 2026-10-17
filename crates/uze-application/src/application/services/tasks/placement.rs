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
        // A manifest that does not parse says nothing about where the agent
        // belongs, so only a caller that named the kind still launches: one
        // landed in the operator's own tree when the project isolates is the
        // one placement a notice cannot undo.
        let (context, unread_policy) = match self.repository_context(pane_cwd) {
            Ok(context) => (context, None),
            Err(error) if kind.is_some() => (None, Some(error.to_string())),
            Err(error) => return Err(UzeError::AgentPlacement(error.to_string())),
        };
        let declared = context
            .as_ref()
            .map(|(_, policy)| policy.default)
            .unwrap_or_default();
        let kind = kind.unwrap_or(if declared.is_isolated() {
            PlacementKind::Isolated
        } else {
            PlacementKind::InPlace
        });
        let placed = match kind {
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
                let mut warnings = if in_the_primary {
                    self.policy_region_warnings(&root)
                } else {
                    Vec::new()
                };
                warnings.extend(unread_policy);
                Ok(AgentPlacement {
                    project: root.clone(),
                    cwd: root,
                    placement: Placement::InPlace { id: agent.id },
                    warnings,
                    told: None,
                    awaiting_approval: None,
                    view,
                    launch_key: String::new(),
                })
            }
        };
        self.keyed(placed?, None)
    }

    /// Brings the local target of the project at `cwd` in line with its
    /// remote, by fast-forward only, and fetches the remote's tip a new
    /// agent is cut from when the local target cannot move, so the next
    /// agent starts from what the team is on rather than from what this
    /// machine last saw.
    ///
    /// Asked on a clock rather than by each placement: the fetch is a
    /// network round trip, and in front of every new agent it was most of
    /// what the operator waited for. Only a project that isolates its
    /// agents asks — nothing is branched from the target in one that does
    /// not. A target that moved invalidates every remembered integration
    /// answer, so they are asked again here, where nobody waits for them,
    /// instead of by the next placement.
    #[tracing::instrument(name = "workspace.sync_target", skip_all, fields(cwd = %cwd.display()))]
    pub fn sync_target(&self, cwd: &Path) -> Option<TargetSyncReport> {
        let (primary, policy) = self.repository_context(cwd).ok().flatten()?;
        if !policy.default.is_isolated() {
            return None;
        }
        let target = declared_target(&primary, &policy)?;
        let sync = landing::sync_target(&primary, &target);
        if matches!(sync, landing::TargetSync::FastForwarded { .. })
            && let Ok(store) = task::load(&self.0.home, &primary)
        {
            checkout::slots(&primary, &store, &checkout::Presence::observe());
        }
        Some(TargetSyncReport {
            concern: sync.concern(&target),
            project: primary,
        })
    }

    /// Pulls the project's target into the checkout `cwd` belongs to, as
    /// the operator asked: fetched, then fast-forwarded and never anything
    /// else, through Git's own merge where they stand on it, so an
    /// uncommitted change in the way is refused rather than overwritten.
    #[tracing::instrument(name = "workspace.pull_target", skip_all, fields(cwd = %cwd.display()))]
    pub fn pull_target(&self, cwd: &Path) -> Option<PullReport> {
        let (primary, policy) = self.repository_context(cwd).ok().flatten()?;
        let target = declared_target(&primary, &policy)?;
        let pulled = match landing::pull_target(&primary, &target) {
            landing::TargetSync::FastForwarded { commits } => {
                if let Ok(store) = task::load(&self.0.home, &primary) {
                    checkout::slots(&primary, &store, &checkout::Presence::observe());
                }
                Ok(commits)
            }
            landing::TargetSync::Current => Ok(0),
            landing::TargetSync::Unpublished => Err(format!("{target} is not on the remote")),
            landing::TargetSync::Diverged { .. } => {
                Err(format!("{target} has commits the remote lacks"))
            }
            landing::TargetSync::Stalled { reason, .. } => Err(in_the_way(&reason)),
        };
        Some(PullReport {
            project: primary,
            target,
            pulled,
        })
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
        let target = declared_target(&primary, &policy)
            .ok_or_else(|| refused("the primary checkout is not on a branch".to_owned()))?;
        // The furthest tip of the target the last sync left
        // ([`Self::sync_target`]) — the remote's when it is ahead, which it
        // stays while the operator stands on the local target the sync never
        // moves. Asking the remote here put a network round trip in front of
        // every agent the operator creates.
        let base_tip = checkout::target_reach(&primary, &target, None)
            .ok_or_else(|| refused("no commit to branch from".to_owned()))?;
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
            let owner = agent.id.clone();
            let isolation = agent
                .isolation_mut()
                .expect("an agent built isolated carries its isolation");
            let acquired = match checkout::acquire(
                &primary,
                store,
                &owner,
                isolation,
                &base_tip,
                policy.slots,
                &|| checkout::Presence::observe_with(occupied),
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
                let agent_id = taken
                    .as_ref()
                    .map(|agent| agent.id.as_str().to_owned())
                    .unwrap_or_default();
                let _ = checkout::discard(&primary, &agent_id, isolation);
                return Err(refused(format!(
                    "the agent's task could not be recorded: {error}"
                )));
            }
        };

        // Outside the document's lock on purpose: the project's `setup` is
        // the one unbounded thing a launch runs, and every other mutation
        // would wait behind it.
        let consent = self.consent(&primary, &policy);
        let warnings = checkout::materialize(&primary, &acquired.path, &consent);
        let awaiting_approval = awaiting(&primary, &consent, &acquired.path);
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
            told: None,
            awaiting_approval,
            view,
            launch_key: String::new(),
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
            .repository_context(cwd)?
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
                &id,
                &isolation,
                &base_tip,
                policy.slots,
                &|| checkout::Presence::observe_with(occupied),
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
                    let _ = checkout::discard(&primary, agent_id, isolation);
                }
                return Err(refused(format!(
                    "the agent's isolation could not be recorded: {error}"
                )));
            }
        };

        // Outside the lock, like every other unbounded step: the
        // project's `setup` runs here, and so does the copy.
        let consent = self.consent(&primary, &policy);
        let mut warnings = checkout::materialize(&primary, &acquired.path, &consent);
        let awaiting_approval = awaiting(&primary, &consent, &acquired.path);
        if carry == Carry::CopyOfChanges
            && let Err(reason) = checkout::carry_changes(&primary, &acquired.path)
        {
            warnings.push(reason);
        }
        let view = self.placed_view(&primary, id.as_str(), &policy);
        self.keyed(
            AgentPlacement {
                project: primary.clone(),
                cwd: acquired.path,
                placement: Placement::Isolated {
                    task: id,
                    checkout: acquired.id,
                    branch,
                    reused: !acquired.created,
                },
                warnings,
                told: None,
                awaiting_approval,
                view,
                launch_key: String::new(),
            },
            None,
        )
    }

    /// Puts a checkout back under a task that lost its own — removed
    /// outside UZE, or swept as idle — so a new agent can continue from
    /// where its branch stands. The task is live again in the slot this
    /// acquires; `occupied` is what [`Self::place_new_agent`] takes. A
    /// task that still has its checkout is answered with that checkout.
    /// An agent that worked in the project's root comes back there, in the
    /// conversation it left. `harness` is the one this launch runs, which
    /// the record takes as the agent's from here on.
    #[tracing::instrument(name = "workspace.resume_task", skip_all, fields(cwd = %cwd.display(), task_id = %task_id, harness = %harness), err)]
    pub fn resume_task(
        &self,
        cwd: &Path,
        task_id: &str,
        harness: &str,
        occupied: &[PathBuf],
    ) -> Result<AgentPlacement> {
        if let Some(placed) = self.resume_in_the_root(cwd, task_id)? {
            return self.keyed(placed, Some(harness));
        }
        let (primary, policy) = self
            .repository_context(cwd)?
            .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
        let target = target_of(&primary, &policy);
        let observe = || checkout::Presence::observe_with(occupied);
        let mut acquired_slot = None;
        let mut restored: Vec<(String, String)> = Vec::new();
        let mut placement = task::locked(&self.0.home, &primary, |store| {
            checkout::reconcile(&primary, store, &target);
            let snapshot = store.clone();
            let agent = task_mut(store, task_id)
                .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
            let id = agent.id.clone();
            agent.revive();
            let Agent {
                state, isolation, ..
            } = agent;
            let task = isolation
                .as_mut()
                .ok_or_else(|| UzeError::UnknownTask(task_id.to_owned()))?;
            // Answered with the checkout it has only while that checkout
            // is still on its branch — or rebasing it, paused — since a
            // slot released under a record that was not rewritten is
            // somebody else's, detached and clean.
            if let (Some(existing), Some(checkout)) =
                (landing::slot_path(&primary, task), task.checkout.clone())
                && checkout::branch_at_work(&existing).as_deref() == Some(task.branch.as_str())
            {
                // Pinned when it ended, so it kept the checkout and was
                // recorded unfinished; somebody is at work in it again.
                if *state == WorkState::Shelved {
                    *state = WorkState::Running;
                }
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
                    told: None,
                    awaiting_approval: None,
                    view: None,
                    launch_key: String::new(),
                });
            }
            // An agent that ended with nothing to deliver, or whose work
            // the target already carries, left a conversation and no
            // branch: it comes back on the target as a new agent would, so
            // nothing is measured against a base its branch never had.
            let fresh = (matches!(*state, WorkState::Closed | WorkState::Integrated)
                && !checkout::branch_exists(&primary, &task.branch))
            .then(|| checkout::target_reach(&primary, &target, None))
            .flatten();
            if let Some(tip) = &fresh {
                task.base_commit = tip.clone();
            }
            let acquired = checkout::resume(
                &primary,
                &snapshot,
                &id,
                task,
                policy.slots,
                &observe,
                fresh.as_deref(),
            )
            .map_err(|error| UzeError::ResumeFailed(error.to_string()))?;
            if let Some(shelf) = checkout::shelf::shelf_of(&primary, id.as_str()) {
                put_back(&primary, &acquired.path, &shelf)?;
                restored.push((id.as_str().to_owned(), shelf));
            }
            let mut taken = vec![acquired.path.clone()];
            task.checkout = Some(acquired.id.clone());
            task.last_checkout = None;
            *state = WorkState::Running;
            let placement = Placement::Isolated {
                task: id.clone(),
                checkout: acquired.id.clone(),
                branch: acquired.branch.clone(),
                reused: !acquired.created,
            };
            acquired_slot = Some(acquired.clone());
            // All or nothing: the record is not written when this fails,
            // and a slot left holding restored work under a record that
            // never names it would be adopted and shelved again as a task
            // of its own, while the shelf it came from still stands.
            match resume_children(&primary, store, &id, policy.slots, &observe, &mut taken) {
                Ok(children) => restored.extend(children),
                Err(error) => {
                    for slot in &taken {
                        checkout::untake(&primary, slot);
                    }
                    return Err(error);
                }
            }
            Ok(AgentPlacement {
                project: primary.clone(),
                cwd: acquired.path,
                placement,
                warnings: Vec::new(),
                told: None,
                awaiting_approval: None,
                view: None,
                launch_key: String::new(),
            })
        })?;
        // Only once the record says the work is back: a shelf removed before
        // that, and a record that failed to write, would leave the work in a
        // checkout nothing names.
        for (task, shelf) in restored {
            let _ = checkout::shelf::drop_shelf(&primary, &task, &shelf);
        }
        // Preparing the checkout runs the project's `setup`; it waits for
        // nobody and nobody waits behind it.
        if let Some(acquired) = acquired_slot {
            let consent = self.consent(&primary, &policy);
            placement.warnings = checkout::materialize(&primary, &acquired.path, &consent);
            if acquired.displaced {
                placement
                    .warnings
                    .insert(0, checkout::CONVERSATION_NOT_CARRIED.to_owned());
            }
            placement.awaiting_approval = awaiting(&primary, &consent, &acquired.path);
        }
        placement.view = self.placed_view(&primary, placement.placement.agent().as_str(), &policy);
        self.keyed(placement, Some(harness))
    }

    /// The placement of `task_id` back in `root` when it is an agent that
    /// worked there, keyed by that directory; `None` for any other.
    fn resume_in_the_root(&self, root: &Path, task_id: &str) -> Result<Option<AgentPlacement>> {
        let root = canonical(root);
        if !task::store_path(&self.0.home, &root).exists() {
            return Ok(None);
        }
        let revived = task::locked(&self.0.home, &root, |store| {
            let Some(agent) = store
                .agent_mut(task_id)
                .filter(|agent| !agent.is_isolated())
            else {
                return Ok(None);
            };
            agent.revive();
            Ok(Some(agent.id.clone()))
        })?;
        let Some(id) = revived else {
            return Ok(None);
        };
        let view = self
            .repository_context(&root)
            .ok()
            .flatten()
            .and_then(|(primary, policy)| self.placed_view(&primary, id.as_str(), &policy));
        Ok(Some(AgentPlacement {
            project: root.clone(),
            cwd: root,
            placement: Placement::InPlace { id },
            warnings: Vec::new(),
            told: None,
            awaiting_approval: None,
            view,
            launch_key: String::new(),
        }))
    }

    /// Issues the placed agent's launch its secret: every placement is a
    /// launch, and each one makes the secret before it the agent's no
    /// longer. Refused, and nothing launched, when the record cannot take
    /// it — an agent nobody can prove to be could not name or deliver its
    /// work. `harness`, for a launch that chose one, becomes the agent's in
    /// the same write: the harness an agent runs is the one its latest
    /// launch was keyed for.
    fn keyed(&self, mut placed: AgentPlacement, harness: Option<&str>) -> Result<AgentPlacement> {
        let agent = placed.placement.agent().as_str().to_owned();
        placed.launch_key = task::locked(&self.0.home, &placed.project, |store| {
            let record = store
                .agent_mut(&agent)
                .ok_or_else(|| UzeError::UnknownTask(agent.clone()))?;
            if let Some(harness) = harness {
                record.harness = harness.to_owned();
            }
            record.issue_launch_key().map_err(|error| {
                UzeError::AgentPlacement(format!(
                    "the agent's launch could not be issued a key: {error}"
                ))
            })
        })?;
        Ok(placed)
    }
}

/// What a checkout placed without its approved commands carries, for the
/// operator to answer.
fn awaiting(
    primary: &Path,
    consent: &uze_workspace::approval::Consent<'_>,
    checkout: &Path,
) -> Option<CommandsAwaitingApproval> {
    consent.awaiting().map(|awaiting| {
        CommandsAwaitingApproval::of(primary, awaiting, Some(checkout.to_path_buf()))
    })
}

/// Puts `shelf` back in the checkout just taken for it, or gives the
/// checkout back and refuses, naming what conflicts: a resume that cannot
/// bring the work back is not a resume.
fn put_back(primary: &Path, slot: &Path, shelf: &str) -> Result<()> {
    match checkout::shelf::restore(slot, shelf) {
        Ok(checkout::shelf::Restoring::Restored) => Ok(()),
        Ok(checkout::shelf::Restoring::Conflict(files)) => {
            checkout::untake(primary, slot);
            let files: Vec<String> = files
                .iter()
                .map(|file| file.display().to_string())
                .collect();
            Err(UzeError::ResumeFailed(format!(
                "its unfinished work no longer applies where its branch stands: {}",
                files.join(", ")
            )))
        }
        Err(reason) => {
            checkout::untake(primary, slot);
            Err(UzeError::ResumeFailed(reason))
        }
    }
}

/// Places every shelved child of `parent` back in a checkout of its own,
/// as that agent's child under the topic it had, with its shelf restored,
/// so the agent can join it. Answers the shelves restored.
fn resume_children(
    primary: &Path,
    store: &mut AgentStore,
    parent: &AgentId,
    cap: Option<usize>,
    observe: &dyn Fn() -> checkout::Presence,
    taken: &mut Vec<PathBuf>,
) -> Result<Vec<(String, String)>> {
    let mut restored = Vec::new();
    let children: Vec<AgentId> = store
        .agents
        .iter()
        .filter(|agent| agent.parent.as_ref() == Some(parent))
        .filter(|agent| {
            agent
                .isolation()
                .is_some_and(|isolation| isolation.checkout.is_none())
        })
        .map(|agent| agent.id.clone())
        .collect();
    for child_id in children {
        let current = store.clone();
        let child = task_mut(store, child_id.as_str()).expect("listed from this store");
        let has_shelf = checkout::shelf::shelf_of(primary, child_id.as_str());
        let isolation = child.isolation_mut().expect("filtered to isolated");
        let holds_commits = checkout::branch_exists(primary, &isolation.branch)
            && !checkout::is_integrated(primary, &isolation.target, &isolation.branch);
        if has_shelf.is_none() && !holds_commits {
            continue;
        }
        let acquired =
            checkout::resume(primary, &current, &child_id, isolation, cap, observe, None)
                .map_err(|error| UzeError::ResumeFailed(error.to_string()))?;
        taken.push(acquired.path.clone());
        checkout::record::write(
            primary,
            &acquired.path,
            &checkout::record::CheckoutRecord {
                parent: Some(parent.clone()),
                split_at: Some(isolation.base_commit.clone()),
                ..checkout::record::CheckoutRecord::made_at(&acquired.path)
            },
        )
        .map_err(UzeError::ResumeFailed)?;
        if let Some(shelf) = has_shelf {
            put_back(primary, &acquired.path, &shelf)?;
            restored.push((child_id.as_str().to_owned(), shelf));
        }
        isolation.checkout = Some(acquired.id);
        isolation.last_checkout = None;
        child.state = WorkState::Running;
    }
    Ok(restored)
}

/// Git's refusal to fast-forward over uncommitted changes, said as the
/// files in the way: its own message is a paragraph whose first line is
/// all a toast has room for, and that line names none of them. Any other
/// reason is passed on as it is.
pub(super) fn in_the_way(reason: &str) -> String {
    if !reason.contains("would be overwritten") {
        return reason.to_owned();
    }
    let files: Vec<&str> = reason
        .lines()
        .filter(|line| line.starts_with('\t'))
        .map(str::trim)
        .collect();
    if files.is_empty() {
        return reason.to_owned();
    }
    format!("uncommitted: {}", files.join(", "))
}
