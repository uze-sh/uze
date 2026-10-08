//! Delivering an agent's work through the project's policy, and recording what came of it.

use super::*;

impl Workspace<'_> {
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
    pub(super) fn deliver_claimed(
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
                &checkout::BranchTips::read(primary),
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
    pub(super) fn unjoined_children(&self, primary: &Path, task_id: &str) -> Option<String> {
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

    pub(super) fn refused_delivery(
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
                &checkout::BranchTips::read(primary),
                agent,
                policy.completion,
                &target_of(primary, policy),
                landing::forge(primary),
            )?,
            outcome: DeliveryOutcome::Refused(reason),
            warnings: Vec::new(),
        })
    }
}

/// What a delivery that landed but could not be written down leaves the
/// operator to know: the next evaluation will read the task as it was and
/// offer the same delivery again.
pub(super) fn unrecorded_delivery(error: &UzeError) -> String {
    format!("the delivery could not be recorded: {error}")
}

/// The same, for a task somebody decided about while it was being
/// delivered. Their decision stands; the delivery still happened.
pub(super) fn superseded_delivery() -> String {
    "the task changed while it was being delivered, so the delivery is not recorded on it"
        .to_owned()
}

/// Whether a delivery's outcome reached the record it was claimed from.
pub(super) enum Recorded {
    Applied,
    Superseded,
}

/// Delivers one claimed task the way `policy` says, updating `task` to
/// say what happened.
///
/// Takes the record rather than the document: this is the unbounded half
/// — the project's gate, then a fetch, a push or a merge — and it runs
/// with the tasks document unlocked, under Git's own write lock alone.
pub(super) fn deliver_one(
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
