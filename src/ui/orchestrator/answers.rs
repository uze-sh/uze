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
        self.remembered.evaluation_generation += 1;
        spawn_task_evaluation(
            home,
            key,
            self.remembered.evaluation_generation,
            cwd,
            occupied,
            sender.clone(),
        );
    }

    /// The working directory the badge and the timeline are about: the
    /// agent's in front, never where a shell beside it moved to, and none
    /// with a shell of the space in front.
    pub(super) fn focused_cwd(&self) -> Option<PathBuf> {
        self.agent_directory_in_front().map(Path::to_path_buf)
    }

    /// Asks for the badge again once it is due, on a thread of its own.
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
        let every = paced(GIT_BADGE_REFRESH, self.remembered.git_took);
        let fresh = self
            .remembered
            .git_badge
            .as_ref()
            .is_some_and(|badge| badge.cwd == cwd && badge.checked_at.elapsed() < every);
        if fresh {
            return;
        }
        self.remembered.git_pending = Some(cwd.clone());
        spawn_git_read(cwd, sender.clone());
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
        self.remembered.git_badge = Some(GitBadge {
            cwd: resolution.cwd,
            summary: resolution.summary,
            checked_at: Instant::now(),
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
}

impl WorkspaceModel {
    /// Asks for the declared extensions when a discovery is due (once a
    /// run, whenever `plugins` — the plugin set's revision — moves, and
    /// on [`DECLARED_REDISCOVER`] otherwise), polls the linked ones' stamps
    /// on [`DECLARED_POLL`], and compiles again each one whose save has
    /// settled. Cheap enough for every tick: it compares instants.
    pub(super) fn schedule_extension_jobs(
        &mut self,
        home: &UzeHome,
        plugins: u64,
        jobs: &mpsc::Sender<ExtensionAnswer>,
        stamps: &mpsc::Sender<StampsAnswer>,
    ) {
        let now = Instant::now();
        let declared = &mut self.remembered.declared;
        if declared.discovery_due(plugins, now) {
            declared.discovered_at = Some(now);
            declared.plugins_seen = plugins;
            declared.discovery_pending = true;
            spawn_extension_job(home, ExtensionJob::Discover(declared.known()), jobs.clone());
            return;
        }
        if declared.discovery_pending {
            return;
        }
        let settled = declared.settled(now);
        if !settled.is_empty() {
            declared
                .recompiling
                .extend(settled.iter().map(|installed| installed.identity.clone()));
            spawn_extension_job(home, ExtensionJob::Recompile(settled), jobs.clone());
        }
        let due = declared
            .polled_at
            .is_none_or(|at| now.duration_since(at) >= DECLARED_POLL);
        if !declared.stamps_pending && due {
            let watched = declared.watched();
            if !watched.is_empty() {
                declared.stamps_pending = true;
                declared.polled_at = Some(now);
                spawn_extension_stamps(watched, stamps.clone());
            }
        }
    }

    /// Hands each hosted engine's pending jobs to a thread, and answers how
    /// many it handed. Cheap enough for every tick: an engine with nothing
    /// to read answers none.
    pub(super) fn schedule_engine_jobs(
        &mut self,
        home: &UzeHome,
        answers: &mpsc::Sender<EngineAnswer>,
    ) -> usize {
        self.schedule_engine_jobs_at(home, answers, Instant::now())
    }

    /// [`Self::schedule_engine_jobs`] on the pass at `now`, which each
    /// engine is told before it is asked for its jobs: what its `every`
    /// and its events' window are counted on.
    pub(super) fn schedule_engine_jobs_at(
        &mut self,
        home: &UzeHome,
        answers: &mpsc::Sender<EngineAnswer>,
        now: Instant,
    ) -> usize {
        let in_front = self
            .focused_cwd()
            .map(|root| uze_extensions::contract::Checkout {
                display: crate::ui::display_project_path(&root),
                target: self.remembered.targets.get(&evaluation_key(&root)).cloned(),
                root,
            });
        let offered: BTreeSet<Owner> = self
            .remembered
            .declared
            .engines
            .keys()
            .filter(|owner| self.offers_declared(owner))
            .cloned()
            .collect();
        let mut handed = 0;
        for (owner, engine) in &mut self.remembered.declared.engines {
            engine.focus(in_front.as_ref().filter(|_| offered.contains(owner)));
        }
        self.pass_engines(now);
        let declared = &mut self.remembered.declared;
        for (owner, engine) in &mut declared.engines {
            let jobs = engine.jobs();
            handed += jobs.len();
            if !jobs.is_empty() {
                spawn_engine_jobs(
                    owner.clone(),
                    jobs,
                    engine.lanes(),
                    home,
                    &declared.runner,
                    answers.clone(),
                );
            }
        }
        handed
    }

    /// Takes what a hosted engine's job answered to the engine that asked.
    /// One whose extension went meanwhile is dropped. Returns whether it
    /// is drawn.
    ///
    /// A marking on the open view whose text the answer changed is
    /// dropped: a marking is a position in the text, and the same lines of
    /// a different diff are not what was marked. One that found everything
    /// as it was, most of them, leaves it drawn.
    pub(super) fn absorb_engine_answer(&mut self, answer: EngineAnswer) -> bool {
        if !self
            .remembered
            .declared
            .engines
            .contains_key(&answer.extension)
        {
            return false;
        }
        let marked = self.marked_text();
        let Some(engine) = self.remembered.declared.engines.get_mut(&answer.extension) else {
            return false;
        };
        let outcome = engine.absorb(answer.answer);
        if marked.is_some_and(|before| self.marked_text() != Some(before)) {
            self.selection = None;
        }
        // An answer arrives after its gesture, wherever the viewer is now:
        // only what is said reaches them. A copy, a path or a surface to
        // open is a gesture's, never an answer's.
        use uze_extensions::contract::Outcome;
        // What the gesture said while it waited ("Say done…") is over once
        // its extension answers: the answer says what there is to say.
        if self
            .slot_in_pane()
            .is_some_and(|slot| slot.owner == answer.extension)
            && !matches!(outcome, Outcome::Notice(_))
        {
            self.open_notice = None;
        }
        match outcome {
            Outcome::Failed { title, detail } => self.say_failed(&title, detail),
            Outcome::Toast(text) => self.say_told(&answer.extension, text),
            Outcome::Notice(span)
                if self
                    .slot_in_pane()
                    .is_some_and(|slot| slot.owner == answer.extension) =>
            {
                self.open_notice = Some(span);
            }
            _ => {}
        }
        true
    }

    /// The text a marking on the open view covers right now.
    pub(super) fn marked_text(&self) -> Option<Vec<String>> {
        let Some(Selection::Text(marking)) = &self.selection else {
            return None;
        };
        let lines = marking.marked()?.lines();
        let slot = self.slot_in_pane()?;
        let engine = self.remembered.declared.engine(&slot.owner)?;
        Some(engine.text(&slot.surface.id, lines))
    }

    /// Takes a declared-extension job's answer. Returns whether anything on
    /// screen may have changed.
    pub(super) fn absorb_extension_answer(&mut self, answer: ExtensionAnswer) -> bool {
        let declared = &mut self.remembered.declared;
        for identity in &answer.recompiled {
            declared.recompiling.remove(identity);
        }
        if answer.discovery {
            declared.discovery_pending = false;
        }
        let mut went = false;
        if let Some(found) = &answer.discovered {
            went = declared.absorb_discovery(found);
        }
        // A recompile answers only for an extension still installed: one a
        // discovery took away meanwhile stays away.
        let recompiled = answer.discovered.is_none();
        let loaded: Vec<LoadedExtension> = answer
            .loaded
            .into_iter()
            .filter(|loaded| {
                !recompiled || declared.extensions.contains_key(&loaded.installed.identity)
            })
            .collect();
        let mut changed = went || !loaded.is_empty();
        for loaded in loaded {
            declared.absorb_loaded(loaded);
        }
        changed |= declared.absorb_records(answer.records);
        changed |= declared.absorb_preferences(answer.preferences);
        // A view whose extension went, or whose surface it stopped
        // declaring, is put away rather than drawn from nothing.
        if self.open_slot.is_some() && self.slot_in_pane().is_none() {
            self.open_slot = None;
            self.open_root = None;
            self.inspecting = false;
        }
        self.register_extension_keys();
        changed
    }

    /// Takes a poll of the linked extensions' stamps. Nothing on screen
    /// changes until a save settles and is compiled.
    pub(super) fn absorb_extension_stamps(&mut self, answer: StampsAnswer) {
        let declared = &mut self.remembered.declared;
        declared.stamps_pending = false;
        declared.absorb_stamps(answer.stamps, Instant::now());
    }
}
