//! Asking for each background read when it is due, and absorbing its answer when it is still about what is on screen.

use super::*;

impl WorkspaceModel {
    pub(super) fn schedule_evaluation(
        &mut self,
        home: &UzeHome,
        cwd: PathBuf,
        sender: &mpsc::Sender<WorkResolution>,
    ) {
        let key = evaluation_key(&cwd);
        if !self.remembered.task_eval_pending.insert(key.clone()) {
            self.remembered.task_eval_again.insert(key, cwd);
            return;
        }
        let occupied: Vec<PathBuf> = self.remembered.occupied_checkouts.iter().cloned().collect();
        spawn_task_evaluation(home, key, cwd, occupied, sender.clone());
    }

    /// The working directory the badge and the timeline are about: the
    /// focused pane of the selected tab.
    pub(super) fn focused_cwd(&self) -> Option<PathBuf> {
        let session = self.session.as_ref()?;
        let tab = session.selected_tab();
        Some(tab.pane.cwd.clone())
    }

    /// Asks for whatever the badge is missing, on a thread of its own.
    ///
    /// Cheap enough to call every tick — it compares two instants and a
    /// path — which is the point: the read it schedules is the expensive
    /// half, and it now happens where nobody is waiting for a frame.
    pub(super) fn schedule_git_read(&mut self, sender: &mpsc::Sender<GitResolution>) {
        if !self.offers_extension(code::CATALOG.id) {
            return;
        }
        let Some(cwd) = self.focused_cwd() else {
            self.remembered.git_badge = None;
            return;
        };
        if self.remembered.git_pending.is_some() {
            return;
        }
        let now = Instant::now();
        let current = self
            .remembered
            .git_badge
            .as_ref()
            .filter(|badge| badge.cwd == cwd);
        let every = paced(GIT_BADGE_REFRESH, self.remembered.git_took);
        let summary_fresh =
            current.is_some_and(|badge| now.duration_since(badge.checked_at) < every);
        let timeline_fresh = current
            .is_some_and(|badge| now.duration_since(badge.timeline_checked_at) < TIMELINE_REFRESH);
        if summary_fresh && timeline_fresh {
            return;
        }
        let target = self.remembered.targets.get(&evaluation_key(&cwd)).cloned();
        self.remembered.git_pending = Some(cwd.clone());
        spawn_git_read(cwd, target, !timeline_fresh, sender.clone());
    }

    /// Asks for the spec summary of the checkout in front when the one
    /// held is about another or has gone stale.
    pub(super) fn schedule_spec_summary(&mut self, sender: &mpsc::Sender<SpecSummaryResolution>) {
        if !self.offers_extension(spec::CATALOG.id) {
            return;
        }
        let Some(cwd) = self.focused_cwd() else {
            self.remembered.spec_summary = None;
            return;
        };
        if self.remembered.spec_summary_pending.is_some() {
            return;
        }
        let fresh = self.remembered.spec_summary.as_ref().is_some_and(|state| {
            state.cwd == cwd && state.checked_at.elapsed() < SPEC_SUMMARY_REFRESH
        });
        if fresh {
            return;
        }
        let target = self.remembered.targets.get(&evaluation_key(&cwd)).cloned();
        self.remembered.spec_summary_pending = Some(cwd.clone());
        spawn_spec_summary(cwd, target, sender.clone());
    }

    /// Installs a finished summary if it is still about the checkout in
    /// front; releases the pending key either way.
    pub(super) fn absorb_spec_summary(&mut self, resolution: SpecSummaryResolution) -> bool {
        if self.remembered.spec_summary_pending.as_ref() == Some(&resolution.cwd) {
            self.remembered.spec_summary_pending = None;
        }
        if self.focused_cwd().as_ref() != Some(&resolution.cwd)
            || !self.offers_extension(spec::CATALOG.id)
        {
            return false;
        }
        let changed =
            self.remembered.spec_summary.as_ref().is_none_or(|state| {
                state.cwd != resolution.cwd || state.summary != resolution.summary
            });
        self.remembered.spec_summary = Some(SpecSummaryState {
            cwd: resolution.cwd,
            summary: resolution.summary,
            checked_at: Instant::now(),
        });
        changed
    }

    /// The summary the sidebar draws: the checkout in front's, when it has
    /// a spec layout.
    pub(super) fn spec_summary(&self) -> Option<&spec::Summary> {
        self.remembered.spec_summary.as_ref()?.summary.as_ref()
    }

    /// Installs a finished read, or drops it.
    ///
    /// Returns whether anything on screen changed. An answer about a
    /// checkout the selection has since left is released — its key must
    /// not stay reserved — and then discarded: it is not wrong, it is no
    /// longer the question being asked.
    pub(super) fn absorb_git_read(&mut self, resolution: GitResolution) -> bool {
        if self.remembered.git_pending.as_ref() == Some(&resolution.cwd) {
            self.remembered.git_pending = None;
        }
        self.remembered.git_took = resolution.took;
        if self.focused_cwd().as_ref() != Some(&resolution.cwd)
            || !self.offers_extension(code::CATALOG.id)
        {
            return false;
        }
        let now = Instant::now();
        let carried = self
            .remembered
            .git_badge
            .take()
            .filter(|badge| badge.cwd == resolution.cwd);
        self.remembered.git_badge = Some(match resolution.answer {
            GitAnswer::Summary(summary) => GitBadge {
                cwd: resolution.cwd,
                summary,
                timeline: carried.as_ref().and_then(|badge| badge.timeline.clone()),
                timeline_checked_at: carried.map_or(now, |badge| badge.timeline_checked_at),
                checked_at: now,
            },
            GitAnswer::Full { summary, timeline } => GitBadge {
                cwd: resolution.cwd,
                summary,
                timeline,
                timeline_checked_at: now,
                checked_at: now,
            },
        });
        true
    }

    /// Whether a commit account is on screen *or* on its way — both are
    /// states a keystroke or a click dismisses, so an answer still in
    /// flight cannot open over a viewer who has already moved on.
    pub(super) fn commit_detail_open(&self) -> bool {
        self.commit_detail.is_some() || self.commit_detail_pending.is_some()
    }

    pub(super) fn dismiss_commit_detail(&mut self) {
        self.commit_detail = None;
        self.commit_detail_pending = None;
        self.dirty = true;
    }

    /// Opens the popup a background `git show` answered for, or drops the
    /// answer.
    ///
    /// Dropped when the viewer clicked another row, or dismissed the
    /// popup, while the read ran: the pending hash is what they last
    /// asked for, and nothing else may open over them.
    pub(super) fn absorb_commit_detail(&mut self, resolution: CommitDetailResolution) -> bool {
        if self.commit_detail_pending.as_deref() != Some(resolution.hash.as_str()) {
            return false;
        }
        self.commit_detail_pending = None;
        let Some(detail) = resolution.detail else {
            return false;
        };
        self.commit_detail = Some(CommitDetailPopup {
            detail,
            target: resolution.target,
            anchor: resolution.anchor,
            scroll: 0,
        });
        true
    }

    /// Hands the surface's file requests to threads.
    ///
    /// Everything that reads or writes a *file* stays serialised: a save
    /// followed by the re-read that re-colours it must land in that
    /// order, and two reads racing would let the older one describe the
    /// newer one's file.
    ///
    /// The second colouring pass is not in that chain. It is the slowest
    /// request there is — seconds for a long file — and the next file
    /// opened used to wait behind the colour of the one being left. It
    /// cannot describe a newer file than the one on screen, because the
    /// view installs colour only for the text it already holds.
    ///
    /// Listings are not in that chain. They answer about directories
    /// nothing else in the queue names, they cost a `readdir` each, and
    /// they arrive in bulk — opening a surface back where it was left
    /// asks for every directory that was open. One per pass made that a
    /// frame each: a third of a second of a tree filling in one row at a
    /// time, for a tenth of a millisecond of actual work. The read of the
    /// checkout's `files.exclude` is off it for the same reason: it is
    /// what the first listing waits on.
    pub(super) fn schedule_file_request(&mut self, sender: &mpsc::Sender<FileResolution>) {
        let Some(view) = self.code.as_mut() else {
            return;
        };
        let root = view.root().to_path_buf();
        loop {
            let listing = match view.peek_request() {
                // A second colouring pass is off the chain too: it is the
                // slowest request there is, and a file opened after it
                // must not wait for the colour of one being left.
                Some(
                    code::FileRequest::List(_)
                    | code::FileRequest::Exclusions(_)
                    | code::FileRequest::Colour(_),
                ) => true,
                // The chain is busy, and the queue is in the order the
                // surface asked: stopping here rather than looking past
                // it is what keeps a save ahead of the read that follows
                // it.
                Some(_) if !self.code_request_pending => false,
                _ => return,
            };
            let Some(request) = view.take_request() else {
                return;
            };
            self.code_request_pending |= !listing;
            spawn_file_request(root.clone(), request, sender.clone());
        }
    }

    pub(super) fn schedule_artifacts_read(&mut self, sender: &mpsc::Sender<ArtifactsResolution>) {
        if self.architect.is_none() || self.architect_asked {
            return;
        }
        if let Some(root) = self.architect_root.clone() {
            self.architect_asked = true;
            spawn_artifacts_read(root, sender.clone());
        }
    }

    pub(super) fn schedule_spec_read(
        &mut self,
        home: &UzeHome,
        sender: &mpsc::Sender<SpecResolution>,
    ) {
        if self.spec.is_none() || self.spec_asked {
            return;
        }
        if let Some(root) = self.spec_root.clone() {
            self.spec_asked = true;
            spawn_spec_read(home, root, sender.clone());
        }
    }

    pub(super) fn absorb_spec(&mut self, resolution: SpecResolution) -> bool {
        match self.spec.as_mut() {
            Some(view) if self.spec_root.as_ref() == Some(&resolution.root) => {
                view.absorb(resolution.answer);
                true
            }
            _ => false,
        }
    }

    pub(super) fn absorb_artifacts(&mut self, resolution: ArtifactsResolution) -> bool {
        match self.architect.as_mut() {
            Some(view) if self.architect_root.as_ref() == Some(&resolution.root) => {
                view.absorb(resolution.answer);
                true
            }
            _ => false,
        }
    }

    /// Installs one file answer, if the surface is still open on the
    /// checkout it was read for.
    pub(super) fn absorb_file_answer(&mut self, resolution: FileResolution) -> bool {
        // A listing never took the chain, so it does not release it — a
        // pass that let one clear a read's reservation would put the next
        // read alongside the read it has to follow.
        if !matches!(
            resolution.answer,
            code::FileAnswer::Listed { .. }
                | code::FileAnswer::Excluded(_)
                | code::FileAnswer::Coloured { .. }
        ) {
            self.code_request_pending = false;
        }
        let Some(view) = self
            .code
            .as_mut()
            .filter(|view| view.root() == resolution.root)
        else {
            // A surface that moved on no longer has anywhere to say a
            // write failed, and a failed write must not pass unsaid: the
            // reader believes it landed.
            return self.report_unheard_write_failure(resolution.answer);
        };
        view.absorb(resolution.answer);
        true
    }

    pub(super) fn report_unheard_write_failure(&mut self, answer: code::FileAnswer) -> bool {
        let (title, path, message) = match answer {
            code::FileAnswer::Saved {
                path,
                outcome: Err(message),
            } => ("save failed", path, message),
            code::FileAnswer::Deleted {
                path,
                outcome: Err(message),
            } => ("delete failed", path, message),
            code::FileAnswer::Renamed {
                from,
                outcome: Err(message),
                ..
            } => ("rename failed", from, message),
            _ => return false,
        };
        self.raise_toast(
            ToastKind::Failed,
            title,
            format!("{}: {message}", path.display()),
            None,
        );
        true
    }

    /// Asks for the selection's diff alone, the moment it moved.
    pub(super) fn schedule_diff_read(&mut self, sender: &mpsc::Sender<DiffResolution>) {
        if self.code_diff_pending {
            return;
        }
        let Some(view) = self.code.as_ref() else {
            return;
        };
        let Some(request) = view.diff_request() else {
            return;
        };
        self.code_diff_pending = true;
        spawn_diff_read(view.root().to_path_buf(), request, sender.clone());
    }

    pub(super) fn absorb_diff(&mut self, resolution: DiffResolution) -> bool {
        self.code_diff_pending = false;
        if self
            .code
            .as_ref()
            .is_none_or(|view| view.root() != resolution.root)
        {
            return false;
        }
        let marked = self.code_marked_text();
        if let Some(view) = self.code.as_mut() {
            view.absorb_diff(resolution.answer);
        }
        self.forget_marking_if_moved(marked);
        true
    }

    /// Asks for the changes half again, on its own cadence.
    pub(super) fn schedule_changes_refresh(&mut self, sender: &mpsc::Sender<ChangesResolution>) {
        if self.code_changes_pending {
            return;
        }
        let Some(view) = self.code.as_ref() else {
            return;
        };
        // The periodic re-read is what the cadence governs. A moved
        // selection is the one-file read's, unless there is no list yet
        // to find it in — the first read is a whole one.
        let first = view.diff_pending() && view.diff_request().is_none();
        if !first && !view.refresh_due() {
            return;
        }
        self.code_changes_pending = true;
        spawn_changes_refresh(view.root().to_path_buf(), view.placement(), sender.clone());
    }

    /// Gives a surface that has just opened the measurement this client
    /// already holds for its checkout, so the map is there to be asked
    /// for rather than arriving a second later.
    pub(super) fn show_remembered_measure(&mut self) {
        let Some(view) = self.code.as_mut() else {
            return;
        };
        if let Some((_, measure)) = self.code_measures.get(view.root()) {
            view.absorb_measure(measure.clone());
        }
    }

    /// Asks for the checkout's measurement, once per surface that has no
    /// map yet. A `git grep` over every file is not a read to repeat, so
    /// the root it was asked about is remembered rather than the request
    /// being in flight: outside a repository the answer is nothing, and
    /// nothing must not be asked for again.
    pub(super) fn schedule_code_measure(&mut self, sender: &mpsc::Sender<MeasureResolution>) {
        let Some(view) = self.code.as_ref().filter(|view| view.wants_measure()) else {
            return;
        };
        let root = view.root().to_path_buf();
        if self.code_measure_asked.as_deref() == Some(root.as_path()) {
            return;
        }
        // A measurement this recent describes the same checkout: a map is
        // where the lines are, and that is not a picture that changes
        // between one look at it and the next.
        if self
            .code_measures
            .get(&root)
            .is_some_and(|(taken, _)| taken.elapsed() < CODE_MEASURE_FRESH)
        {
            return;
        }
        self.code_measure_asked = Some(root.clone());
        spawn_code_measure(root, sender.clone());
    }

    /// Installs the measurement, if the surface is still open on the
    /// checkout it was measured from.
    pub(super) fn absorb_measure(&mut self, resolution: MeasureResolution) -> bool {
        let Some(measure) = resolution.measure else {
            let Some(view) = self
                .code
                .as_mut()
                .filter(|view| view.root() == resolution.root)
            else {
                return false;
            };
            view.absorb_unmeasurable();
            return true;
        };
        // Kept whether or not there is still a surface to show it: the
        // next one to open on this checkout is the reason it was worth
        // measuring.
        self.code_measures
            .insert(resolution.root.clone(), (Instant::now(), measure.clone()));
        let Some(view) = self
            .code
            .as_mut()
            .filter(|view| view.root() == resolution.root)
        else {
            return false;
        };
        view.absorb_measure(measure);
        true
    }

    /// Installs a refreshed changes half, if the surface is still open on
    /// the checkout it was read for. The view itself decides whether the
    /// answer still describes where the viewer is.
    pub(super) fn absorb_changes(&mut self, resolution: ChangesResolution) -> bool {
        self.code_changes_pending = false;
        if self
            .code
            .as_ref()
            .is_none_or(|view| view.root() != resolution.root)
        {
            return false;
        }
        let marked = self.code_marked_text();
        if let Some(view) = self.code.as_mut() {
            view.absorb_changes(resolution.refreshed);
        }
        self.forget_marking_if_moved(marked);
        true
    }

    /// The text a marking on the code surface covers right now.
    pub(super) fn code_marked_text(&self) -> Option<Vec<String>> {
        let Some(Selection::Text(marking)) = &self.selection else {
            return None;
        };
        Some(code::text(self.code.as_ref()?, marking.marked()?.lines()))
    }

    /// Drops a marking whose text a refresh changed. A marking is a
    /// position in the text, and the same lines of a different diff are
    /// not what was marked; a refresh that found everything as it was —
    /// most of them — leaves it drawn.
    pub(super) fn forget_marking_if_moved(&mut self, before: Option<Vec<String>>) {
        if before.is_some_and(|before| self.code_marked_text() != Some(before)) {
            self.selection = None;
        }
    }
}
