//! The evaluation pass: what each agent's work is now, read from Git and recorded once.

use super::*;

impl Workspace<'_> {
    /// Re-reads every live task's state from its checkout — what the
    /// sidebar shows after an agent's pane goes quiet — lets a clean, live
    /// task follow a target that moved, and asks the remote about the
    /// requests that are due: [`Self::read_tasks`] and
    /// [`Self::ask_about_requests`] in one call, for a caller with nobody
    /// waiting on the first half.
    #[tracing::instrument(name = "workspace.evaluate_tasks", skip_all, fields(cwd = %cwd.display()))]
    pub fn evaluate_tasks(&self, cwd: &Path, occupied: &[PathBuf]) -> Evaluation {
        let mut evaluation = self.read_tasks(cwd, occupied);
        if let Some(tasks) = self.ask_about_requests(&evaluation.requests) {
            evaluation.tasks = tasks;
        }
        evaluation
    }

    /// The pass without the network: every state read from the checkouts
    /// and the record, and the question for the remote carried in
    /// [`Evaluation::requests`] rather than asked. A `git ls-remote` takes
    /// seconds over SSH, and a state the disk already knows was drawn only
    /// once the forge had answered about something else.
    #[tracing::instrument(name = "workspace.read_tasks", skip_all, fields(cwd = %cwd.display()))]
    pub fn read_tasks(&self, cwd: &Path, occupied: &[PathBuf]) -> Evaluation {
        let (primary, policy) = match self.repository_context(cwd) {
            Ok(Some(context)) => context,
            Ok(None) => return Evaluation::default(),
            // Said rather than drawn as a project with no agents: a
            // manifest mid-edit would otherwise take every agent's row away.
            Err(error) => {
                return Evaluation {
                    unreadable: Some(error.to_string()),
                    ..Evaluation::default()
                };
            }
        };
        let target = target_of(&primary, &policy);
        let declared = declaration::actions(&primary).unwrap_or_default();
        let instructions = declared.with_fallbacks();
        let ignored_actions = declared.ignored;
        let mut arrived = Vec::new();
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
                refs: checkout::Refs::read(&primary),
                holding_children: store
                    .agents
                    .iter()
                    .filter(|agent| {
                        checkout::is_live(&agent.state) || agent.state == WorkState::Shelved
                    })
                    .filter_map(|agent| agent.parent.clone())
                    .collect(),
                vocabulary: &policy.branch,
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
                let was = (agent.state.clone(), agent.label.clone());
                if let Some(read) = pass.evaluate(agent) {
                    ask_the_remote.extend(read.ask_the_remote);
                    arrived.extend(read.arrived);
                }
                if was != (agent.state.clone(), agent.label.clone()) {
                    tracing::info!(
                        agent = %agent.id.as_str(),
                        label = %agent.label,
                        from = ?was.0,
                        to = ?agent.state,
                        "an agent's work changed"
                    );
                }
            }
            // The views are built from this copy once the lock is released:
            // they read Git for every recorded agent, closed ones included,
            // and inside the lock that held every placement and delivery in
            // the project behind a pass that only reads.
            Ok(store.clone())
        });
        let (recorded, recovery) = match evaluated {
            Ok(answered) => answered,
            Err(error) => {
                return Evaluation {
                    unreadable: Some(error.to_string()),
                    ..Evaluation::default()
                };
            }
        };
        Evaluation {
            tasks: task_views(&primary, &recorded, completion, &instructions, &target),
            arrived,
            ignored_actions,
            unreadable: None,
            recovered: recovery.set_aside.map(|set_aside| {
                let kept = set_aside.path.file_name().map_or_else(
                    || set_aside.path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                format!("unreadable record kept as {kept}")
            }),
            requests: RequestQuestion {
                asked: ask_the_remote,
                primary,
                completion,
                instructions,
                target,
            },
        }
    }

    /// Asks the remote the question a [`Self::read_tasks`] pass left, and
    /// answers with every task's view as recorded after it — `None` when
    /// the remote was not asked at all (each task was asked within the
    /// minute, or already has its number), so the views that pass gave
    /// still stand.
    #[tracing::instrument(name = "workspace.ask_about_requests", skip_all)]
    pub fn ask_about_requests(&self, question: &RequestQuestion) -> Option<Vec<AgentView>> {
        let recorded = self.adopt_observed_requests(&question.primary, &question.asked)?;
        Some(task_views(
            &question.primary,
            &recorded,
            question.completion,
            &question.instructions,
            &question.target,
        ))
    }

    /// Asks the remote about each task's request with the document
    /// unlocked, then takes the lock once to write the answers down,
    /// answering with the records as written so a request discovered here
    /// is in the views at once rather than in the next pass's. `None` when
    /// the remote was not asked, or nothing could be written.
    pub(super) fn adopt_observed_requests(
        &self,
        primary: &Path,
        asked: &[(AgentId, Isolation)],
    ) -> Option<AgentStore> {
        let observed: Vec<(&(AgentId, Isolation), landing::RequestObservation)> = asked
            .iter()
            .map(|asked| (asked, landing::observe_request(primary, &asked.1)))
            .collect();
        if observed.is_empty() {
            return None;
        }
        let asked_the_remote = observed
            .iter()
            .any(|(_, observation)| observation.asked_the_remote());
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
            Ok(store.clone())
        });
        match adopted {
            // Written either way — a number for a branch the remote no
            // longer has is dropped without asking it — but only a
            // question the remote answered is news the pass's views lack.
            Ok(recorded) => asked_the_remote.then_some(recorded),
            // The pass itself stands — its states were written, and the
            // views already say them. What is lost is a request number the
            // next pass asks for again.
            Err(error) => {
                tracing::warn!(%error, "the remote's answer about open requests was not recorded");
                None
            }
        }
    }
}

/// What reading one task from its checkout came to: the task as the remote
/// is to be asked about it, when the remote has something to say, and the
/// agent, when following the target moved it into a conflict.
pub(super) struct Read {
    pub(super) ask_the_remote: Option<(AgentId, Isolation)>,
    /// The agent, when this pass's own rebase moved it into a conflict.
    pub(super) arrived: Option<String>,
}

/// What every task of one evaluation pass is read against.
pub(super) struct EvaluationPass<'a> {
    pub(super) primary: &'a Path,
    /// What the project delivers onto, and what an agent in the root is
    /// measured against — it has no base of its own to be ahead of.
    pub(super) target: &'a str,
    /// Where the work in the project's own root stands, read once.
    ///
    /// It is the *checkout's* answer, and every agent in that checkout
    /// reads the same one — so asking it per agent would run the same four
    /// Git reads once for each of them and get the same result every time.
    pub(super) in_the_root: OnceCell<WorkState>,
    /// The checkout directories a live pane still sits in.
    pub(super) occupied: &'a [PathBuf],
    pub(super) owners: BTreeSet<AgentId>,
    /// The repository's branches and shelves, read once for every task the
    /// pass settles rather than by a process per task.
    pub(super) refs: checkout::Refs,
    /// Agents a subagent still holds a checkout for. One unfinished for its
    /// children is unfinished for their work, however level its own branch is.
    pub(super) holding_children: BTreeSet<AgentId>,
    pub(super) vocabulary: &'a BranchVocabulary,
}

impl EvaluationPass<'_> {
    /// Reads `task` from its checkout, or `None` when it was not read:
    /// nobody's turn, or settled without needing to be.
    pub(super) fn evaluate(&self, agent: &mut Agent) -> Option<Read> {
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
        // Every question below runs Git in the slot. One whose `.git` no
        // longer names the project's repository answers to whoever
        // rewrote it, so it is not asked at all.
        if let Some(slot) = &slot
            && let Err(reason) = checkout::anchored(primary, slot)
        {
            tracing::warn!(agent = %id.as_str(), %reason, "a checkout is no longer the project's");
            return None;
        }
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
        // agent's task, not to the one that used to sit there. `Shelved` is
        // nobody's turn by definition and stays put — unless a pane sits in
        // its checkout (`occupied`): unfinished means "no agent left", and an
        // agent that is there makes it a lie, whichever way it got there —
        // a release that raced the tab opening, a resume.
        let ended_owner = matches!(*state, WorkState::Integrated | WorkState::Closed)
            && self.owners.contains(&id);
        let parked_with_agent = *state == WorkState::Shelved
            && slot
                .as_ref()
                .is_some_and(|slot| self.occupied.iter().any(|pane| pane.starts_with(slot)));
        let parked_alone = *state == WorkState::Shelved && !parked_with_agent;
        if !(is_agents_turn(state) || ended_owner || parked_with_agent || parked_alone) {
            return None;
        }
        // Shelved is nobody's turn, but its work can still reach the target
        // without it — a request opened before its agent left, merged on
        // the forge. Left unfinished, it was listed as preserved work for good
        // and its slot never went back to the pool.
        if parked_alone {
            if !self.holding_children.contains(&id) {
                landing::settle_delivered(primary, &self.refs, id.as_str(), state, task);
            }
            return None;
        }
        // A rebase paused on work the target already carries — what an
        // earlier refresh left behind when it replayed a squashed branch
        // onto its own squash — is nobody's to resolve.
        let paused = slot
            .as_ref()
            .is_some_and(|slot| landing::paused_rebase(slot).is_some());
        if paused && landing::settle_delivered(primary, &self.refs, id.as_str(), state, task) {
            return None;
        }
        self.read_readiness(state, task, ended_owner);
        // The request, learned the same way readiness is: whoever pushed
        // the branch and opened a request for it, UZE's own records never
        // cover it, and a published branch reads as in review only once
        // the forge says it is. Whatever the project's delivery: a push is
        // a push. Only a task the remote has something to say about is
        // carried to it (`request_due`).
        //
        // Asked after the pass, with the document unlocked: it is a `git
        // ls-remote` per task, and inside the lock one slow remote made
        // every delivery and every placement in the project wait behind the
        // whole pass.
        let ask_the_remote =
            landing::request_due(&self.refs.tips, task).then(|| (id.clone(), task.clone()));
        // Following a moved target costs a clean task nothing and a dirty
        // one its work in progress, which `refresh` refuses. Whatever the
        // completion behaviour: a task that follows the target as it moves
        // meets a conflict while its agent is still holding the change,
        // rather than in a request already opened.
        let arrived = (matches!(*state, WorkState::Running | WorkState::Ready)
            && slot.is_some()
            && matches!(
                landing::refresh(primary, state, task),
                Err(DeliveryFailure::Conflict { .. })
            ))
        .then(|| id.as_str().to_owned());
        // The automatic half of naming is the agent's label as much as its
        // branch, so it runs with the isolation's borrow released.
        self.name_from_the_work(agent);
        Some(Read {
            ask_the_remote,
            arrived,
        })
    }

    pub(super) fn read_readiness(
        &self,
        state: &mut WorkState,
        task: &mut Isolation,
        ended_owner: bool,
    ) {
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
    pub(super) fn name_from_the_work(&self, agent: &mut Agent) {
        let primary = self.primary;
        let Some(isolation) = agent.isolation() else {
            return;
        };
        if self.vocabulary.names_work()
            && agent.state == WorkState::Ready
            && !agent.is_named()
            && let Some(derived) = landing::derived_name(primary, isolation, self.vocabulary)
            && !checkout::name_is_taken(primary, &derived)
            && checkout::rename_branch_within(
                primary,
                &isolation.branch.clone(),
                &derived,
                std::time::Duration::ZERO,
            )
            .is_ok()
        {
            // A label the operator typed is a name somebody chose, and the
            // branch being named after the commit does not take it away.
            let chosen = agent.has_chosen_label().then(|| agent.label.clone());
            agent.take_name(derived);
            if let Some(label) = chosen {
                agent.label = label;
            }
        }
    }
}
