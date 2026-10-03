//! The work modal: its projects, its rows, the questions it asks and the changes it makes to checkouts and preserved work.

use super::*;

impl Attach<'_> {
    /// The work modal: moving between its projects and its rows, closing
    /// it, and everything asked of the row in front.
    pub(in crate::ui::orchestrator) fn work_action(&mut self, action: Action, viewport: &Viewport) {
        let Some(work) = self.model.work.as_mut() else {
            return;
        };
        match action {
            Action::ToggleWork => self.model.work = None,
            Action::NextProject | Action::PreviousProject => {
                self.step_project(action == Action::NextProject);
            }
            Action::Dismiss => {
                if !work.withdraw() {
                    self.model.work = None;
                }
            }
            _ => self.row_action(action, viewport),
        }
        self.model.dirty = true;
    }

    /// Opens the work modal on the project `key` names or, with none, on
    /// the first that needs the operator. Preserved work is swept again and
    /// the project in front is read; each is drawn from its last answer
    /// while the new one is out, since an empty list that fills a moment
    /// later reads as work having been lost.
    pub(in crate::ui::orchestrator) fn open_work(&mut self, key: Option<PathBuf>) {
        self.sweep_preserved_work();
        self.model.work = Some(WorkOverlay::open(key));
        self.read_front_project();
        self.model.dirty = true;
    }

    /// The project in front and its rows, as the modal draws them.
    pub(super) fn work_in_front(&self) -> Option<(Project, Vec<WorkRow>)> {
        let overlay = self.model.work.as_ref()?;
        let (_, project) = front(&self.model, overlay)?;
        let rows = rows_of(&self.model, overlay, &project);
        Some((project, rows))
    }

    /// Reads the checkouts of the project in front, the first time it is.
    pub(super) fn read_front_project(&mut self) {
        let Some((project, _)) = self.work_in_front() else {
            return;
        };
        let read = self
            .model
            .work
            .as_ref()
            .is_some_and(|work| work.reads.contains_key(&project.key));
        if !read {
            self.read_project(project.key);
        }
    }

    /// Asks for `key`'s checkouts again, whether or not a read of it is
    /// out: the answer to the earlier one is dropped when it lands.
    pub(super) fn read_project(&mut self, key: PathBuf) {
        let occupied: Vec<PathBuf> = self
            .model
            .remembered
            .occupied_checkouts
            .iter()
            .cloned()
            .collect();
        let Some(work) = self.model.work.as_mut() else {
            return;
        };
        self.model.remembered.checkouts_asked += 1;
        let asked = self.model.remembered.checkouts_asked;
        work.reads
            .entry(key.clone())
            .and_modify(|read| {
                read.asked = asked;
                read.pending = true;
            })
            .or_insert(ProjectRead {
                asked,
                pending: true,
                answer: None,
            });
        spawn_checkouts(
            self.home,
            key,
            asked,
            occupied,
            self.channels.checkouts.sender.clone(),
        );
    }

    /// Reads again every project the modal has read that `path` names,
    /// after something changed what it holds.
    pub(super) fn read_again(&mut self, path: &Path) {
        let keys: Vec<PathBuf> = self
            .model
            .work
            .as_ref()
            .map(|work| {
                work.reads
                    .keys()
                    .filter(|key| {
                        key.as_path() == path
                            || work.view(key).is_some_and(|view| view.primary == path)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for key in keys {
            self.read_project(key);
        }
    }

    pub(super) fn step_project(&mut self, forward: bool) {
        let Some(work) = self.model.work.as_ref() else {
            return;
        };
        let projects = projects(&self.model, work);
        let count = projects.len();
        if count == 0 {
            return;
        }
        let current = front(&self.model, work).map_or(0, |(index, _)| index);
        let next = if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        self.select_project(projects[next].key.clone());
    }

    pub(in crate::ui::orchestrator) fn select_project(&mut self, key: PathBuf) {
        if let Some(work) = self.model.work.as_mut() {
            work.withdraw();
            if work.project.as_ref() != Some(&key) {
                work.selected = 0;
            }
            work.project = Some(key);
        }
        self.read_front_project();
    }

    /// Keeps the selection on a row after the list changed under it.
    pub(super) fn keep_work_selection(&mut self) {
        let count = self.work_in_front().map_or(0, |(_, rows)| rows.len());
        if let Some(work) = self.model.work.as_mut() {
            work.selected = work.selected.min(count.saturating_sub(1));
        }
    }

    pub(super) fn ask_about_work(&mut self, question: WorkQuestion) {
        if let Some(work) = self.model.work.as_mut() {
            work.asking = Some(question);
        }
    }

    /// Anything asked of the list: walking it, answering its question,
    /// cleaning up, or acting on the row in front. A key moves on from a
    /// question it does not answer.
    pub(super) fn row_action(&mut self, action: Action, viewport: &Viewport) {
        let Some((project, rows)) = self.work_in_front() else {
            return;
        };
        let Some(work) = self.model.work.as_mut() else {
            return;
        };
        let asking = work.asking.take();
        let row = rows.get(work.selected).cloned();
        let answered = work.view(&project.key).is_some();
        match action {
            Action::SelectNext => {
                work.selected = (work.selected + 1).min(rows.len().saturating_sub(1));
            }
            Action::SelectPrevious => work.selected = work.selected.saturating_sub(1),
            Action::ConfirmDiscard => {
                if let Some(question) = asking {
                    self.go_ahead(question, &project, row.as_ref());
                }
            }
            Action::CleanUpCheckouts => {
                if rows
                    .iter()
                    .filter_map(|row| row.checkout.as_ref())
                    .any(cleaned_up)
                {
                    self.ask_about_work(WorkQuestion::CleanUp);
                } else if answered {
                    self.model.raise_toast(
                        ToastKind::Told,
                        "nothing to clean up",
                        "no checkout of yours is clean, unused and in the target",
                        None,
                    );
                }
            }
            _ => {
                if let Some(row) = row {
                    self.act_on_row(action, row, viewport);
                }
            }
        }
    }

    /// One action on the row in front. A row it does not apply to says
    /// why at once, rather than leaving the key to look broken.
    pub(super) fn act_on_row(&mut self, action: Action, row: WorkRow, viewport: &Viewport) {
        let title = row.title().to_owned();
        let refuse = |model: &mut WorkspaceModel, heading: &str, why: String| {
            model.raise_toast(ToastKind::Failed, heading, format!("{title}: {why}"), None);
        };
        let not_a_task = "it holds no task of UZE's, only a checkout".to_owned();
        match (action, &row.task, &row.checkout) {
            // A task's slot is never opened as a space of its own: work is
            // bound to its project's space, so a space rooted at the slot
            // would hold nothing the task does and hand every agent asked
            // for there to the project's space instead. Entering a task
            // resumes it, where it belongs.
            (Action::Activate, None, _) => {
                if let Some(directory) = row.directory() {
                    let directory = directory.to_path_buf();
                    self.model.work = None;
                    self.open_space_at(directory, viewport.columns, viewport.rows);
                }
            }
            // Placement answers with the task's own slot when it still has
            // one, and otherwise gives it a slot again on its own branch — a
            // checkout removed by hand took only the uncommitted work.
            // Either way the launch carries the task's identity.
            (Action::Activate | Action::ResumeTask, Some(task), _) => {
                let resume = ResumeTarget {
                    primary: task.project.clone(),
                    task: task.id.clone(),
                    // Asked for from the list, not from a row: there is no
                    // dead tab behind it.
                    replacing: None,
                };
                self.model.work = None;
                let anchor = self.new_agent_anchor();
                self.offer_agents(anchor, Some(resume));
            }
            (Action::DeliverTask, Some(task), _) => {
                self.model
                    .remembered
                    .delivery_pending
                    .insert(task.id.clone());
                spawn_delivery(
                    self.home,
                    task.project.clone(),
                    Some(task.id.clone()),
                    self.channels.deliveries.sender.clone(),
                );
            }
            (Action::FinishTask, Some(task), _) => {
                let task = task.clone();
                self.mutate_preserved(&task, WorkMutation::Finish);
            }
            (Action::ResumeTask, None, _) => refuse(&mut self.model, "not resumed", not_a_task),
            (Action::DeliverTask, None, _) => refuse(&mut self.model, "not delivered", not_a_task),
            (Action::FinishTask, None, _) => refuse(&mut self.model, "not marked done", not_a_task),
            (Action::JoinCheckout, _, Some(checkout)) if join_of(checkout).is_some() => {
                self.ask_about_work(WorkQuestion::Join);
            }
            (Action::JoinCheckout, _, checkout) => refuse(
                &mut self.model,
                "not joined",
                checkout.as_ref().map_or_else(
                    || "only a subagent's checkout joins into its agent".to_owned(),
                    join_refusal,
                ),
            ),
            (Action::AdoptCheckout, None, Some(checkout)) if checkout.adoptable => {
                self.ask_about_work(WorkQuestion::Adopt);
            }
            (Action::AdoptCheckout, ..) => refuse(
                &mut self.model,
                "not adopted",
                "only a checkout of yours directly under .worktrees/ can be adopted".to_owned(),
            ),
            (Action::DiscardTask, Some(_), _) => self.ask_about_work(WorkQuestion::Discard),
            (Action::DiscardTask, None, Some(checkout)) => match &checkout.removal_refusal {
                Some(reason) => refuse(&mut self.model, "not removed", reason.clone()),
                None => self.ask_about_work(WorkQuestion::Remove),
            },
            _ => {}
        }
    }

    /// Makes the change the modal asked about, now that it is confirmed.
    pub(super) fn go_ahead(
        &mut self,
        question: WorkQuestion,
        project: &Project,
        row: Option<&WorkRow>,
    ) {
        let checkout = row.and_then(|row| row.checkout.as_ref());
        let change = match question {
            WorkQuestion::Discard => {
                if let Some(task) = row.and_then(|row| row.task.clone()) {
                    self.mutate_preserved(&task, WorkMutation::Discard);
                }
                None
            }
            WorkQuestion::Remove => checkout.map(|checkout| CheckoutChange::Remove {
                path: checkout.path.clone(),
                name: checkout.name.clone(),
            }),
            WorkQuestion::Adopt => checkout.map(|checkout| CheckoutChange::Adopt {
                path: checkout.path.clone(),
                name: checkout.name.clone(),
            }),
            WorkQuestion::Join => checkout.and_then(join_of),
            WorkQuestion::CleanUp => Some(CheckoutChange::CleanUp),
        };
        if let Some(change) = change {
            self.change_checkouts(project.key.clone(), change);
        }
    }

    /// Makes one change to `project`'s checkouts, off this thread. One at
    /// a time: a second started while the first is still removing
    /// directories would inspect what the first is taking away.
    pub(super) fn change_checkouts(&mut self, project: PathBuf, change: CheckoutChange) {
        if std::mem::replace(&mut self.model.remembered.checkout_change_pending, true) {
            return;
        }
        self.model.set_busy_notice(match &change {
            CheckoutChange::Adopt { name, .. } => format!("adopting {name}"),
            CheckoutChange::Remove { name, .. } => format!("removing {name}"),
            CheckoutChange::Join { topic, parent, .. } => format!("joining {topic} into {parent}"),
            CheckoutChange::CleanUp => "cleaning up checkouts".to_owned(),
        });
        spawn_checkout_change(
            self.home,
            project,
            change,
            self.model
                .remembered
                .occupied_checkouts
                .iter()
                .cloned()
                .collect(),
            self.channels.checkout_changes.sender.clone(),
        );
    }

    /// A read of a project's checkouts, kept only while it answers the
    /// last question asked about that project in the open modal.
    pub(super) fn absorb_checkouts(&mut self) {
        while let Ok(resolution) = self.channels.checkouts.receiver.try_recv() {
            let Some(read) = self
                .model
                .work
                .as_mut()
                .and_then(|work| work.reads.get_mut(&resolution.project))
                .filter(|read| read.asked == resolution.asked)
            else {
                continue;
            };
            read.pending = false;
            read.answer = Some(resolution.view);
            self.keep_work_selection();
            self.model.dirty = true;
        }
    }

    /// Changes to the checkouts that ended, each said either way; the
    /// project, the preserved work and the project's tasks are read again,
    /// since a slot may have come or gone and a join changes what an agent
    /// holds.
    pub(super) fn absorb_checkout_changes(&mut self) {
        while let Ok(resolution) = self.channels.checkout_changes.receiver.try_recv() {
            self.model.remembered.checkout_change_pending = false;
            self.model.clear_busy_notice();
            let (kind, title, detail) = describe_change(&resolution.outcome);
            self.model.raise_toast(kind, title, detail, None);
            self.read_again(&resolution.project);
            if matches!(resolution.outcome, CheckoutOutcome::Joined { .. }) {
                self.sweep_preserved_work();
            }
            self.model.schedule_evaluation(
                self.home,
                resolution.project,
                &self.channels.tasks.sender,
            );
            self.model.dirty = true;
        }
    }

    /// Re-reads every project's preserved work, off the UI thread.
    ///
    /// Asked once at a time: the sweep opens `$UZE_HOME` and walks every
    /// project UZE has recorded, and a second one in flight would answer
    /// the same question twice.
    pub(super) fn sweep_preserved_work(&mut self) {
        if self.model.remembered.preserved_pending {
            return;
        }
        self.model.remembered.preserved_pending = true;
        spawn_preserved_sweep(self.home, self.channels.preserved.sender.clone());
    }

    /// Finishes or discards one piece of preserved work, off this thread.
    ///
    /// The row carries the project it belongs to, rather than borrowing
    /// whichever one the operator happens to be looking at — which is what
    /// lets this list cross projects at all.
    ///
    /// Reserved under the work's own id, because a discard removes a whole
    /// checkout and a second Enter arriving while the first removal is
    /// still walking it must not start another. The busy notice is the only
    /// thing said until the answer lands: unlike a delivery there is no
    /// button drawn for this, so silence would read as the key doing
    /// nothing.
    pub(super) fn mutate_preserved(
        &mut self,
        work: &uze_application::PreservedWork,
        mutation: WorkMutation,
    ) {
        if !self
            .model
            .remembered
            .task_mutation_pending
            .insert(work.id.clone())
        {
            return;
        }
        self.model
            .set_busy_notice(format!("{}: {}", work.label, mutation.underway()));
        spawn_task_mutation(
            self.home,
            work.project.clone(),
            work.id.clone(),
            work.label.clone(),
            mutation,
            self.channels.mutations.sender.clone(),
        );
    }
}
