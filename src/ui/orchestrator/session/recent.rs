//! Recent: opening and leaving it, walking its rows, and what each row's
//! keys do to the agent it lists.

use super::*;

impl Attach<'_> {
    /// Opens recent: the machine's kept work is swept again and every
    /// project holding some is read, each drawn from its last answer while
    /// the new one is out.
    pub(in crate::ui::orchestrator) fn open_recent(&mut self) {
        self.model.recent.open = true;
        self.model.recent.asking = false;
        self.sweep_preserved_work();
        for project in projects_to_read(&self.model) {
            self.read_project(project);
        }
        self.model.dirty = true;
    }

    pub(super) fn close_recent(&mut self) {
        self.model.recent.open = false;
        self.model.recent.asking = false;
        self.model.dirty = true;
    }

    /// Everything asked of recent while it is open.
    pub(in crate::ui::orchestrator) fn recent_action(&mut self, action: Action) {
        let rows = recent_rows(&self.model);
        let index = selected_index(&self.model.recent, &rows);
        let row = index.and_then(|index| rows.get(index)).cloned();
        if self.model.recent.asking {
            match action {
                Action::FocusNext | Action::FocusPrevious => {
                    self.model.recent.on_confirm = !self.model.recent.on_confirm;
                }
                Action::Activate => {
                    self.model.recent.asking = false;
                    if self.model.recent.on_confirm
                        && let Some(row) = row
                    {
                        self.take_away(&row);
                    }
                }
                _ => self.model.recent.asking = false,
            }
            self.model.dirty = true;
            return;
        }
        match action {
            Action::ToggleRecent | Action::Dismiss => self.close_recent(),
            Action::SelectNext | Action::SelectPrevious => {
                let step = if action == Action::SelectNext { 1 } else { -1 };
                self.step_recent(&rows, index, step);
            }
            Action::Activate => {
                if let (Some(row), Some(index)) = (row, index) {
                    self.act_on_recent_row(&row, index);
                }
            }
            Action::DeliverTask => {
                if let Some(task) = row.filter(RecentRow::delivers).and_then(|row| row.task) {
                    self.deliver_kept(&task);
                }
            }
            Action::DiscardTask if row.as_ref().and_then(RecentRow::taking_away).is_some() => {
                self.model.recent.asking = true;
                self.model.recent.on_confirm = false;
            }
            _ => {}
        }
        self.model.dirty = true;
    }

    /// A click on one of the question's answers: the same as walking to it
    /// and pressing enter.
    pub(in crate::ui::orchestrator) fn answer_recent(&mut self, yes: bool) {
        self.model.recent.on_confirm = yes;
        self.recent_action(Action::Activate);
    }

    /// A drag that began on recent's scrollbar handle: along the divider it
    /// scrolls the list and leaves the selection where it is; across it, it
    /// hands the gesture to the divider, which resizes the column.
    pub(super) fn drag_recent_edge(&mut self, column: u16, row: u16) {
        let Some(mut grab) = self.model.recent.grab else {
            return;
        };
        let intent = grab.decide(column, row);
        self.model.recent.grab = Some(grab);
        match intent {
            Some(EdgeIntent::Scroll) => self.scroll_recent_to(row),
            Some(EdgeIntent::Resize) => {
                self.model.recent.grab = None;
                self.model.dragging_sidebar = true;
            }
            None => {}
        }
    }

    /// Shows the part of the list a point on the scrollbar names.
    fn scroll_recent_to(&mut self, row: u16) {
        if let Some(bar) = self.model.recent.bar {
            self.model.recent.scroll.first = bar.first_at(row);
            self.model.dirty = true;
        }
    }

    /// Moves the list a line under the wheel, leaving the selection where
    /// it is.
    pub(in crate::ui::orchestrator) fn scroll_recent(&mut self, direction: ScrollDirection) {
        self.model.recent.scroll = self.model.recent.scroll.scrolled(direction);
        self.model.dirty = true;
    }

    /// Selects the row `index` names: a click on the row already selected
    /// does what enter would.
    pub(in crate::ui::orchestrator) fn click_recent_row(&mut self, index: usize) {
        let rows = recent_rows(&self.model);
        let Some(row) = rows.get(index) else {
            return;
        };
        let already = selected_index(&self.model.recent, &rows) == Some(index);
        self.model.recent.asking = false;
        self.model.recent.selected = Some(row.key.clone());
        if already {
            let row = row.clone();
            self.act_on_recent_row(&row, index);
        }
        self.model.dirty = true;
    }

    fn step_recent(&mut self, rows: &[RecentRow], index: Option<usize>, step: isize) {
        let Some(index) = index else {
            return;
        };
        let next = index
            .saturating_add_signed(step)
            .min(rows.len().saturating_sub(1));
        self.model.recent.selected = rows.get(next).map(|row| row.key.clone());
    }

    /// Enter on a row: join a subagent where its agent left it, resume
    /// anything else that has a task. Resuming leaves recent, since the
    /// agent is about to be a row of the tree.
    fn act_on_recent_row(&mut self, row: &RecentRow, index: usize) {
        if let (Some(join), Reason::JoinReady) = (&row.join, row.reason) {
            self.join_kept(row.project.clone(), join.clone());
            return;
        }
        let Some(task) = row.task.as_ref().filter(|_| row.primary().is_some()) else {
            return;
        };
        let anchor = self.recent_row_anchor(index);
        self.close_recent();
        // Placement answers with the task's own slot when it still has
        // one, and otherwise gives it a slot again on its own branch — a
        // checkout removed by hand took only the uncommitted work.
        // Either way the launch carries the task's identity.
        self.offer_agents(
            anchor,
            Some(ResumeTarget {
                primary: task.project.clone(),
                task: task.id.clone(),
                replacing: None,
                harness: Some(task.harness.clone()),
            }),
        );
    }

    /// Where the agent picker a resume opens hangs from: the row it was
    /// asked from, as last drawn.
    fn recent_row_anchor(&self, index: usize) -> Rect {
        self.model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == WorkspaceHit::RecentRow(index)).then_some(*rect))
            .unwrap_or_default()
    }

    fn take_away(&mut self, row: &RecentRow) {
        let Some(task) = &row.task else {
            return;
        };
        let mutation = if row.reason == Reason::ProjectGone {
            WorkMutation::Forget
        } else {
            WorkMutation::Discard
        };
        self.mutate_preserved(task, mutation);
    }

    fn deliver_kept(&mut self, task: &uze_application::PreservedWork) {
        if !self
            .model
            .remembered
            .delivery_pending
            .insert(task.id.clone())
        {
            return;
        }
        spawn_delivery(
            self.home,
            task.project.clone(),
            Some(task.id.clone()),
            self.channels.deliveries.sender.clone(),
        );
    }

    /// Joins a subagent into its agent, off this thread: the join replays
    /// commits and removes a checkout. One at a time, since a second
    /// started beside it would inspect what the first is taking away.
    fn join_kept(&mut self, project: PathBuf, join: Join) {
        if std::mem::replace(&mut self.model.remembered.join_pending, true) {
            return;
        }
        self.model
            .set_busy_notice(format!("joining {} into {}", join.topic, join.parent));
        spawn_join(self.home, project, join, self.channels.joins.sender.clone());
    }

    /// Asks for `key`'s checkouts again, whether or not a read of it is
    /// out: the answer to the earlier one is dropped when it lands.
    pub(super) fn read_project(&mut self, key: PathBuf) {
        self.model.remembered.checkouts_asked += 1;
        let asked = self.model.remembered.checkouts_asked;
        self.model
            .recent
            .reads
            .entry(key.clone())
            .and_modify(|read| read.asked = asked)
            .or_insert(ProjectRead {
                asked,
                answer: None,
            });
        spawn_checkouts(
            self.home,
            key,
            asked,
            self.channels.checkouts.sender.clone(),
        );
    }

    /// Reads `project` again, after something changed what it holds, when
    /// recent has read it before.
    pub(super) fn read_again(&mut self, project: &Path) {
        if self.model.recent.reads.contains_key(project) {
            self.read_project(project.to_path_buf());
        }
    }

    /// A read of a project's checkouts, kept only while it answers the
    /// last question asked about that project.
    pub(super) fn absorb_checkouts(&mut self) {
        while let Ok(resolution) = self.channels.checkouts.receiver.try_recv() {
            let reads = &mut self.model.recent.reads;
            if reads
                .get(&resolution.project)
                .is_none_or(|read| read.asked != resolution.asked)
            {
                continue;
            }
            // Kept under the repository's primary checkout, whichever
            // directory of it was asked about: that is the key every row
            // looks its project up by.
            let primary = resolution
                .view
                .as_ref()
                .map_or_else(|| resolution.project.clone(), |view| view.primary.clone());
            if primary != resolution.project {
                reads.remove(&resolution.project);
            }
            let answer = Some(resolution.view);
            match reads.get_mut(&primary) {
                // A newer read of the same repository is out: this answer
                // fills in until it lands, and is not what it waits for.
                Some(read) if read.asked > resolution.asked => {
                    read.answer.get_or_insert(answer.flatten());
                }
                _ => {
                    reads.insert(
                        primary,
                        ProjectRead {
                            asked: resolution.asked,
                            answer,
                        },
                    );
                }
            }
            self.model.dirty = true;
        }
    }

    /// Joins that ended, each said either way; the project and the kept
    /// work are read again, since a join changes what an agent holds.
    pub(super) fn absorb_joins(&mut self) {
        while let Ok(resolution) = self.channels.joins.receiver.try_recv() {
            self.model.remembered.join_pending = false;
            self.model.clear_busy_notice();
            let (kind, title, detail) = describe_join(&resolution);
            self.model.raise_toast(kind, title, detail, None);
            self.read_again(&resolution.project);
            self.sweep_preserved_work();
            self.model.schedule_evaluation(
                self.home,
                resolution.project,
                &self.channels.tasks.sender,
            );
            self.model.dirty = true;
        }
    }

    /// Names recorded for agents: a failure is said, since the tab shows
    /// the name either way and only the record would quietly lack it.
    pub(super) fn absorb_labels(&mut self) {
        while let Ok(resolution) = self.channels.labels.receiver.try_recv() {
            match resolution.outcome {
                Ok(()) => self.sweep_preserved_work(),
                Err(error) => self.model.raise_toast(
                    ToastKind::Failed,
                    "name not kept",
                    format!("{}: {error}", resolution.label),
                    None,
                ),
            }
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

    /// Discards or forgets one piece of preserved work, off this thread.
    ///
    /// The row carries the project it belongs to, rather than borrowing
    /// whichever one the operator happens to be looking at — which is what
    /// lets this list cross projects at all.
    ///
    /// Reserved under the work's own id, because a discard removes a whole
    /// checkout and a second Enter arriving while the first removal is
    /// still walking it must not start another. The busy notice is the only
    /// thing said until the answer lands, since silence would read as the
    /// key doing nothing.
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
