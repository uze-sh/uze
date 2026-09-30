//! The workspace service: slots, tasks, delivery, and the read models
//! presentation sees them through.
//!
//! Split out of `services.rs`, which had grown to carry eight capability
//! views plus every read model the largest of them answers with. This is
//! that largest one — the only service with a domain of its own rather
//! than a thin route into `uze-core`, which is why it is the one that
//! became a file.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use uze_core::{Result, UzeError, anchor, manifest};

use uze_workspace::{
    checkout, client_layout,
    conversation::{self, Claim},
    landing::{self, Delivered, DeliveryFailure, Forge, Readiness},
    prompt_history,
    task::{self, Agent, AgentId, AgentStore, Base, Isolation, WorkState},
    worktree::{self, BranchVocabulary, CompletionBehavior, NameRefusal, WorktreePolicy},
};

use super::{AgentIdentity, Workspace};

impl Workspace<'_> {
    /// The workspace root a directory belongs to, or the directory itself.
    ///
    /// One repository is one terminal server, and this is the answer both
    /// the server key and the prompt history are keyed on — resolved once,
    /// here, rather than twice at two call sites.
    #[tracing::instrument(name = "workspace.root", skip_all, fields(cwd = %cwd.display()))]
    pub fn root(&self, cwd: &Path) -> PathBuf {
        anchor::anchor_root_or_self(cwd)
    }

    /// The harnesses this installation can recognize, as descriptors.
    #[tracing::instrument(name = "workspace.agent_identities", skip_all)]
    pub fn agent_identities(&self) -> Vec<AgentIdentity> {
        self.0
            .integrations
            .iter()
            .map(|integration| {
                let binary = integration
                    .aliases()
                    .first()
                    .copied()
                    .unwrap_or(integration.id());
                let (launch, continuity_gap) = self.launcher(integration.as_ref(), binary);
                AgentIdentity {
                    binary,
                    integration: integration.id(),
                    display_name: integration.display_name(),
                    launch,
                    continuity_gap,
                    configured: integration.status(&self.0.home)
                        != uze_core::integration::IntegrationStatus::NotConfigured,
                }
            })
            .collect()
    }

    /// What to launch an agent of `integration` by, and what that costs.
    ///
    /// UZE's own launcher is what decides, per launch, whether an agent
    /// resumes its task's conversation or starts one, so naming it here by
    /// path is what makes continuity independent of the operator's `PATH`.
    /// It is never created on their behalf: the launcher's presence is the
    /// operator's own opt-in, and resurrecting one they removed would
    /// override a decision they made. Without it the harness still starts —
    /// on its plain name, with no conversation carried over, and the reason
    /// said rather than silently missing.
    fn launcher(
        &self,
        integration: &dyn uze_core::integration::IntegrationPort,
        binary: &str,
    ) -> (PathBuf, Option<String>) {
        let bare = PathBuf::from(binary);
        if integration.session_continuity() == uze_core::integration::SessionContinuity::Unsupported
        {
            return (
                bare,
                Some("this harness offers no way to continue a conversation".to_owned()),
            );
        }
        let launcher = self.0.home.shims_dir().join(integration.shim_name());
        if launcher.exists() {
            return (launcher, None);
        }
        (
            bare,
            Some(
                "UZE's launcher is not installed for this harness, so its conversation is not \
                 carried over"
                    .to_owned(),
            ),
        )
    }

    /// Writes back which conversation the agent a claim names is actually
    /// in.
    ///
    /// Answers whether anything changed. Runs off whatever thread the
    /// caller gives it — one of these asks a harness about its own
    /// records, which can mean spawning it — and is silent about every
    /// way of having nothing to say.
    #[tracing::instrument(name = "workspace.refresh_conversation", skip_all, fields(integration = %integration, agent = %claim.id, cwd = %claim.cwd.display()))]
    pub fn refresh_conversation(&self, integration: &str, claim: Claim<'_>) -> bool {
        self.0
            .integrations
            .iter()
            .find(|candidate| candidate.id() == integration)
            .is_some_and(|integration| {
                uze_workspace::continuity::refresh(&self.0.home, claim, integration.as_ref())
            })
    }

    /// Recent prompts submitted into the agent tabs of `root`'s workspace.
    #[tracing::instrument(name = "workspace.prompt_history", skip_all, fields(root = %root.display(), limit))]
    pub fn prompt_history(&self, root: &Path, limit: usize) -> Vec<prompt_history::PromptEntry> {
        prompt_history::list_for_workspace(&self.0.home, root, limit)
    }

    /// Records one prompt submitted into an agent tab of `root`'s
    /// workspace. Best-effort by construction: an empty prompt is ignored
    /// rather than refused.
    ///
    /// The span carries the prompt's length, never the prompt. What this
    /// writes is `prompt-history.json`, which [`prompt_history`] opens and
    /// keeps at `0600` on purpose; the journal beside it is world-readable
    /// and is attached to bug reports. A field interpolating the text put
    /// the same bytes in both places, and only one of them was protected.
    #[tracing::instrument(
        name = "workspace.record_prompt",
        skip_all,
        fields(root = %root.display(), bytes = prompt.len()),
        err
    )]
    pub fn record_prompt(
        &self,
        root: &Path,
        origin: &prompt_history::PromptOrigin,
        prompt: &str,
    ) -> Result<()> {
        prompt_history::record(&self.0.home, root, origin, prompt)
    }

    /// Forgets every prompt recorded for `root`'s workspace.
    #[tracing::instrument(name = "workspace.clear_prompt_history", skip_all, fields(root = %root.display()), err)]
    pub fn clear_prompt_history(&self, root: &Path) -> Result<()> {
        prompt_history::clear(&self.0.home, root)
    }

    /// What the TUI was last left looking like, in both of its modes.
    /// Best-effort: unreadable state answers with the defaults rather
    /// than failing.
    #[tracing::instrument(name = "workspace.client_layout", skip_all)]
    pub fn client_layout(&self) -> client_layout::ClientLayout {
        client_layout::load(&self.0.home)
    }

    /// Remembers the TUI's shape for the next run.
    #[tracing::instrument(name = "workspace.save_client_layout", skip_all, err)]
    pub fn save_client_layout(&self, layout: &client_layout::ClientLayout) -> Result<()> {
        client_layout::save(&self.0.home, layout)
    }

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

    fn place_in_slot(
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
        warnings.extend(checkout::materialize(
            &primary,
            &acquired.path,
            &policy.link,
            &policy.setup,
        ));
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
        let mut warnings =
            checkout::materialize(&primary, &acquired.path, &policy.link, &policy.setup);
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
            placement.warnings =
                checkout::materialize(&primary, &acquired.path, &policy.link, &policy.setup);
        }
        placement.view = self.placed_view(&primary, placement.placement.agent().as_str(), &policy);
        Ok(placement)
    }

    /// Names the work of the agent a claim identifies.
    ///
    /// The claim is verified the way every reader verifies one: the store
    /// names its identifier and the directory is the record's own. The
    /// stamp says which agent and the directory says where, so a process
    /// editing its own environment cannot reach another agent's branch.
    /// An agent that is not isolated takes the name as its label alone:
    /// it works on the operator's branch, which already has the name it
    /// will keep, so there is nothing in Git to rename — but the operator
    /// still reads a tab for it, and "agent 3" says nothing about the work.
    ///
    /// Asking renames, however often it is asked. A branch is an address,
    /// and the work it holds turns out to be something else often enough
    /// that naming it once was never the realistic case — what refusing a
    /// second name actually stranded was a branch UZE had derived from a
    /// commit, because nothing downstream could tell that name from one an
    /// agent had chosen.
    ///
    /// Renaming to the name it already carries is not an error — the
    /// record is confirmed and Git is left alone — so an agent may state
    /// its branch's name without first asking what it is.
    #[tracing::instrument(name = "workspace.name_task", skip_all, fields(agent = %claim.id, cwd = %claim.cwd.display(), proposed = %proposed), err)]
    pub fn name_task(&self, claim: Claim<'_>, proposed: &str) -> Result<NamedTask> {
        let not_an_agent =
            || UzeError::TaskNaming("this process is not an agent UZE launched".to_owned());
        let owner = conversation::owner_of(&self.0.home, claim).ok_or_else(not_an_agent)?;
        if !owner.isolated {
            return self.label_in_place(&owner, proposed);
        }
        let (primary, policy) = self
            .repository_context(claim.cwd)
            .ok_or_else(|| UzeError::TaskNaming("not inside a Git working tree".to_owned()))?;
        let vocabulary = policy.branch.clone();
        let branch = vocabulary
            .accept(proposed)
            .map_err(|refusal| UzeError::TaskNaming(refusal_words(&refusal, &vocabulary)))?;
        let target = target_of(&primary, &policy);
        task::locked(&self.0.home, &primary, |store| {
            checkout::reconcile(&primary, store, &target);
            let agent = store.get_mut(&owner.agent).ok_or_else(not_an_agent)?;
            let task = agent.isolation().ok_or_else(not_an_agent)?;
            if checkout::current_branch(claim.cwd).as_deref() != Some(task.branch.as_str()) {
                return Err(UzeError::TaskNaming(
                    "this checkout is not on the task's branch — finish the rebase first"
                        .to_owned(),
                ));
            }
            if task.branch != branch {
                if checkout::branch_exists(&primary, &branch) {
                    return Err(UzeError::TaskNaming(format!(
                        "`{branch}` already exists in this repository"
                    )));
                }
                checkout::rename_branch(&primary, &task.branch.clone(), &branch)?;
            }
            agent.take_name(branch.clone());
            let agent_id = agent.id.clone();
            for child in store.agents.iter_mut() {
                if child.parent.as_ref() == Some(&agent_id)
                    && let Some(isolation) = child.isolation_mut()
                {
                    isolation.target = branch.clone();
                }
            }
            let agent = store.get(&agent_id).ok_or_else(not_an_agent)?;
            Ok(NamedTask {
                task: agent.id.as_str().to_owned(),
                branch: Some(branch),
                label: agent.label.clone(),
            })
        })
    }

    /// Names an agent in the operator's checkout: judged by the same
    /// vocabulary as a branch, so the label reads the same wherever the
    /// agent was placed, and recorded without touching Git — the branch
    /// is the operator's. The policy is read from the root the record is
    /// keyed on, which needs no repository.
    fn label_in_place(&self, owner: &conversation::Owner, proposed: &str) -> Result<NamedTask> {
        let vocabulary = self.policy(&owner.project_root)?.branch;
        let name = vocabulary
            .accept(proposed)
            .map_err(|refusal| UzeError::TaskNaming(refusal_words(&refusal, &vocabulary)))?;
        task::locked(&self.0.home, &owner.project_root, |store| {
            let agent = store.get_mut(&owner.agent).ok_or_else(|| {
                UzeError::TaskNaming("this process is not an agent UZE launched".to_owned())
            })?;
            agent.take_label(&name);
            Ok(NamedTask {
                task: agent.id.as_str().to_owned(),
                branch: None,
                label: agent.label.clone(),
            })
        })
    }

    /// The project's declared policy, or the defaults when its manifest
    /// declares none. Read from the primary checkout on purpose: a worktree
    /// never declares a policy of its own, and nothing machine-scoped
    /// participates, so the same repository resolves identically everywhere.
    /// A malformed manifest is an error rather than a silent default.
    pub(super) fn policy(&self, primary: &Path) -> Result<WorktreePolicy> {
        uze_workspace::declaration::policy(primary)
    }

    /// The row a placement answers with: the agent just recorded, read
    /// back and derived exactly as an evaluation derives every other row.
    ///
    /// One derivation, so the row a launch draws and the row the first
    /// evaluation replaces it with can never disagree about which group
    /// the agent is in. Without it a client has only the directory to go
    /// on until that evaluation lands — a Git pass over the whole
    /// repository — and draws a freshly isolated agent among the ones
    /// sharing the operator's checkout for as long as it takes.
    fn placed_view(&self, primary: &Path, id: &str, policy: &WorktreePolicy) -> Option<AgentView> {
        let target = target_of(primary, policy);
        let store = task::load(&self.0.home, primary).ok()?;
        AgentView::from_agent(
            primary,
            store.agent(id)?,
            policy.completion,
            &target,
            landing::forge(primary),
        )
    }

    /// The repository `cwd` belongs to, with its policy and recorded tasks.
    fn repository(&self, cwd: &Path) -> Option<Repository> {
        self.open(cwd).ok().flatten()
    }

    /// The same without the tasks: what a *mutation* resolves before it
    /// takes the document, so the one read it acts on is the one
    /// [`task::locked`] takes under the lock rather than an earlier one
    /// somebody else has since replaced.
    pub(super) fn repository_context(&self, cwd: &Path) -> Option<(PathBuf, WorktreePolicy)> {
        let primary = worktree::primary_checkout(cwd)?;
        let policy = self.policy(&primary).ok()?;
        Some((primary, policy))
    }

    /// The same, telling "there is no repository here" apart from "there
    /// is one and its recorded tasks could not be read" — the second is
    /// the condition every caller used to render as the first.
    fn open(&self, cwd: &Path) -> std::result::Result<Option<Repository>, String> {
        let Some(primary) = worktree::primary_checkout(cwd) else {
            return Ok(None);
        };
        let Ok(policy) = self.policy(&primary) else {
            return Ok(None);
        };
        let store = task::load(&self.0.home, &primary).map_err(|error| error.to_string())?;
        Ok(Some(Repository {
            primary,
            policy,
            store,
        }))
    }

    /// The primary checkout `cwd` belongs to — the key every task view
    /// hangs off — or `None` outside a Git working tree.
    #[tracing::instrument(name = "workspace.primary_of", skip_all, fields(cwd = %cwd.display()))]
    pub fn primary_of(&self, cwd: &Path) -> Option<PathBuf> {
        worktree::primary_checkout(cwd)
    }

    /// The branch checked out where `cwd` sits — what an agent working
    /// outside any slot is on — or `None` for a detached `HEAD` or no
    /// repository at all.
    #[tracing::instrument(name = "workspace.current_branch", skip_all, fields(cwd = %cwd.display()))]
    pub fn current_branch(&self, cwd: &Path) -> Option<String> {
        checkout::current_branch(cwd)
    }

    /// How the branch checked out at `cwd` stands against its upstream —
    /// what a pull would bring and a push would send — when that branch
    /// is the repository's delivery target. `None` on any other branch,
    /// and without an upstream to measure against: an agent on a branch
    /// of its own reaches the remote through the target, so the target
    /// is the one branch whose sync with it is worth a caption.
    #[tracing::instrument(name = "workspace.target_upstream_sync", skip_all, fields(cwd = %cwd.display()))]
    pub fn target_upstream_sync(&self, cwd: &Path) -> Option<UpstreamSync> {
        let repository = self.repository(cwd)?;
        if checkout::current_branch(cwd)? != repository.target() {
            return None;
        }
        let divergence = checkout::upstream_divergence(cwd)?;
        Some(UpstreamSync {
            pull: divergence.behind,
            push: divergence.ahead,
        })
    }

    /// Every piece of work on this machine that no live agent is in front
    /// of, whichever project it belongs to.
    ///
    /// Answers from UZE's own records of every project it has recorded an
    /// agent for — not from the projects this session happens to have
    /// opened — so a space closed by accident does not take its agents'
    /// work out of the one surface that exists to find it again.
    ///
    /// Liveness is not asked here. Which agents a client is in front of is
    /// the client's own question, and it answers it by launch stamp; this
    /// hands over everything still preserved and lets the caller subtract.
    ///
    /// A project whose records cannot be read is skipped rather than
    /// refused: withholding every other project's work because one
    /// document is unreadable is the failure this list exists to prevent.
    #[tracing::instrument(name = "workspace.preserved_work", skip_all)]
    pub fn preserved_work(&self) -> Vec<PreservedWork> {
        let mut preserved: Vec<PreservedWork> = uze_core::record::roots(&self.0.home)
            .into_iter()
            .flat_map(|project| {
                let agents = task::load(&self.0.home, &project)
                    .map(|store| store.agents)
                    .unwrap_or_default();
                agents.into_iter().filter_map({
                    let project = project.clone();
                    move |agent| PreservedWork::from_agent(&project, &agent)
                })
            })
            .collect();
        preserved.sort_by(|left, right| {
            left.created_at_unix
                .cmp(&right.created_at_unix)
                .then_with(|| left.id.cmp(&right.id))
        });
        preserved
    }

    /// Every task recorded for `cwd`'s repository, as last evaluated.
    #[tracing::instrument(name = "workspace.tasks", skip_all, fields(cwd = %cwd.display()))]
    pub fn tasks(&self, cwd: &Path) -> Vec<AgentView> {
        self.repository(cwd)
            .map(|repository| repository.views())
            .unwrap_or_default()
    }

    /// The project's say in delivery, for a header to name what `deliver`
    /// will do.
    /// What declaring a completion behavior would do, so a caller can say
    /// it before doing it rather than after. Writing the policy touches a
    /// *tracked* file — the one the whole team reads — and the projected
    /// `AGENTS.md` still needs `uze agent context reconcile` to follow it, so a
    /// click that silently did both would be a click nobody could predict.
    #[tracing::instrument(name = "workspace.completion_change_consequence", skip_all, fields(cwd = %cwd.display()))]
    pub fn completion_change_consequence(&self, cwd: &Path) -> Option<PolicyWriteConsequence> {
        let repository = self.repository(cwd)?;
        let manifest = manifest::manifest_path_for(&repository.primary);
        Some(PolicyWriteConsequence {
            creates_manifest: !manifest.exists(),
            manifest,
        })
    }

    /// Declares the completion behavior for the repository `cwd` belongs
    /// to, creating `agents.yaml` when the project has none. Reports
    /// whether it created it, so the caller can say which of the two
    /// things just happened.
    ///
    /// The policy is the primary checkout's, always: an isolated checkout
    /// declaring one of its own would be a per-worktree policy, which
    /// there is deliberately none of.
    #[tracing::instrument(name = "workspace.set_completion", skip_all, fields(cwd = %cwd.display()), err)]
    pub fn set_completion(&self, cwd: &Path, behavior: CompletionBehavior) -> Result<bool> {
        // `MissingPath` is what resolving a project root already answers
        // with when there is nothing to resolve; a policy is a repository's,
        // and outside one there is no primary checkout to declare it in.
        let primary = self
            .repository(cwd)
            .map(|repository| repository.primary)
            .ok_or_else(|| UzeError::MissingPath(cwd.to_path_buf()))?;
        uze_workspace::declaration::set_completion(&primary, behavior)
    }

    #[tracing::instrument(name = "workspace.delivery_policy", skip_all, fields(cwd = %cwd.display()))]
    pub fn delivery_policy(&self, cwd: &Path) -> Option<DeliveryPolicyView> {
        let repository = self.repository(cwd)?;
        Some(DeliveryPolicyView {
            completion: repository.policy.completion.abi_name(),
            target: repository
                .policy
                .target
                .clone()
                .or_else(|| checkout::current_branch(&repository.primary)),
            gate: repository.policy.gate.clone(),
        })
    }

    /// Re-reads every live task's state from its checkout — what the
    /// sidebar shows after an agent's pane goes quiet — and lets a clean,
    /// live task follow a target that moved. A conflict that produces
    /// returns to the owning agent as a notice for its pane.
    #[tracing::instrument(name = "workspace.evaluate_tasks", skip_all, fields(cwd = %cwd.display()))]
    pub fn evaluate_tasks(&self, cwd: &Path, occupied: &[PathBuf]) -> Evaluation {
        let Some((primary, policy)) = self.repository_context(cwd) else {
            return Evaluation::default();
        };
        let target = target_of(&primary, &policy);
        let mut notices = Vec::new();
        let mut ask_the_remote: Vec<(AgentId, Isolation)> = Vec::new();
        let completion = policy.completion;
        let evaluated = task::locked_reporting(&self.0.home, &primary, |store| {
            checkout::reconcile(&primary, store, &target);
            let pass = EvaluationPass {
                primary: &primary,
                target: &target,
                in_the_root: OnceCell::new(),
                occupied,
                owners: store.slot_owners(),
                holding_children: store
                    .agents
                    .iter()
                    .filter(|agent| {
                        checkout::is_live(&agent.state) || agent.state == WorkState::Parked
                    })
                    .filter_map(|agent| agent.parent.clone())
                    .collect(),
                vocabulary: &policy.branch,
                completion,
            };
            // Every agent, not only the isolated ones: where the work
            // stands is a fact about the checkout an agent sits in, and
            // one in the project's own root sits in a checkout too.
            // A subagent's checkout answers to its agent alone: nothing
            // is ready, named, followed or delivered there.
            for agent in store
                .agents
                .iter_mut()
                .filter(|agent| agent.parent.is_none())
            {
                if let Some(read) = pass.evaluate(agent) {
                    ask_the_remote.extend(read.ask_the_remote);
                    notices.extend(read.notice);
                }
            }
            Ok(task_views(&primary, store, completion, &target))
        });
        let (tasks, recovery) = match evaluated {
            Ok(answered) => answered,
            Err(error) => {
                return Evaluation {
                    unreadable: Some(error.to_string()),
                    ..Evaluation::default()
                };
            }
        };
        let mut evaluation = Evaluation {
            tasks,
            notices,
            unreadable: None,
            recovered: recovery.set_aside.map(|set_aside| {
                format!(
                    "the agents recorded here could not be read — set aside at {}",
                    set_aside.path.display()
                )
            }),
        };
        self.adopt_observed_requests(
            &primary,
            completion,
            &target,
            &ask_the_remote,
            &mut evaluation,
        );
        evaluation
    }

    /// Asks the remote about each task's request with the document
    /// unlocked, then takes the lock once to write the answers down and
    /// re-read the views, so a request discovered here is in the answer
    /// this pass gives rather than in the next one's.
    fn adopt_observed_requests(
        &self,
        primary: &Path,
        completion: CompletionBehavior,
        target: &str,
        asked: &[(AgentId, Isolation)],
        evaluation: &mut Evaluation,
    ) {
        let observed: Vec<(&(AgentId, Isolation), landing::RequestObservation)> = asked
            .iter()
            .map(|asked| (asked, landing::observe_request(primary, &asked.1)))
            .collect();
        if observed.is_empty() {
            return;
        }
        let adopted = task::locked(&self.0.home, primary, |store| {
            for ((id, asked), observation) in &observed {
                // Only onto the record the question was asked about.
                // Anything that moved it since — a delivery that pushed
                // the branch and found the request itself — knows more
                // about this than an answer taken before it ran.
                if let Some(record) = task_mut(store, id.as_str()).and_then(Agent::isolation_mut)
                    && record.branch == asked.branch
                    && record.published_as == asked.published_as
                    && record.published_request == asked.published_request
                    && record.request_branch == asked.request_branch
                    && record.request_asked_at_unix == asked.request_asked_at_unix
                {
                    landing::adopt_request(record, observation);
                }
            }
            Ok(task_views(primary, store, completion, target))
        });
        match adopted {
            Ok(tasks) => evaluation.tasks = tasks,
            // The pass itself stands — its states were written, and the
            // views already say them. What is lost is a request number the
            // next pass asks for again, which is not worth a notice in an
            // agent's pane, the only channel an evaluation has.
            Err(error) => {
                tracing::warn!(%error, "the remote's answer about open requests was not recorded")
            }
        }
    }

    /// Delivers one task the way the project's completion says, one task
    /// at a time under the repository write lock.
    #[tracing::instrument(name = "workspace.deliver_task", skip_all, fields(cwd = %cwd.display(), task_id = %task_id))]
    pub fn deliver_task(&self, cwd: &Path, task_id: &str) -> Option<DeliveryReport> {
        let (primary, policy) = self.repository_context(cwd)?;
        self.deliver_claimed(&primary, &policy, task_id)
    }

    /// Delivers every ready task, oldest first; the second sees the first.
    #[tracing::instrument(name = "workspace.deliver_ready", skip_all, fields(cwd = %cwd.display()))]
    pub fn deliver_ready(&self, cwd: &Path) -> Vec<DeliveryReport> {
        let Some(repository) = self.repository(cwd) else {
            return Vec::new();
        };
        let mut ready: Vec<(u64, String)> = repository
            .store
            .isolated()
            .filter(|agent| agent.state == WorkState::Ready)
            .map(|agent| (agent.created_at_unix, agent.id.as_str().to_owned()))
            .collect();
        ready.sort();
        // One claim per task, in order, so the second is claimed against
        // what the first wrote: the listing is only a listing, and the
        // document decides under the lock each delivery takes for itself.
        ready
            .into_iter()
            .filter_map(|(_, id)| {
                self.deliver_claimed(&repository.primary, &repository.policy, &id)
            })
            .collect()
    }

    /// Delivers one recorded task, holding the tasks document only to
    /// claim the task and to write down what happened to it.
    ///
    /// The gate a project declares has half an hour, and the `git fetch`
    /// and `git push` around it are bounded by nothing at all. Held for
    /// that, the document's lock made every other mutation in the project
    /// — another delivery, a placement, the evaluation behind a pane
    /// going quiet — wait the full two minutes and then fail, and the
    /// press that waited came back as "nothing ready".
    ///
    /// So the lock is taken to mark the task [`WorkState::Integrating`]
    /// and released. That state is what tells the rest of the client the
    /// task is spoken for: an evaluation skips it, and so does the release
    /// of abandoned tasks — both already did, because a delivery has
    /// always owned its task while it ran. It is retaken at the end, and
    /// the outcome is written onto the record only while it is still the
    /// one that was claimed.
    ///
    /// A process that dies between the two leaves the record
    /// `Integrating`, which is the state every pass reads as "a delivery
    /// owns this" — the delivery it names is gone, and the operator's way
    /// out is the one they already have for a task nothing is doing:
    /// finish it, or discard it.
    fn deliver_claimed(
        &self,
        primary: &Path,
        policy: &WorktreePolicy,
        task_id: &str,
    ) -> Option<DeliveryReport> {
        if let Some(waiting) = self.unjoined_children(primary, task_id) {
            return self.refused_delivery(primary, policy, task_id, waiting);
        }
        let claimed = task::locked(&self.0.home, primary, |store| {
            Ok(task_mut(store, task_id).and_then(|agent| {
                // Claimed as it stands, then marked: the delivery reads
                // the state the operator's record was in, and
                // `Integrating` is what the *document* says while it runs
                // — the claim, not an input to it.
                let claimed = agent.clone();
                agent.isolation()?;
                agent.state = WorkState::Integrating;
                Some(claimed)
            }))
        });
        // The whole record, not only the branch: what the delivery
        // reports is a view of the agent, and the agent is what the
        // document will be asked to take back.
        let mut agent = match claimed {
            Ok(claimed) => claimed?,
            // Nothing was delivered and nothing was written — said as a
            // report rather than as `None`, which the client renders as
            // "nothing ready": the operator who waited on a busy document
            // would be told the task they can see is not there.
            Err(error) => {
                return self.refused_delivery(
                    primary,
                    policy,
                    task_id,
                    format!("the delivery could not start: {error}"),
                );
            }
        };
        let id = agent.id.clone();
        let outcome = deliver_one(primary, policy, &id, &mut agent);
        let mut report = DeliveryReport {
            task: AgentView::from_agent(
                primary,
                &agent,
                policy.completion,
                &target_of(primary, policy),
                landing::forge(primary),
            )?,
            outcome,
            warnings: Vec::new(),
        };
        let delivered = agent.clone();
        let recorded = task::locked(&self.0.home, primary, |store| {
            let Some(record) = task_mut(store, task_id) else {
                return Ok(Recorded::Superseded);
            };
            // Only the record this delivery claimed. The operator can
            // finish or discard a task while its gate runs, and what they
            // decided about it is newer than this.
            if record.state != WorkState::Integrating {
                return Ok(Recorded::Superseded);
            }
            *record = delivered;
            Ok(Recorded::Applied)
        });
        match recorded {
            Ok(Recorded::Applied) => {}
            Ok(Recorded::Superseded) => report.warnings.push(superseded_delivery()),
            Err(error) => report.warnings.push(unrecorded_delivery(&error)),
        }
        Some(report)
    }

    /// The report for a delivery that never started: the task as the
    /// document last had it, and the reason in place of an outcome.
    ///
    /// The view is read without the lock, which is exactly what the lock
    /// is not for — there is nothing to write here, and the document is
    /// replaced atomically, so a reader sees one version or the other.
    /// `None` only where there is genuinely nothing to report about: no
    /// document, or no such task in it.
    /// Why an agent cannot be delivered yet because of its children: each
    /// one holding uncommitted changes, or commits its agent's branch
    /// lacks, is work the delivery would leave behind.
    fn unjoined_children(&self, primary: &Path, task_id: &str) -> Option<String> {
        let store = task::load(&self.0.home, primary).ok()?;
        let waiting: Vec<&str> = store
            .agents
            .iter()
            .filter(|child| child.parent.as_ref().map(AgentId::as_str) == Some(task_id))
            .filter(|child| checkout::is_live(&child.state) || child.state == WorkState::Parked)
            .filter_map(|child| {
                let isolation = child.isolation()?;
                let dirty = isolation
                    .checkout
                    .as_ref()
                    .map(|checkout| checkout.directory(primary))
                    .is_some_and(|directory| directory.is_dir() && checkout::is_dirty(&directory));
                let unjoined = checkout::branch_exists(primary, &isolation.branch)
                    && !checkout::is_integrated(primary, &isolation.target, &isolation.branch);
                (dirty || unjoined).then_some(child.label.as_str())
            })
            .collect();
        (!waiting.is_empty()).then(|| {
            format!(
                "its subagents hold work not joined yet ({}); join them first",
                waiting.join(", ")
            )
        })
    }

    fn refused_delivery(
        &self,
        primary: &Path,
        policy: &WorktreePolicy,
        task_id: &str,
        reason: String,
    ) -> Option<DeliveryReport> {
        let store = task::load(&self.0.home, primary).ok()?;
        let agent = store.agent(task_id)?;
        Some(DeliveryReport {
            task: AgentView::from_agent(
                primary,
                agent,
                policy.completion,
                &target_of(primary, policy),
                landing::forge(primary),
            )?,
            outcome: DeliveryOutcome::Refused(reason),
            warnings: Vec::new(),
        })
    }

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
    fn record_in_the_root(&self, root: &Path, harness: &str) -> Result<Agent> {
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
    fn collect_slot_garbage(&self, cwd: &Path, occupied: &[PathBuf]) -> Vec<String> {
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
        collected
            .branches
            .into_iter()
            .chain(collected.slots.into_iter().map(|slot| slot.to_string()))
            .collect()
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

/// A repository as the task operations see it: its primary checkout, the
/// project's policy, and the recorded tasks.
struct Repository {
    primary: PathBuf,
    policy: WorktreePolicy,
    store: AgentStore,
}

impl Repository {
    fn target(&self) -> String {
        target_of(&self.primary, &self.policy)
    }

    fn views(&self) -> Vec<AgentView> {
        task_views(
            &self.primary,
            &self.store,
            self.policy.completion,
            &self.target(),
        )
    }
}

/// Whether an agent may still be at work on a task in `state`: live, and
/// not owned by a delivery in flight.
fn is_agents_turn(state: &WorkState) -> bool {
    checkout::is_live(state) && *state != WorkState::Integrating
}

/// Ends the children of an agent that ended: each holding nothing its
/// agent's branch lacks goes back to the pool, and each holding work is
/// parked — and then so is the agent, whose checkout is the only way that
/// work reaches the target. Returns whether any child was parked.
fn release_children(primary: &Path, store: &mut AgentStore, parent: &str) -> bool {
    let mut parked_a_child = false;
    for child in store.agents.iter_mut() {
        if child.parent.as_ref().map(AgentId::as_str) != Some(parent)
            || !checkout::is_live(&child.state)
        {
            continue;
        }
        let into = child
            .isolation()
            .map(|isolation| isolation.target.clone())
            .unwrap_or_default();
        if checkout::release(primary, child, &into) == checkout::SlotState::Parked {
            parked_a_child = true;
        }
    }
    if parked_a_child && let Some(agent) = task_mut(store, parent) {
        agent.state = WorkState::Parked;
    }
    parked_a_child
}

/// What reading one task from its checkout came to: the task as the remote
/// is to be asked about it, where completion opens a request, and the
/// conflict its agent is told about when following the target produced one.
struct Read {
    ask_the_remote: Option<(AgentId, Isolation)>,
    notice: Option<AgentNotice>,
}

/// What every task of one evaluation pass is read against.
struct EvaluationPass<'a> {
    primary: &'a Path,
    /// What the project delivers onto, and what an agent in the root is
    /// measured against — it has no base of its own to be ahead of.
    target: &'a str,
    /// Where the work in the project's own root stands, read once.
    ///
    /// It is the *checkout's* answer, and every agent in that checkout
    /// reads the same one — so asking it per agent would run the same four
    /// Git reads once for each of them and get the same result every time.
    in_the_root: OnceCell<WorkState>,
    /// The checkout directories a live pane still sits in.
    occupied: &'a [PathBuf],
    owners: BTreeSet<AgentId>,
    /// Agents a subagent still holds a checkout for. One parked for its
    /// children is parked for their work, however level its own branch is.
    holding_children: BTreeSet<AgentId>,
    vocabulary: &'a BranchVocabulary,
    completion: CompletionBehavior,
}

impl EvaluationPass<'_> {
    /// Reads `task` from its checkout, or `None` when it was not read:
    /// nobody's turn, or settled without needing to be.
    fn evaluate(&self, agent: &mut Agent) -> Option<Read> {
        let primary = self.primary;
        let id = agent.id.clone();
        if !agent.is_isolated() {
            // An agent in the project's own root: the checkout is the
            // project, and where the work stands there is what the
            // operator and every sibling agent are looking at too. Only
            // the three questions a directory can answer are asked — a
            // delivery is UZE's to run only on a branch UZE cut.
            if is_agents_turn(&agent.state) {
                agent.state = self
                    .in_the_root
                    .get_or_init(
                        || match landing::readiness_of_checkout(primary, self.target) {
                            Readiness::Running => WorkState::Running,
                            Readiness::Uncommitted => WorkState::Uncommitted,
                            Readiness::Rebasing { files } => WorkState::Conflicted { files },
                            Readiness::Ready { .. } => WorkState::Ready,
                        },
                    )
                    .clone();
            }
            return None;
        }
        // The branch a task is on is a Git fact, and the recorded branch
        // is a cache of it. Re-read before anything is asked *about* the
        // branch: an operator renaming it by hand otherwise leaves every
        // later question pointed at a ref that no longer exists, and
        // `commits_ahead` answers such a question with `0` — which reads
        // as "nothing to deliver" rather than as "wrong branch". A
        // checkout mid-rebase is on no branch and is left alone. Taken
        // before the isolation is borrowed, since a rename is the agent's
        // label as much as its branch.
        let slot = landing::slot_path(primary, agent.isolation()?);
        if let Some(actual) = slot.as_deref().and_then(checkout::current_branch)
            && actual != agent.isolation()?.branch
        {
            agent.take_name(actual);
        }
        let Agent {
            state, isolation, ..
        } = agent;
        let task = isolation.as_mut()?;
        // A task that ended is still looked at while it owns its slot: the
        // agent that delivered usually keeps working in the same checkout,
        // and skipping every non-live task froze that row on `delivered`
        // for the rest of the session however much the slot changed.
        // `Closed` is the same story with nothing delivered — the checkout
        // it ended in can be written in again. Only the *current* owner is
        // reconsidered: a freed slot handed to a new agent belongs to that
        // agent's task, not to the one that used to sit there. `Parked` is
        // nobody's turn by definition and stays put — unless a pane sits in
        // its checkout (`occupied`): parked means "no agent left", and an
        // agent that is there makes it a lie, whichever way it got there —
        // a release that raced the tab opening, a resume.
        let ended_owner = matches!(*state, WorkState::Integrated | WorkState::Closed)
            && self.owners.contains(&id);
        let parked_with_agent = *state == WorkState::Parked
            && slot
                .as_ref()
                .is_some_and(|slot| self.occupied.iter().any(|pane| pane.starts_with(slot)));
        let parked_alone = *state == WorkState::Parked && !parked_with_agent;
        if !(is_agents_turn(state) || ended_owner || parked_with_agent || parked_alone) {
            return None;
        }
        // Parked is nobody's turn, but its work can still reach the target
        // without it — a request opened before its agent left, merged on
        // the forge. Left parked, it was listed as preserved work for good
        // and its slot never went back to the pool.
        if parked_alone {
            if !self.holding_children.contains(&id) {
                landing::settle_delivered(primary, state, task);
            }
            return None;
        }
        // A rebase paused on work the target already carries — what an
        // earlier refresh left behind when it replayed a squashed branch
        // onto its own squash — is nobody's to resolve.
        let paused = slot
            .as_ref()
            .is_some_and(|slot| landing::paused_rebase(slot).is_some());
        if paused && landing::settle_delivered(primary, state, task) {
            return None;
        }
        self.read_readiness(state, task, ended_owner);
        // The other half of what a delivery would do, learned the same way
        // readiness is: an agent told to push and open the request itself
        // is the one case UZE's own records can never cover, and until this
        // ran the button went on offering to publish a branch the forge
        // already had a request open for. Only where a request is what
        // completion means — a project that merges or hands off never asks
        // the remote anything.
        //
        // Asked after the pass, with the document unlocked: it is a `git
        // ls-remote` per task, and inside the lock one slow remote made
        // every delivery and every placement in the project wait behind the
        // whole pass.
        let ask_the_remote =
            (self.completion == CompletionBehavior::Pr).then(|| (id.clone(), task.clone()));
        // Following a moved target costs a clean task nothing and a dirty
        // one its work in progress, which `refresh` refuses. Whatever the
        // completion behaviour: a task that follows the target as it moves
        // meets a conflict while its agent is still holding the change,
        // rather than in a request already opened.
        let notice = if matches!(*state, WorkState::Running | WorkState::Ready)
            && let Err(DeliveryFailure::Conflict {
                files,
                target_moved,
            }) = landing::refresh(primary, state, task)
            && let Some(slot) = slot
        {
            Some(AgentNotice {
                task: id.as_str().to_owned(),
                checkout: slot,
                message: landing::conflict_message(task, &files, target_moved),
            })
        } else {
            None
        };
        // The automatic half of naming is the agent's label as much as its
        // branch, so it runs with the isolation's borrow released.
        self.name_from_the_work(agent);
        Some(Read {
            ask_the_remote,
            notice,
        })
    }

    fn read_readiness(&self, state: &mut WorkState, task: &mut Isolation, ended_owner: bool) {
        let primary = self.primary;
        match landing::readiness(primary, task) {
            // Nothing new since it ended leaves the ending standing: the
            // delivery is the last thing that happened to the task, and
            // saying `running` instead would erase it on the next tick.
            Readiness::Running if ended_owner => {}
            Readiness::Running => *state = WorkState::Running,
            Readiness::Uncommitted => *state = WorkState::Uncommitted,
            Readiness::Rebasing { files } => *state = WorkState::Conflicted { files },
            // A forge that squashes what it merges leaves none of the
            // branch's commits in the target, so they still count as ahead;
            // asked by patch instead, the work is delivered. Left `Ready`,
            // the refresh replayed it onto its own squash and paused
            // mid-rebase on every file it touched.
            Readiness::Ready { .. }
                if checkout::is_integrated(primary, &task.target, &task.branch) =>
            {
                landing::mark_delivered(primary, state, task);
            }
            Readiness::Ready { base, .. } => {
                // Delivered, and now holding work the target lacks: the
                // agent kept going, and the request it had answered for the
                // work already merged, not for this.
                if *state == WorkState::Integrated {
                    task.forget_request();
                }
                task.base_commit = base;
                if *state != WorkState::GateFailed {
                    *state = WorkState::Ready;
                }
            }
        }
    }

    /// The work has a commit and still carries the name UZE generated for
    /// it: name it from what the agent wrote. This is the automatic half,
    /// and it deliberately runs *late* — until there is a commit there is
    /// nothing to name the work after, and the agent naming it deliberately
    /// arrives earlier and therefore wins. `Ready` is the safe moment by
    /// construction: commits ahead, a clean tree, no rebase in progress.
    ///
    /// Nothing about this reaches a harness. It is a Git fact read on a
    /// pass that already runs, which is why it works on every harness and
    /// on the next one.
    fn name_from_the_work(&self, agent: &mut Agent) {
        let primary = self.primary;
        let Some(isolation) = agent.isolation() else {
            return;
        };
        if self.vocabulary.names_work()
            && agent.state == WorkState::Ready
            && !agent.is_named()
            && let Some(derived) = landing::derived_name(primary, isolation, self.vocabulary)
            && !checkout::branch_exists(primary, &derived)
            && checkout::rename_branch(primary, &isolation.branch.clone(), &derived).is_ok()
        {
            agent.take_name(derived);
        }
    }
}

/// The branch this project delivers into: what it declared, else whatever
/// the primary checkout is on.
pub(super) fn target_of(primary: &Path, policy: &WorktreePolicy) -> String {
    policy
        .target
        .clone()
        .or_else(|| checkout::current_branch(primary))
        .unwrap_or_else(|| "HEAD".to_owned())
}

fn task_mut<'a>(store: &'a mut AgentStore, id: &str) -> Option<&'a mut Agent> {
    store.agent_mut(id)
}

fn task_views(
    primary: &Path,
    store: &AgentStore,
    completion: CompletionBehavior,
    target: &str,
) -> Vec<AgentView> {
    // A property of the repository, not of a row: asked once here rather
    // than once per agent, since every task of one project reaches the
    // same remote. Only where the completion publishes — the word this
    // buys is a word for a request, and a project that opens none has no
    // use for it.
    let forge = if completion == CompletionBehavior::Pr {
        landing::forge(primary)
    } else {
        Forge::default()
    };
    store
        .agents
        .iter()
        .filter_map(|agent| AgentView::from_agent(primary, agent, completion, target, forge))
        .collect()
}

/// What a delivery that landed but could not be written down leaves the
/// operator to know: the next evaluation will read the task as it was and
/// offer the same delivery again.
fn unrecorded_delivery(error: &UzeError) -> String {
    format!("the delivery could not be recorded: {error}")
}

/// The same, for a task somebody decided about while it was being
/// delivered. Their decision stands; the delivery still happened.
fn superseded_delivery() -> String {
    "the task changed while it was being delivered, so the delivery is not recorded on it"
        .to_owned()
}

/// Whether a delivery's outcome reached the record it was claimed from.
enum Recorded {
    Applied,
    Superseded,
}

/// Delivers one claimed task the way `policy` says, updating `task` to
/// say what happened.
///
/// Takes the record rather than the document: this is the unbounded half
/// — the project's gate, then a fetch, a push or a merge — and it runs
/// with the tasks document unlocked, under Git's own write lock alone.
fn deliver_one(
    primary: &Path,
    policy: &WorktreePolicy,
    id: &AgentId,
    agent: &mut Agent,
) -> DeliveryOutcome {
    let completion = policy.completion;
    let gate = policy.gate.clone();
    let policy = landing::Policy {
        completion,
        gate: &gate,
    };
    // Kept for the messages below, which describe the branch rather than
    // the outcome; delivery itself writes through the agent.
    let task = agent.isolation().cloned();
    let task = task.as_ref();
    match landing::deliver(primary, agent, &policy) {
        Ok(Delivered::Handoff) => DeliveryOutcome::Handoff,
        Ok(Delivered::Merged { .. }) => DeliveryOutcome::Merged,
        Ok(Delivered::Published { branch, request }) => {
            DeliveryOutcome::Published { branch, request }
        }
        Ok(Delivered::AwaitingRequest {
            branch: _,
            instruction,
        }) => DeliveryOutcome::AwaitingRequest(AgentNotice {
            task: id.as_str().to_owned(),
            checkout: task
                .and_then(|task| landing::slot_path(primary, task))
                .unwrap_or_default(),
            message: instruction,
        }),
        Err(DeliveryFailure::Conflict {
            files,
            target_moved,
        }) => DeliveryOutcome::ReturnedToAgent(AgentNotice {
            task: id.as_str().to_owned(),
            checkout: task
                .and_then(|task| landing::slot_path(primary, task))
                .unwrap_or_default(),
            message: landing::conflict_message(
                task.expect("delivery needs isolation"),
                &files,
                target_moved,
            ),
        }),
        Err(DeliveryFailure::GateFailed { command, output }) => {
            DeliveryOutcome::ReturnedToAgent(AgentNotice {
                task: id.as_str().to_owned(),
                checkout: task
                    .and_then(|task| landing::slot_path(primary, task))
                    .unwrap_or_default(),
                message: landing::gate_failure_message(
                    task.expect("delivery needs isolation"),
                    &command,
                    &output,
                ),
            })
        }
        Err(other) => DeliveryOutcome::Refused(other.to_string()),
    }
}

/// What an isolation does with the changes the operator's tree holds.
///
/// Never "move" and never "discard": UZE cannot say whose uncommitted
/// change is whose, so the root's working tree is left exactly as it was
/// either way.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Carry {
    /// The checkout starts from the branch alone.
    #[default]
    Nothing,
    /// The checkout starts with a copy of what the root's tree holds over
    /// its `HEAD`, for the files the repository tracks.
    CopyOfChanges,
}

/// What naming a task produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedTask {
    pub task: String,
    /// The branch the work now lives on; `None` for an agent in the
    /// operator's checkout, whose naming renames nothing.
    pub branch: Option<String>,
    pub label: String,
}

/// A refusal, in words an agent can act on: which half was wrong, and what
/// this project would have accepted.
fn refusal_words(refusal: &NameRefusal, vocabulary: &BranchVocabulary) -> String {
    match refusal {
        NameRefusal::NotDeclared => {
            "this project does not name agent work: declare `worktrees.branch` in agents.yaml"
                .to_owned()
        }
        NameRefusal::UnknownType { found, .. } => format!(
            "`{found}` is not a type this project accepts; use one of `{}`",
            vocabulary.spelled()
        ),
        NameRefusal::UnexpectedType { found } => format!(
            "this project takes {}, so drop the `{found}/`",
            vocabulary.spelled()
        ),
        NameRefusal::MissingType { .. } => format!(
            "a name is `<type>/<subject>`; the types this project accepts are `{}`",
            vocabulary.spelled()
        ),
        NameRefusal::MalformedSubject { reason } => format!(
            "the subject is one or two words naming the intention, and {reason} — \
             try something like `fix/branch-naming`"
        ),
    }
}

/// A task ended because its agent is gone, and what became of its slot.
/// What one pass of [`Workspace::reconcile_occupancy`] changed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reconciliation {
    /// The directories whose repository actually gave a slot up, or whose
    /// agents ended, so a caller knows which agents are worth re-reading.
    /// Empty is the ordinary answer.
    pub changed: Vec<PathBuf>,
    pub released: Vec<ReleasedTask>,
}

/// The canonical spelling of a directory: the key every record of it is
/// stored under, so two spellings of one root never make two stores.
fn canonical(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// Where one agent is placed: in a checkout of its own, or in the space's
/// own directory beside whoever else is in it. The same two answers
/// `worktrees.default` gives, asked of a single launch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementKind {
    Isolated,
    InPlace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleasedTask {
    pub id: String,
    pub label: String,
    /// `true` when the checkout held work and was parked for the operator
    /// instead of going back to the pool.
    pub parked: bool,
}

/// The delivery target against its upstream: commits a pull would bring
/// in and a push would send out.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpstreamSync {
    pub pull: usize,
    pub push: usize,
}

/// One piece of preserved work, as the list that crosses projects sees it.
///
/// Deliberately thinner than a [`AgentView`]. Readiness, publication and how
/// far a branch is ahead are questions about the project you are *in*, and
/// answering them here would put one Git read per project on the machine
/// behind a keystroke. Everything below comes from the record alone, which
/// is what keeps the list instant however many projects accumulate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreservedWork {
    /// The repository this work belongs to. A list that crosses projects
    /// has to say, or two agents on a branch of the same name in two
    /// projects are one row twice.
    pub project: PathBuf,
    pub id: String,
    pub label: String,
    pub branch: String,
    pub checkout: Option<PathBuf>,
    /// The state the record itself carries — never `Integrated` or
    /// `Closed`, which is what "preserved" means.
    pub state: WorkStateView,
    pub created_at_unix: u64,
}

/// One task as presentation sees it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentView {
    pub id: String,
    pub label: String,
    pub branch: String,
    pub target: String,
    /// The checkout this agent works in: a slot of its own when it has
    /// one, and the project's own root when it does not.
    pub checkout: Option<PathBuf>,
    /// Whether the checkout above was cut for this agent alone.
    ///
    /// The fact, not one of its consequences: an isolated agent is the one
    /// UZE gave a branch and a slot to, and that is what decides whether
    /// delivering it is UZE's to do, which group its row sits in, and
    /// whether it can still be offered isolation. Read the fact and state
    /// the rule at the call site — `branch` is not a proxy for it, because
    /// an agent in the project's root is on one too.
    pub isolated: bool,
    /// The agent whose subagent this is: its checkout was split from that
    /// agent's, and its work joins that agent's branch rather than the
    /// target.
    pub parent: Option<String>,
    pub state: WorkStateView,
    /// What delivering this task does — the project's own say, carried on
    /// the task so a surface offering the delivery can name its ending
    /// instead of showing one verb for three different outcomes.
    pub completion: CompletionBehavior,
    /// Commits the branch has beyond its base — what a delivery would land.
    pub ahead: usize,
    /// The name the branch is published under on the remote, once it is
    /// there. Read from the repository's remote-tracking refs, so a
    /// branch its own agent pushed reads as published exactly like one
    /// UZE pushed.
    pub published_as: Option<String>,
    /// The request open on the forge for the published branch, once there
    /// is one: what turns the delivery button from an errand into a sync.
    pub published_request: Option<u32>,
    /// Which forge `origin` points at, so a surface reporting the request
    /// can use that forge's own word for it — a pull request and a merge
    /// request are one thing under two names, and the remote is what says
    /// which name this project's reader reads. [`Forge::Unknown`] where
    /// it did not say, and then nothing is claimed.
    pub forge: Forge,
    /// Commits the published branch does not carry yet — what a sync would
    /// send. `None` until the branch has been published at all, and
    /// `Some(0)` once the request is level with the branch: work already
    /// handed over is not work waiting to be handed over, however far the
    /// branch still is from the target, which only a merge closes.
    pub unsynced: Option<usize>,
    pub created_at_unix: u64,
}

impl PreservedWork {
    /// `None` for an agent that has nothing preserved: one working in the
    /// project's own root, which has no branch to hold work on, and one
    /// whose work the target already carries or that never had any.
    ///
    /// Read from the record alone — the state is the one the record
    /// carries, not the one a Git read would draw — because this is what
    /// lets the list answer for a machine without asking a repository
    /// anything.
    fn from_agent(project: &Path, agent: &Agent) -> Option<Self> {
        let isolation = agent.isolation()?;
        if matches!(agent.state, WorkState::Integrated | WorkState::Closed) {
            return None;
        }
        Some(Self {
            project: project.to_path_buf(),
            id: agent.id.as_str().to_owned(),
            label: agent.label.clone(),
            branch: isolation.branch.clone(),
            checkout: landing::slot_path(project, isolation),
            state: WorkStateView::from(&agent.state),
            created_at_unix: agent.created_at_unix,
        })
    }
}

impl AgentView {
    /// `None` for an agent working in the project's root: every field
    /// here is about a branch of its own, and it has none.
    fn from_agent(
        primary: &Path,
        agent: &Agent,
        completion: CompletionBehavior,
        target: &str,
        forge: Forge,
    ) -> Option<Self> {
        let Some(task) = agent.isolation() else {
            return Some(Self::in_the_root(primary, agent, completion, target, forge));
        };
        // What the remote holds, not what UZE remembers having sent: a
        // push the agent made is a push, and a view built from UZE's own
        // record of its own deliveries goes on offering to send commits
        // the request already carries.
        //
        // Only where the completion publishes. A branch on the remote is
        // no part of what a merge or a handoff would do, and counting a
        // merge's commits against the remote would report a task as
        // delivered the moment its agent pushed it.
        let published = (completion == CompletionBehavior::Pr)
            .then(|| landing::publication(primary, task))
            .flatten();
        let unsynced = published
            .as_ref()
            .map(|published| checkout::commits_ahead(primary, &published.tip, &task.branch));
        Some(Self {
            id: agent.id.as_str().to_owned(),
            label: agent.label.clone(),
            branch: task.branch.clone(),
            target: task.target.clone(),
            checkout: landing::slot_path(primary, task),
            state: drawn_state(primary, &agent.state, task, unsynced),
            completion,
            isolated: true,
            parent: agent
                .parent
                .as_ref()
                .map(|parent| parent.as_str().to_owned()),
            ahead: checkout::commits_ahead(primary, &task.base_commit, &task.branch),
            published_as: published.map(|published| published.branch),
            published_request: task.published_request,
            forge,
            unsynced,
            created_at_unix: agent.created_at_unix,
        })
    }

    /// An agent working in the project's own root, beside the operator.
    ///
    /// Every field here is about the checkout it sits in rather than one
    /// cut for it, and every other agent in that checkout reads the same
    /// answers — which is the truth about where they are. What it never
    /// carries is a publication or a base of its own: the branch is the
    /// operator's, UZE did not cut it, and `isolated` says so, because
    /// rebasing and pushing it is theirs to ask for and never UZE's to
    /// offer.
    fn in_the_root(
        primary: &Path,
        agent: &Agent,
        completion: CompletionBehavior,
        target: &str,
        forge: Forge,
    ) -> Self {
        let branch = checkout::current_branch(primary).unwrap_or_default();
        Self {
            id: agent.id.as_str().to_owned(),
            label: agent.label.clone(),
            ahead: if branch.is_empty() {
                0
            } else {
                checkout::commits_ahead(primary, target, &branch)
            },
            branch,
            target: target.to_owned(),
            checkout: Some(primary.to_path_buf()),
            state: WorkStateView::from(&agent.state),
            completion,
            isolated: false,
            parent: None,
            published_as: None,
            published_request: None,
            forge,
            unsynced: None,
            created_at_unix: agent.created_at_unix,
        }
    }
}

/// The state a task reads as once what the remote holds is folded in.
///
/// `Ready` alone answers "the branch holds commits its base lacks", which
/// stops being the interesting question the moment the branch is on the
/// remote: from then on what every surface needs to say is whether
/// anything is still waiting to be handed over. Only where the completion
/// publishes — `published` is `None` everywhere else, and the record's own
/// state stands.
fn drawn_state(
    primary: &Path,
    state: &WorkState,
    task: &Isolation,
    unsynced: Option<usize>,
) -> WorkStateView {
    // Parked says only that nobody is there. A rebase paused in the
    // checkout is what the operator will find, and "uncommitted changes"
    // sent them looking for edits that were really conflict markers.
    if *state == WorkState::Parked
        && let Some(files) =
            landing::slot_path(primary, task).and_then(|slot| landing::paused_rebase(&slot))
    {
        return WorkStateView::Conflicted { files };
    }
    publication_state(state, unsynced)
}

fn publication_state(state: &WorkState, unsynced: Option<usize>) -> WorkStateView {
    let view = WorkStateView::from(state);
    if view == WorkStateView::Ready && unsynced == Some(0) {
        return WorkStateView::Published;
    }
    view
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkStateView {
    Running,
    Uncommitted,
    Ready,
    /// The branch is on the remote and carries nothing the remote lacks:
    /// the work is with whoever reviews it, not with the operator.
    ///
    /// A view of `Ready`, not a record of its own — `Ready` is still what
    /// the branch holds, and pressing deliver still syncs it as the target
    /// moves. It is a state here because "there is work to hand over" and
    /// "the work is handed over" are the two things every surface has to
    /// tell apart, and a surface that reads only `Ready` cannot: the
    /// sidebar went on marking a task deliverable for the whole life of an
    /// open request.
    Published,
    Integrating,
    Conflicted {
        files: Vec<PathBuf>,
    },
    GateFailed,
    Integrated,
    Parked,
    /// The agent is gone and its branch held nothing to deliver.
    Closed,
}

impl WorkStateView {
    /// Why delivery is refused, for a state where it is — `None` for the
    /// states delivery may be offered for. `Published` is one: the request
    /// is level with the branch, but the target moves, and a re-sync is how
    /// the branch follows it.
    ///
    /// Beside the predicate rather than beside whoever shows the answer:
    /// "not yet" and "already done" are the same refusal to a caller that
    /// only sees a boolean, and a second surface asking the same question
    /// would otherwise write its own second version of these words.
    ///
    /// A few words each: these are read in the header's own row, beside
    /// the button that was just pressed, where the state's mark and the
    /// tab already carry everything the sentence would repeat.
    pub fn undeliverable_reason(&self) -> Option<&'static str> {
        match self {
            Self::Ready | Self::Published | Self::GateFailed => None,
            Self::Running => Some("nothing committed"),
            Self::Uncommitted => Some("uncommitted changes"),
            Self::Conflicted { .. } => Some("rebase paused"),
            Self::Integrating => Some("already delivering"),
            Self::Integrated => Some("already delivered"),
            Self::Closed => Some("branch holds nothing"),
            Self::Parked => Some("parked — resume it first"),
        }
    }
}

impl From<&WorkState> for WorkStateView {
    fn from(state: &WorkState) -> Self {
        match state {
            WorkState::Running => Self::Running,
            WorkState::Uncommitted => Self::Uncommitted,
            WorkState::Ready => Self::Ready,
            WorkState::Integrating => Self::Integrating,
            WorkState::Conflicted { files } => Self::Conflicted {
                files: files.clone(),
            },
            WorkState::GateFailed => Self::GateFailed,
            WorkState::Integrated => Self::Integrated,
            WorkState::Parked => Self::Parked,
            WorkState::Closed => Self::Closed,
        }
    }
}

/// A message for the pane of the agent that owns `task`, running in
/// `checkout`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentNotice {
    pub task: String,
    pub checkout: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Evaluation {
    pub tasks: Vec<AgentView>,
    pub notices: Vec<AgentNotice>,
    /// Why the repository's recorded tasks could not be read, when they
    /// could not be.
    ///
    /// An empty `tasks` says "this repository has no tasks", and a store
    /// that failed to open says something entirely different — every agent
    /// loses its branch, its mark and its delivery button, and the surface
    /// that swallowed the error has no way to say why. `place_new_agent`
    /// already reported this and was the only thing that did, so the
    /// condition surfaced as a single truncated line the one time somebody
    /// happened to add an agent.
    pub unreadable: Option<String>,
    /// The document could not be read at all and was set aside, so the
    /// tasks above were adopted afresh from the checkouts Git registers
    /// rather than read from what UZE had recorded.
    ///
    /// Said once, and never in place of the work: an old schema, a hand
    /// edit or a corrupt file used to stop every agent in the project
    /// from being placed, which is a worse answer than starting the
    /// record again from what is on disk.
    pub recovered: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryOutcome {
    Handoff,
    Merged,
    /// The branch was pushed and the forge already has this request
    /// open for it: from here delivery is a sync, and Git alone.
    Published {
        branch: String,
        request: u32,
    },
    /// The branch was pushed and has no request yet, so the owning agent
    /// was handed the words to open one. Carries an [`AgentNotice`] like
    /// the two failures do, because it reaches the agent the same way: a
    /// submission into its pane.
    AwaitingRequest(AgentNotice),
    /// Nothing was written; the reason names why.
    Refused(String),
    /// The target is untouched and the owning agent has been told what to do.
    ReturnedToAgent(AgentNotice),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryReport {
    pub task: AgentView,
    pub outcome: DeliveryOutcome,
    /// What the delivery could not do, none of which undoes the outcome —
    /// the same channel [`AgentPlacement`] carries. The one entry today is
    /// the delivery that landed and could not be written down: the branch
    /// is pushed and the request is open, and the next evaluation still
    /// reads the task as deliverable, so the operator has to know before
    /// pressing it again.
    pub warnings: Vec<String>,
}

/// What writing the policy is about to do to the project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyWriteConsequence {
    /// The file does not exist yet, so declaring adds a tracked file to
    /// the repository rather than editing one.
    pub creates_manifest: bool,
    pub manifest: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryPolicyView {
    pub completion: &'static str,
    pub target: Option<String>,
    pub gate: Vec<String>,
}

/// Where an agent starts, and the record its launch carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentPlacement {
    pub cwd: PathBuf,
    /// The repository the agent belongs to, which is a different question
    /// from where it will run: an isolated agent's `cwd` is a checkout
    /// under the project, and a client deciding *which space* to open the
    /// tab in needs the project rather than the checkout.
    pub project: PathBuf,
    pub placement: Placement,
    /// What preparing the checkout could not do — a missing link target, a
    /// failed setup — none of which stops the launch.
    pub warnings: Vec<String>,
    /// The agent's row as it stands the instant it exists, so a client can
    /// draw it before an evaluation has run. `None` only where the record
    /// could not be read back — never a reason to draw the agent as
    /// something else.
    pub view: Option<AgentView>,
}

/// What an agent was placed as. There is no third case: a placement that
/// cannot do what was asked answers with an error and starts nothing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Placement {
    /// The agent runs in a checkout of its own, on the task's branch.
    Isolated {
        task: AgentId,
        checkout: checkout::CheckoutId,
        branch: String,
        reused: bool,
    },
    /// The agent runs in the space's own directory, on whatever branch it
    /// is on, sharing that tree with whoever else is in it.
    InPlace { id: AgentId },
}

impl Placement {
    /// The identity the launch carries, whichever kind of record it is.
    pub fn agent(&self) -> &AgentId {
        match self {
            Self::Isolated { task, .. } => task,
            Self::InPlace { id } => id,
        }
    }
}

#[cfg(test)]
mod tests;
