//! Whether each agent is working: its repaints, the echo of what was typed into it, the chime when it finishes, and what its tab shows.

use super::*;

impl WorkspaceModel {
    /// Marks the pane as busy and, when the submission was reconstructed
    /// with confidence, records it. Activity is noted for every Enter in an
    /// agent pane — that signal predates the history and does not depend on
    /// knowing what was typed.
    pub(super) fn note_agent_prompt_submission(
        &mut self,
        pane: PaneId,
        identities: &[AgentIdentity],
        prompt: Option<&str>,
    ) {
        let origin = self.session.as_ref().and_then(|session| {
            session.workspace.spaces.iter().find_map(|space| {
                space.tabs.iter().find_map(|tab| {
                    if tab.pane.id != pane {
                        return None;
                    }
                    agent_identity_for_tab(identities, tab).map(|binary| {
                        uze_application::PromptOrigin {
                            space_label: space.label.clone(),
                            tab_id: tab.id.0,
                            tab_label: tab.label.clone(),
                            agent_binary: binary.to_owned(),
                            agent: launched_agent_id(tab).map(str::to_owned),
                        }
                    })
                })
            })
        });
        let Some(origin) = origin else {
            return;
        };
        self.remembered
            .agent_activity
            .entry(pane)
            .or_default()
            .working_until = Some(Instant::now() + AGENT_QUIET_AFTER);
        self.remembered.completed_agent_panes.remove(&pane);
        self.dirty = true;

        if let (Some(prompt), Some(recorder)) = (prompt, self.prompt_recorder.as_ref())
            && let Some(root) = self.space_root_of_pane(pane)
        {
            let _ = recorder.send((root, origin, prompt.to_owned()));
        }
    }

    /// Notes that `pane` painted something. Repainting both *starts* and
    /// extends an agent's busy state — the asymmetry where only Enter could
    /// start it left every self-driven turn, and every turn that resumed
    /// after a quiet stretch, showing as idle — but only once there is
    /// enough of it to be animation rather than a blink (see
    /// [`AGENT_BUSY_REPAINTS`]). Damage that is really the echo of the
    /// user's own typing or pasting is ignored, as is damage in a shell
    /// pane.
    pub(super) fn note_agent_output(
        &mut self,
        pane: PaneId,
        identities: &[AgentIdentity],
        now: Instant,
    ) {
        if !self.is_agent_pane(pane, identities) {
            return;
        }
        if self.is_echoing_input(pane, now) {
            // Still settling from what we asked for: hold the window open
            // around this frame rather than letting the clock run out
            // underneath the redraw (see [`AGENT_SETTLE_QUIET`]).
            self.extend_echo_window(pane, now);
            return;
        }
        let activity = self.remembered.agent_activity.entry(pane).or_default();
        if activity.note_repaint(now) {
            activity.working_until = Some(now + AGENT_QUIET_AFTER);
            self.remembered.completed_agent_panes.remove(&pane);
        }
    }

    pub(super) fn agent_is_working(&self, pane: PaneId) -> bool {
        self.remembered
            .agent_activity
            .get(&pane)
            .is_some_and(AgentActivity::is_working)
    }

    pub(super) fn is_echoing_input(&self, pane: PaneId, now: Instant) -> bool {
        self.input_echo_until
            .get(&pane)
            .is_some_and(|window| now < window.until)
    }

    /// Holds an open window around a frame that arrived inside it: the
    /// redraw is not over while it is still painting. Never past the cap
    /// the window was opened with, so a pane that simply keeps painting
    /// stops being excused.
    pub(super) fn extend_echo_window(&mut self, pane: PaneId, now: Instant) {
        if let Some(window) = self.input_echo_until.get_mut(&pane) {
            window.until = (now + AGENT_SETTLE_QUIET).min(window.cap).max(window.until);
        }
    }

    /// Opens the window in which `pane`'s own repaints read as the echo of
    /// a keystroke we just forwarded there.
    pub(super) fn note_pane_input(&mut self, pane: PaneId) {
        self.open_echo_window(pane, Instant::now(), AGENT_ECHO_GRACE);
    }

    /// Same, for a paste: the harness re-lays out its prompt box around the
    /// pasted content — an image especially — long after the bytes landed.
    pub(super) fn note_pane_paste(&mut self, pane: PaneId) {
        self.open_echo_window(pane, Instant::now(), AGENT_PASTE_GRACE);
    }

    /// Same, for a redraw this client provoked by resizing the pane.
    pub(super) fn note_pane_redraw(&mut self, pane: PaneId) {
        self.open_echo_window(pane, Instant::now(), AGENT_REDRAW_GRACE);
    }

    /// Same, for the repaint an attach provokes across every open pane at
    /// once rather than in the one pane being resized.
    pub(super) fn note_attach_redraw(&mut self) {
        let now = Instant::now();
        let panes: Vec<PaneId> = self.panes.keys().copied().collect();
        for pane in panes {
            self.open_echo_window(pane, now, AGENT_REDRAW_GRACE);
        }
    }

    pub(super) fn open_echo_window(&mut self, pane: PaneId, now: Instant, grace: Duration) {
        self.input_echo_until.insert(
            pane,
            EchoWindow {
                until: now + grace,
                cap: now + grace.max(AGENT_SETTLE_CAP),
            },
        );
    }

    pub(super) fn is_agent_pane(&self, pane: PaneId, identities: &[AgentIdentity]) -> bool {
        self.tab_of_pane(pane)
            .is_some_and(|tab| agent_identity_for_tab(identities, tab).is_some())
    }

    /// Advances every agent pane's phase for the current instant: a pane
    /// quiet for [`AGENT_QUIET_AFTER`] stops working, and one that stopped
    /// out of sight keeps a check until its tab is actually on screen.
    /// Clearing that check here — rather than only at the handful of call
    /// sites that switch tabs — is what makes "done" disappear exactly when
    /// the user looks at it, whichever way they got there.
    pub(super) fn expire_agent_activity(&mut self, now: Instant) -> bool {
        let focused = self.focused_pane();
        let mut expired = Vec::new();
        for (pane, activity) in &mut self.remembered.agent_activity {
            if activity.expire(now) {
                expired.push(*pane);
            }
        }
        for pane in &expired {
            self.unsettled_turns.insert(*pane, now);
            if *pane != focused {
                self.remembered.completed_agent_panes.insert(*pane);
            }
        }
        self.recently_quiet.extend(expired.iter().copied());
        // A closed echo window, and a pane holding neither a deadline nor
        // recent repaints, have nothing left to say about themselves —
        // dropping them keeps this tick's early exit reachable.
        self.input_echo_until.retain(|_, window| now < window.until);
        self.remembered
            .agent_activity
            .retain(|_, activity| activity.is_working() || !activity.repaints.is_empty());
        let acknowledged = self.remembered.completed_agent_panes.remove(&focused);
        // A pane whose tab is gone can never be looked at again, and its id
        // is free to be handed to a future pane — leaving its check behind
        // would eventually surface on something unrelated.
        self.forget_closed_agent_panes();
        !expired.is_empty() || acknowledged
    }

    /// Whether the bell rings now, under `chime`, for the turns that have
    /// stayed ended for [`CHIME_SETTLE`].
    ///
    /// A settled turn is judged by what the operator sees *then*, not when
    /// it ended: with [`Chime::OutOfSight`](uze_application::Chime) it rings
    /// only while its tab still carries the check, so a tab looked at in
    /// the meantime stays quiet. A turn is consumed once settled whether or
    /// not it rang — one that did not ring when it settled must not ring
    /// later, about something the operator has moved on from.
    pub(super) fn take_ring(&mut self, chime: uze_application::Chime, now: Instant) -> bool {
        use uze_application::Chime;
        if self.unsettled_turns.is_empty() {
            return false;
        }
        let activity = &self.remembered.agent_activity;
        let completed = &self.remembered.completed_agent_panes;
        let mut due = false;
        self.unsettled_turns.retain(|pane, ended| {
            if activity.get(pane).is_some_and(AgentActivity::is_working) {
                return false;
            }
            if now.duration_since(*ended) < CHIME_SETTLE {
                return true;
            }
            due |= match chime {
                Chime::Silent => false,
                Chime::OutOfSight => completed.contains(pane),
                Chime::Always => true,
            };
            false
        });
        let rested = self
            .chimed_at
            .is_none_or(|last| now.duration_since(last) >= CHIME_COOLDOWN);
        let ring = due && rested;
        if ring {
            self.chimed_at = Some(now);
        }
        ring
    }

    pub(super) fn forget_closed_agent_panes(&mut self) {
        // Runs on every input tick, so it earns the early exit: with no
        // per-pane state held there is nothing to reconcile, and walking
        // the tab tree to build a live-pane set would be pure overhead.
        if self.remembered.agent_activity.is_empty()
            && self.remembered.completed_agent_panes.is_empty()
            && self.input_echo_until.is_empty()
            && self.unsettled_turns.is_empty()
        {
            return;
        }
        if self.session.is_none() {
            return;
        }
        let live: BTreeSet<PaneId> = self.tabs().map(|tab| tab.pane.id).collect();
        self.remembered
            .agent_activity
            .retain(|pane, _| live.contains(pane));
        self.remembered
            .completed_agent_panes
            .retain(|pane| live.contains(pane));
        self.input_echo_until.retain(|pane, _| live.contains(pane));
        // A closed tab's turn has nobody left to tell, and its id is free
        // for a new pane to inherit the ring.
        self.unsettled_turns.retain(|pane, _| live.contains(pane));
    }

    /// The one place the four sidebar states are decided. Working outranks
    /// Completed (fresh output means the run the check would announce is
    /// not over), and both outrank Selected — a spinner or a check on the
    /// tab you are already on still carries information the plain dot does
    /// not.
    pub(super) fn agent_tab_status(&self, pane: PaneId, selected: bool) -> AgentTabStatus {
        if self.agent_is_working(pane) {
            AgentTabStatus::Working
        } else if self.remembered.completed_agent_panes.contains(&pane) {
            AgentTabStatus::Completed
        } else if selected {
            AgentTabStatus::Selected
        } else {
            AgentTabStatus::Idle
        }
    }

    pub(super) fn acknowledge_completed_agent_tab(&mut self, tab: TabId) {
        if let Some(pane) = self.pane_for_tab(tab)
            && self.remembered.completed_agent_panes.remove(&pane)
        {
            self.dirty = true;
        }
    }
}
