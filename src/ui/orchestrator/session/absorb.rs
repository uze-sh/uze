//! Absorbing the background answers the pump drains, and scheduling the next reads.

use super::*;

impl Attach<'_> {
    /// Submits `message` into `pane` as UZE's own words — see
    /// [`notice_bytes`] for why it cannot be typed as it stands.
    fn submit_notice(&mut self, pane: PaneId, message: &str) {
        let bracketed = self
            .model
            .panes
            .get(&pane)
            .is_some_and(|snapshot| snapshot.bracketed_paste);
        let bytes = crate::ui::orchestrator::input::notice_bytes(message, bracketed);
        let _ = send_request(&mut self.stream, &ClientRequest::Input { pane, bytes });
    }

    /// Sends the instruction the project declares for the state `task` is
    /// in to its agent, as one submission — on a click when `automatic` is
    /// false, and when an operation of UZE's left the work there when it is
    /// true. Never queued: a pane that may not take it now refuses with
    /// the reason, and the status's click is how to send it later.
    pub(super) fn send_instruction(&mut self, task: &AgentView, automatic: bool) {
        /// How much of the instruction a toast quotes back.
        const QUOTED: usize = 32;

        let Some(instruction) = task.action.clone() else {
            return;
        };
        let Some(pane) = self.model.pane_for_agent(&task.id) else {
            if !automatic {
                self.model.raise_toast(
                    ToastKind::Told,
                    "agent is not open",
                    task.label.clone(),
                    None,
                );
            }
            return;
        };
        let submission = self.model.may_submit(pane);
        match submission.refusal(&task.label) {
            None => {
                self.submit_notice(pane, &instruction);
                self.model
                    .note_agent_prompt_submission(pane, &self.identities, None);
                self.model.raise_toast(
                    ToastKind::Done,
                    "sent to agent",
                    crate::ui::widget::text::elide(&instruction, QUOTED),
                    None,
                );
            }
            Some((reason, _)) if automatic => {
                self.model.raise_toast(
                    ToastKind::Told,
                    "held back",
                    format!("{}: {reason}", task.label),
                    None,
                );
            }
            Some((title, detail)) => {
                self.model.raise_toast(ToastKind::Told, title, detail, None);
            }
        }
    }

    /// What the task evaluations answered: branches, targets, syncs and
    /// the tasks themselves, and any evaluation asked for again while
    /// one was out.
    pub(in crate::ui::orchestrator) fn absorb_task_evaluations(&mut self) {
        let mut asked_again = Vec::new();
        while let Ok(resolution) = self.channels.tasks.receiver.try_recv() {
            let absorbed = self
                .model
                .remembered
                .absorbed_generation
                .entry(resolution.key.clone())
                .or_default();
            if let Some((primary, tasks)) = resolution.remote {
                // Only onto the evaluation it finishes: a later one
                // already drew states this answer was asked before.
                if *absorbed <= resolution.generation {
                    self.model.remembered.tasks.insert(primary, tasks);
                    self.model.dirty = true;
                }
                continue;
            }
            *absorbed = (*absorbed).max(resolution.generation);
            self.model
                .remembered
                .task_eval_pending
                .remove(&resolution.key);
            asked_again.extend(
                self.model
                    .remembered
                    .task_eval_again
                    .remove(&resolution.key),
            );
            self.model
                .remembered
                .evaluated
                .insert(resolution.key.clone());
            let Some(EvaluationAnswer {
                primary,
                branch,
                target,
                sync,
                evaluation,
            }) = resolution.answered
            else {
                continue;
            };
            match branch {
                Some(branch) => self
                    .model
                    .remembered
                    .branches
                    .insert(resolution.key.clone(), branch),
                None => self.model.remembered.branches.remove(&resolution.key),
            };
            match target {
                Some(target) => self
                    .model
                    .remembered
                    .targets
                    .insert(resolution.key.clone(), target),
                None => self.model.remembered.targets.remove(&resolution.key),
            };
            match sync {
                Some(sync) => self
                    .model
                    .remembered
                    .upstream_syncs
                    .insert(resolution.key.clone(), sync),
                None => self.model.remembered.upstream_syncs.remove(&resolution.key),
            };
            // A store that could not be read is not a repository without
            // tasks, and must never be drawn as one: replacing what the
            // client already knew with an empty list takes every agent's
            // branch, mark and status away and puts nothing in
            // their place. The last good answer stands, and the reason is
            // said instead.
            if let Some(reason) = evaluation.unreadable {
                self.model
                    .raise_toast(ToastKind::Failed, "agents unreadable", reason, None);
                continue;
            }
            // Said before the tasks are taken, because it is what those
            // tasks are: records adopted from the checkouts on disk, with
            // the labels and publication UZE had recorded left behind in
            // a document it could not read.
            if let Some(recovered) = evaluation.recovered {
                self.model
                    .raise_toast(ToastKind::Warned, "agents recovered", recovered, None);
            }
            // Said once for each new set of reasons, not on every pass:
            // the manifest is read again each time, and an unchanged typo
            // is not news.
            if evaluation.ignored_actions.is_empty() {
                self.model.remembered.ignored_actions.remove(&primary);
            } else if self.model.remembered.ignored_actions.get(&primary)
                != Some(&evaluation.ignored_actions)
            {
                if let Some(first) = evaluation.ignored_actions.first() {
                    self.model.raise_toast(
                        ToastKind::Warned,
                        "actions left out",
                        first.clone(),
                        None,
                    );
                }
                self.model
                    .remembered
                    .ignored_actions
                    .insert(primary.clone(), evaluation.ignored_actions);
            }
            let arrived: Vec<AgentView> = evaluation
                .tasks
                .iter()
                .filter(|task| evaluation.arrived.contains(&task.id))
                .cloned()
                .collect();
            self.model
                .remembered
                .tasks
                .insert(primary, evaluation.tasks);
            // A rebase this pass ran stopped on a conflict: the project's
            // instruction for it goes to the agent.
            for task in &arrived {
                self.send_instruction(task, true);
            }
            self.model.dirty = true;
        }
        for cwd in asked_again {
            self.model
                .schedule_evaluation(self.home, cwd, &self.channels.tasks.sender);
        }
    }

    /// Deliveries that ended, each said as an outcome, and what the owning
    /// agent has to act on handed to its pane.
    pub(super) fn absorb_deliveries(&mut self) {
        while let Ok(resolution) = self.channels.deliveries.receiver.try_recv() {
            // Released before anything is read out of the answer: an
            // empty one is exactly the case that used to leave the task
            // drawn as "delivering" with no way back.
            if let Some(reserved) = &resolution.reserved {
                self.model.remembered.delivery_pending.remove(reserved);
                self.model.clear_busy_notice();
            }
            for report in &resolution.reports {
                self.model
                    .remembered
                    .delivery_pending
                    .remove(&report.task.id);
                // A delivery that failed is worth an offer: the branch is
                // where it was, and trying again is the one thing the
                // reader would go looking for.
                // A gate nobody approved is a question, not an outcome:
                // the toast that asks it is the one the reader needs.
                if let DeliveryOutcome::AwaitingApproval(awaiting) = &report.outcome {
                    self.model.commands_await(awaiting.clone(), true);
                    continue;
                }
                let kind = match &report.outcome {
                    // Refused is the gate saying no, and a conflict or a
                    // failed check is work that came back: neither is a
                    // delivery, and both need the reader.
                    DeliveryOutcome::Refused { .. } => ToastKind::Failed,
                    DeliveryOutcome::Conflicted { .. } | DeliveryOutcome::Failed { .. } => {
                        ToastKind::Warned
                    }
                    _ => ToastKind::Done,
                };
                // A refusal's reason is what the reader needs, and too long
                // for a title: it goes under one, where the label would.
                let (title, detail) = match &report.outcome {
                    DeliveryOutcome::Refused(reason) => {
                        ("not delivered".to_owned(), reason.clone())
                    }
                    _ => (describe_delivery(report), report.task.label.clone()),
                };
                self.model.raise_toast(kind, title, detail, None);
                // An operation of UZE's left the work in a new state, so
                // the project's instruction for it goes to the agent.
                if matches!(
                    report.outcome,
                    DeliveryOutcome::Published { .. }
                        | DeliveryOutcome::Conflicted { .. }
                        | DeliveryOutcome::Failed { .. }
                ) {
                    self.send_instruction(&report.task, true);
                }
            }
            if resolution.reports.is_empty() {
                // "Nothing ready" answers the gesture that offered every
                // ready task and found none. A press on *one* task that
                // came back with nothing means something else entirely —
                // the record is gone, or the document holding it could not
                // be read — and said as "nothing ready" it told the
                // operator the task in front of them is not there.
                match &resolution.reserved {
                    Some(_) => self.model.raise_toast(
                        ToastKind::Failed,
                        "not delivered",
                        "its record is gone or unreadable",
                        None,
                    ),
                    None => self.model.raise_toast(
                        ToastKind::Told,
                        "nothing ready",
                        "no task has commits to deliver",
                        None,
                    ),
                }
            }
            self.model
                .schedule_evaluation(self.home, resolution.cwd, &self.channels.tasks.sender);
            self.model.dirty = true;
        }
    }

    /// Finishes and discards that ended, each said either way.
    pub(super) fn absorb_task_mutations(&mut self) {
        while let Ok(resolution) = self.channels.mutations.receiver.try_recv() {
            self.model
                .remembered
                .task_mutation_pending
                .remove(&resolution.task);
            self.model.clear_busy_notice();
            // Both endings are said. A finish whose store write failed
            // used to say nothing at all, and the re-evaluation right
            // behind it simply redrew the task unchanged — which reads as
            // the key not working.
            match resolution.outcome {
                Ok(()) => self.model.raise_toast(
                    ToastKind::Done,
                    resolution.mutation.done(),
                    resolution.label.clone(),
                    None,
                ),
                Err(error) => {
                    self.model
                        .raise_toast(ToastKind::Failed, error, resolution.label.clone(), None)
                }
            }
            // A finish or a discard changes what is preserved, and the
            // list may well be the surface the operator is looking at.
            self.sweep_preserved_work();
            self.read_again(&resolution.cwd);
            self.model
                .schedule_evaluation(self.home, resolution.cwd, &self.channels.tasks.sender);
            self.model.dirty = true;
        }
    }

    /// Asks the task questions that have gone stale: a pane that went
    /// quiet, a directory named but never read, and the refresh clock.
    pub(super) fn schedule_task_evaluations(&mut self) {
        // Readiness is a Git fact, read when a pane goes quiet and, less
        // often, on a clock — never told by the agent.
        let quiet_panes = std::mem::take(&mut self.model.recently_quiet);
        let quiet: Vec<PathBuf> = quiet_panes
            .into_iter()
            .filter_map(|pane| self.model.pane_cwd(pane))
            .collect();
        for cwd in quiet {
            self.model
                .schedule_evaluation(self.home, cwd, &self.channels.tasks.sender);
        }
        // A directory the sidebar names is read the moment it is known,
        // not when its pane next goes quiet or on the refresh clock: a
        // folded space's root or an agent nobody selected otherwise
        // showed its path for as long as `TASK_REFRESH` before its branch.
        let unread = self.model.unread_named_directories(&self.identities);
        for cwd in &unread {
            self.model
                .schedule_evaluation(self.home, cwd.clone(), &self.channels.tasks.sender);
        }
        // A directory first seen is where a space opens: its project's
        // `AGENTS.md` is brought in step before any agent there reads it.
        spawn_unspelled_gates(
            self.home,
            unread.clone(),
            self.channels.unspelled_gates.sender.clone(),
        );
        spawn_commands_awaiting(
            self.home,
            unread.clone(),
            self.channels.commands_awaiting.sender.clone(),
        );
        spawn_policy_region_sync(
            self.home,
            unread,
            self.channels.policy_regions.sender.clone(),
        );
        if self
            .model
            .remembered
            .last_task_refresh
            .is_none_or(|last| last.elapsed() >= TASK_REFRESH)
        {
            self.model.remembered.last_task_refresh = Some(Instant::now());
            // A checkout can be deleted with nothing to say so. Every other
            // trigger for the occupancy pass is an event the server sends,
            // and the server only speaks when a pane's cwd or process
            // *changed* — which, when a checkout vanishes, happens only
            // because Linux's `/proc` starts spelling the cwd
            // `<path> (deleted)`. Where the platform has no such spelling
            // the reading simply stops resolving, nothing changes, no event
            // is sent, and the row keeps offering a way into a directory
            // that is gone. Asked on this clock instead, so the answer comes
            // from the disk rather than from a quirk of how one kernel
            // renames what it lost.
            self.model.occupancy_stale = true;
            // Every space's header, not only the selected pane's: a push
            // or a pull made from a terminal leaves nothing behind for any
            // other trigger to notice, and a folded space's `↑1` stayed up
            // until one of its agents happened to go quiet.
            let directories: Vec<PathBuf> = selected_pane_cwd(&self.model)
                .into_iter()
                .chain(self.model.space_directories(&self.identities))
                .collect();
            for cwd in &directories {
                self.model
                    .schedule_evaluation(self.home, cwd.clone(), &self.channels.tasks.sender);
            }
            // The same clock keeps each project's `AGENTS.md` in step with
            // an edit to its `agents.yaml`: a sync with nothing new writes
            // nothing.
            spawn_policy_region_sync(
                self.home,
                directories,
                self.channels.policy_regions.sender.clone(),
            );
            // On the same clock, and for every agent rather than the
            // selected one: this is also where a launch left pending by a
            // client that was not running is finally resolved, well before
            // a relaunch needs the answer.
            spawn_conversation_refresh(self.home, agent_contexts(&self.model, &self.identities));
            // And on it, the machine facts agents proposed: a proposal is
            // asked about within one pass of being written.
            if !self.model.remembered.memory_proposals_pending {
                self.model.remembered.memory_proposals_pending = true;
                spawn_memory_proposals(self.home, self.channels.proposals.sender.clone());
            }
        }
        if !self.model.remembered.checkout_watch_pending
            && self
                .model
                .remembered
                .last_checkout_watch
                .is_none_or(|last| last.elapsed() >= CHECKOUT_WATCH)
        {
            self.model.remembered.last_checkout_watch = Some(Instant::now());
            self.model.remembered.checkout_watch_pending = true;
            spawn_checkout_watch(
                self.home,
                self.model.watched_checkouts(&self.identities),
                self.channels.checkout_stamps.sender.clone(),
            );
        }
        if self
            .model
            .remembered
            .last_target_sync
            .is_none_or(|last| last.elapsed() >= TARGET_SYNC)
        {
            self.model.remembered.last_target_sync = Some(Instant::now());
            spawn_target_sync(
                self.home,
                self.model.space_directories(&self.identities),
                self.channels.target_syncs.sender.clone(),
            );
        }
    }

    /// Pulls the target into the focused checkout, as an editor's sync
    /// count does when clicked. One at a time: a second click while one
    /// runs is the same pull.
    pub(in crate::ui::orchestrator) fn pull_target(&mut self) {
        if self.model.remembered.pull_pending {
            return;
        }
        let Some(cwd) = self.model.focused_cwd() else {
            return;
        };
        self.model.remembered.pull_pending = true;
        let target = self
            .model
            .remembered
            .targets
            .get(&evaluation_key(&cwd))
            .cloned()
            .unwrap_or_else(|| "the target".to_owned());
        self.model.set_busy_notice(format!("pulling {target}"));
        spawn_pull(self.home, cwd, self.channels.pulls.sender.clone());
        self.model.dirty = true;
    }

    /// A pull that ended, said as an outcome, and the checkout read again
    /// so its counts follow.
    pub(super) fn absorb_pulls(&mut self) {
        while let Ok(resolution) = self.channels.pulls.receiver.try_recv() {
            self.model.remembered.pull_pending = false;
            self.model.clear_busy_notice();
            match resolution.report {
                Some(report) => match report.pulled {
                    Ok(0) => self.model.raise_toast(
                        ToastKind::Told,
                        "already up to date",
                        report.target,
                        None,
                    ),
                    Ok(commits) => {
                        let plural = if commits == 1 { "" } else { "s" };
                        self.model.raise_toast(
                            ToastKind::Done,
                            format!("pulled {commits} commit{plural}"),
                            report.target,
                            None,
                        );
                    }
                    Err(reason) => {
                        self.model
                            .raise_toast(ToastKind::Failed, "pull refused", reason, None)
                    }
                },
                None => self.model.raise_toast(
                    ToastKind::Failed,
                    "pull refused",
                    "no target to pull",
                    None,
                ),
            }
            self.model
                .schedule_evaluation(self.home, resolution.cwd, &self.channels.tasks.sender);
            self.model.dirty = true;
        }
    }

    /// Checkouts whose stamp moved since it was last read: Git recorded
    /// something there, so its repository is evaluated now rather than on
    /// the refresh clock. A checkout read for the first time only sets its
    /// stamp — the evaluation that put it on screen already read it.
    pub(in crate::ui::orchestrator) fn absorb_checkout_stamps(&mut self) {
        while let Ok(resolution) = self.channels.checkout_stamps.receiver.try_recv() {
            self.model.remembered.checkout_watch_pending = false;
            // Each answer is every checkout watched now: one no longer
            // asked about is forgotten, so coming back is a first read.
            let watched: BTreeSet<&PathBuf> = resolution
                .stamps
                .iter()
                .map(|(checkout, _)| checkout)
                .collect();
            self.model
                .remembered
                .checkout_stamps
                .retain(|checkout, _| watched.contains(checkout));
            for (checkout, stamp) in resolution.stamps {
                let Some(stamp) = stamp else {
                    self.model.remembered.checkout_stamps.remove(&checkout);
                    continue;
                };
                let before = self
                    .model
                    .remembered
                    .checkout_stamps
                    .insert(checkout.clone(), stamp);
                if before.is_some_and(|before| before != stamp) {
                    self.model.schedule_evaluation(
                        self.home,
                        checkout,
                        &self.channels.tasks.sender,
                    );
                }
            }
        }
    }

    /// A local target carrying commits its remote lacks, which is the one
    /// case in which a new agent starts behind the remote: said when the
    /// project falls behind and not again until it has caught up, since the
    /// remote moving further is the same news, and the operator is the one
    /// who can push or rebase what only they have.
    pub(super) fn absorb_target_syncs(&mut self) {
        while let Ok(report) = self.channels.target_syncs.receiver.try_recv() {
            let behind = &mut self.model.remembered.target_sync_behind;
            let Some(concern) = report.concern else {
                behind.remove(&report.project);
                continue;
            };
            if !behind.insert(report.project) {
                continue;
            }
            self.model
                .raise_toast(ToastKind::Warned, "target diverged", concern, None);
        }
    }

    /// What the Git badge, the release notes, the commit detail and the
    /// code and architect surfaces' reads answered.
    /// What keeping `AGENTS.md` in step could not do: said once a session
    /// per file, never repaired over the operator's edit.
    /// The registry's launcher names, asked once and absorbed when they
    /// arrive, so a bypass already on screen is noticed without waiting for
    /// the next status tick.
    pub(super) fn absorb_launchers(&mut self) {
        if !self.model.remembered.launchers_asked {
            self.model.remembered.launchers_asked = true;
            spawn_launcher_names(self.home, self.channels.launchers.sender.clone());
        }
        while let Ok(names) = self.channels.launchers.receiver.try_recv() {
            self.model.remembered.launchers = Some(names);
            self.model.note_launcher_bypass();
        }
    }

    pub(super) fn absorb_policy_regions(&mut self) {
        while let Ok(resolution) = self.channels.policy_regions.receiver.try_recv() {
            if !self
                .model
                .remembered
                .policy_region_reported
                .insert(resolution.file.clone())
            {
                continue;
            }
            let (kind, title) = if resolution.drifted {
                (ToastKind::Warned, "AGENTS.md edited by hand")
            } else {
                (ToastKind::Failed, "AGENTS.md not updated")
            };
            self.model
                .raise_toast(kind, title, resolution.problem, None);
        }
    }

    /// A project whose gates this machine cannot run: said once a session,
    /// before a delivery from it is refused for that.
    pub(super) fn absorb_unspelled_gates(&mut self) {
        while let Ok(answer) = self.channels.unspelled_gates.receiver.try_recv() {
            if !self
                .model
                .remembered
                .unspelled_gates_reported
                .insert(answer.project.clone())
            {
                continue;
            }
            let gates = answer
                .gates
                .iter()
                .map(|gate| format!("`{gate}`"))
                .collect::<Vec<_>>()
                .join(", ");
            self.model.raise_toast(
                ToastKind::Warned,
                "gate can't run here",
                format!("no `{}` form of {gates}", answer.platform),
                None,
            );
        }
    }

    /// A project's commands waiting for the operator, read where it
    /// opens, and what approving them answered.
    pub(super) fn absorb_command_approvals(&mut self) {
        while let Ok(awaiting) = self.channels.commands_awaiting.receiver.try_recv() {
            self.model.commands_await(awaiting, false);
        }
        while let Ok(resolution) = self.channels.proposals.receiver.try_recv() {
            self.model.absorb_proposals(resolution);
        }
        while let Ok(resolution) = self.channels.proposal_answers.receiver.try_recv() {
            self.model.absorb_proposal_answer(resolution);
        }
        while let Ok(resolution) = self.channels.approvals.receiver.try_recv() {
            self.model.absorb_approval(resolution);
        }
    }

    pub(super) fn absorb_surface_answers(&mut self) {
        self.absorb_space_overview_answers();
        while let Ok(resolution) = self.channels.git.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_git_read(resolution);
        }
        while let Ok(resolution) = self.channels.release_notes.receiver.try_recv() {
            if let Some(modal) = &mut self.model.release_notes {
                self.model.dirty |= modal.absorb(&resolution.version, resolution.notes);
            }
        }
        while let Ok(resolution) = self.channels.commit_details.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_commit_detail(resolution);
        }
        while let Ok(resolution) = self.channels.code_changes.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_changes(resolution);
        }
        while let Ok(resolution) = self.channels.code_diffs.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_diff(resolution);
        }
        while let Ok(resolution) = self.channels.code_files.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_file_answer(resolution);
        }
        while let Ok(resolution) = self.channels.code_measures.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_measure(resolution);
        }
        while let Ok(resolution) = self.channels.artifacts.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_artifacts(resolution);
        }
        while let Ok(resolution) = self.channels.spec.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_spec(resolution);
        }
        while let Ok(resolution) = self.channels.spec_summaries.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_spec_summary(resolution);
        }
    }

    /// Asks whatever those same surfaces now show and have not read.
    pub(super) fn schedule_surface_reads(&mut self) {
        self.model.schedule_git_read(&self.channels.git.sender);
        self.model
            .schedule_diff_read(&self.channels.code_diffs.sender);
        self.model
            .schedule_changes_refresh(&self.channels.code_changes.sender);
        self.model
            .schedule_file_request(&self.channels.code_files.sender);
        self.model
            .schedule_code_measure(&self.channels.code_measures.sender);
        self.model
            .schedule_artifacts_read(&self.channels.artifacts.sender);
        self.model
            .schedule_spec_read(self.home, &self.channels.spec.sender);
        self.model
            .schedule_spec_summary(&self.channels.spec_summaries.sender);
    }

    /// Advances the activity spinner while anything it animates is on
    /// screen.
    pub(super) fn turn_activity_clock(&mut self) {
        // The same clock drives the notice chip's spinner, the delivering
        // button's, and a caption sliding under the pointer, so it has to
        // turn for any of them even with every agent idle.
        if workspace_has_active_agent_operation(&self.model, &self.identities)
            || self.model.notice_is_busy()
            || self.model.marquee
            || !self.model.remembered.delivery_pending.is_empty()
        {
            let now = Instant::now();
            if now >= self.next_tick {
                self.spinner.inc(1);
                self.model.tick = self.spinner.position() as usize;
                // Scheduled from the beat it was due on, not from when the
                // loop got round to it: counting from `now` let every late
                // check stretch that one frame, and the pulse stuttered.
                // A clock that fell more than a beat behind (the clock was
                // idle, or a frame was slow) starts over rather than
                // racing through the frames it missed.
                self.next_tick += AGENT_ACTIVITY_TICK;
                if self.next_tick <= now {
                    self.next_tick = now + AGENT_ACTIVITY_TICK;
                }
                self.model.dirty = true;
            }
        }
    }
}
