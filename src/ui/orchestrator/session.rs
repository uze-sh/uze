//! One attach, and what it does with an event.
//!
//! Split out of `orchestrator.rs`'s `attach_workspace`, which had grown to
//! ~1.5k lines carrying five jobs at once: the server handshake, the frame
//! loop, the cadence of every background read, slot lifecycle, and a
//! 46-arm `match` over every key and click the workspace understands.
//!
//! Two of those live here — [`Attach::pump`], which absorbs what the
//! server and the background reads have said and asks for whatever has
//! gone stale, and [`Attach::handle`], which answers one event. [`Attach`]
//! itself is the state both needed: the model, the connection it drives
//! the server through, and the channels its reads answer on, so a handler
//! reaches for a field instead of closing over a local and the loop that
//! calls them fits on a screen.
//!
//! # Why one match became several
//!
//! The arms were never mixed: a `Event::Key(_) if …` guard can only ever
//! match a key, and every mouse arm tested exactly one `MouseEventKind`.
//! Splitting by event kind is therefore exact rather than a judgement
//! call, and it leaves the thing the guards *do* encode legible — modal
//! precedence. In [`Attach::key`] and [`Attach::press`] the order of the
//! guards is the order overlays stack in, and each overlay's own keys
//! live in a method named after it, so those two lists are the precedence
//! and nothing else. `WorkspaceModel::no_modal_open` names the same set
//! from one place.

use super::*;

use crate::ui::model::Route;
use crate::ui::widget::ToastKind;
use uze_extensions::view::Command;
use uze_keys::{Action, Resolution, Scope};

mod absorb;
mod agents;
mod keys;
mod manage;
mod mouse;
mod overlays;
mod pane_input;
mod surface;
mod work_modal;

/// Where an event leaves the loop.
pub(super) enum Flow {
    /// Keep going — almost everything.
    Continue,
    /// Ctrl+Q out, or the terminal runtime gone.
    Exit(WorkspaceExit),
}

/// The frame an event is handled against.
///
/// Recomputed once per iteration, before any event is read, so a resize
/// that arrived in the same tick is already accounted for — several arms
/// size a PTY from it, and sizing one from a stale layout is how a pane
/// ends up drawn at one size and running at another.
pub(super) struct Viewport {
    pub(super) size: ratatui::layout::Size,
    pub(super) layout: WorkspaceLayout,
    pub(super) columns: u16,
    pub(super) rows: u16,
}

/// Everything one attach holds while its loop runs.
pub(super) struct Attach<'a> {
    pub(super) model: WorkspaceModel,
    pub(super) stream: uze_terminal::Stream,
    pub(super) home: &'a UzeHome,
    /// The registered harness set, resolved once per attach — it cannot
    /// change mid-session.
    pub(super) identities: Vec<AgentIdentity>,
    pub(super) channels: &'a Channels,
    /// Drives the agent-activity animation. Ratatui owns the alternate
    /// screen, so this one is hidden and only its position is read — see
    /// [`AGENT_ACTIVITY_TICK`].
    pub(super) spinner: ProgressBar,
    pub(super) next_tick: Instant,
    /// Whether the action being performed right now asked the server to
    /// select a tab.
    ///
    /// Everything else a first step can do lands in the model before the
    /// handler returns, so the evidence is there to read. Moving between
    /// agents does not: the client asks and the server answers frames
    /// later, so the selection is unchanged at the moment the question
    /// "did that land?" is asked, and the step was never ticked off
    /// however many times it was taken.
    pub(super) asked_for_a_tab: bool,
    /// What the management modal keeps between openings — session-lived,
    /// like the workspace's own memory, so an answer still in flight when
    /// the modal closes lands when it opens again.
    pub(super) manage_memory: &'a mut crate::ui::management::ManagementMemory,
    /// What the host terminal's keyboard can deliver, asked once at
    /// startup and handed to the modal's Keys screen on every opening.
    pub(super) keyboard: crate::ui::keys::KeyboardSupport,
    /// The theme this client last drew in and told the server about, as
    /// [`uze_theme::generation`] counts them.
    pub(super) theme_generation: u64,
}

/// What a code door does when it is pressed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CodeDoor {
    /// It is the door already open, so it shuts. The action is called a
    /// toggle and the hint line promises one; re-showing what is already
    /// showing looks exactly like a key that does nothing.
    Close,
    /// The other door, so the surface switches to it — the gesture of
    /// someone reaching for the other half of the same surface.
    Switch,
    /// Nothing is open, so this door is not the one that opens it: that
    /// is the workspace's own binding, one scope out.
    Nothing,
}

pub(super) fn code_door(showing: Option<code::ContentMode>, wanted: code::ContentMode) -> CodeDoor {
    // A document read as its preview is still the files half: the door to
    // the files is the door already open, and it shuts rather than
    // turning the preview back into source first.
    let half = |mode| match mode {
        code::ContentMode::Preview => code::ContentMode::Contents,
        other => other,
    };
    match showing {
        Some(mode) if half(mode) == half(wanted) => CodeDoor::Close,
        Some(_) => CodeDoor::Switch,
        None => CodeDoor::Nothing,
    }
}

impl Attach<'_> {
    /// Draws in a theme put in force since the last frame — chosen in
    /// Settings, or followed from the desktop — and tells the server, so a
    /// pane's program asking for its colours hears the new ones.
    fn follow_theme(&mut self) {
        let generation = uze_theme::generation();
        if generation == self.theme_generation {
            return;
        }
        self.theme_generation = generation;
        let _ = send_request(
            &mut self.stream,
            &uze_terminal::ClientRequest::SetPalette(super::active_palette()),
        );
        self.model.dirty = true;
    }

    /// Routes one event to the half of the client that owns it.
    pub(super) fn handle(&mut self, event: Event, viewport: &Viewport) -> Flow {
        let _span = tracing::debug_span!(
            "tui.event",
            kind = match &event {
                Event::Key(_) => "key",
                Event::Mouse(_) => "mouse",
                _ => "other",
            }
        )
        .entered();
        if let Event::Mouse(mouse) = event
            && let Some(flow) = self.toast_gesture(mouse, viewport)
        {
            return flow;
        }
        // The modal seals the client: while it is open every key and every
        // click is its own, resolved against its own scopes and its own
        // hit list, and the workspace behind it answers nothing. One
        // guard here rather than one at the head of each handler, so
        // nothing below can be reached around it.
        if self.model.manage.is_some() {
            return match event {
                Event::Key(key) => self.manage_key(key),
                Event::Mouse(mouse) => self.manage_mouse(mouse, viewport),
                _ => Flow::Continue,
            };
        }
        match event {
            Event::Key(key) => self.key(key, viewport),
            Event::Paste(text) => self.paste(text),
            Event::Mouse(mouse) => self.mouse(mouse, viewport),
            _ => Flow::Continue,
        }
    }
}

impl Attach<'_> {
    /// One turn of everything that is not an event: absorb what the
    /// server and the background reads have said, then ask for whatever
    /// has gone stale.
    ///
    /// Nothing here blocks. Every read this schedules runs on a thread of
    /// its own and answers through [`Channels`], which is what lets
    /// this be called every tick without the frame waiting on any of it.
    ///
    /// Answers with a [`Flow`] for the one thing absorbing can discover
    /// that no keystroke can: the terminal runtime having gone away
    /// underneath the client.
    pub(super) fn pump(&mut self, events: &mpsc::Receiver<ClientEvent>) -> Flow {
        if let Some((revision, notice)) = crate::self_update::since(self.model.release_revision) {
            self.model.release = notice;
            self.model.release_revision = revision;
            self.model.dirty = true;
        }
        loop {
            match events.try_recv() {
                Ok(event) => self.model.apply(event, &self.identities),
                Err(mpsc::TryRecvError::Empty) => break,
                // The reader thread drops its sender only when the socket
                // stopped answering: the server exited, was replaced, or
                // the protocol desynced. Nothing will ever arrive again,
                // and every request this client writes is already failing
                // in silence — so the panes on screen are frozen images
                // of a session that is gone, in a client that still looks
                // perfectly responsive. Treating it as `Empty`, which is
                // what a `while let Ok(..)` does, is how that became a
                // hang with no message and no way out but quitting.
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.model.raise_toast(
                        ToastKind::Failed,
                        "terminal runtime disconnected",
                        "the client is leaving rather than waiting on a dead socket",
                        None,
                    );
                    self.model.dirty = true;
                    return Flow::Exit(WorkspaceExit::Disconnected);
                }
            }
        }
        // The modal has a clock of its own — a spinner, a status that
        // expires, answers to absorb — turned here, before the frame, for
        // as long as it is open. It redraws when that clock moved
        // something; the workspace behind it marks its own changes.
        if let Some(manage) = self.model.manage.as_mut()
            && self.manage_memory.tick(manage, self.home)
        {
            self.model.dirty = true;
        }
        self.follow_theme();
        for request in adopt_task_names(&mut self.model) {
            let _ = send_request(&mut self.stream, &request);
        }
        for request in adopt_agent_labels(&mut self.model, &self.identities) {
            let _ = send_request(&mut self.stream, &request);
        }
        // Absorb before scheduling, throughout: an answer sitting in the
        // channel still holds its reservation, so draining first is what
        // lets the very same tick ask the next question.
        while let Ok(resolution) = self.channels.occupancy.receiver.try_recv() {
            self.absorb_occupancy(resolution);
        }
        sync_slot_occupancy(
            &mut self.model,
            self.home,
            &self.channels.occupancy.sender,
            &self.channels.tasks.sender,
        );
        while let Ok(resolution) = self.channels.placements.receiver.try_recv() {
            self.absorb_placement(resolution);
        }
        while let Ok(resolution) = self.channels.support.receiver.try_recv() {
            if self.model.remembered.agent_support_pending.as_ref() == Some(&resolution.key) {
                self.model.remembered.agent_support_pending = None;
            }
            if let Some(drawer) = self.model.support_dropdown.as_mut()
                && drawer.key == resolution.key
                && resolution.support.is_some()
            {
                drawer.support = resolution.support.clone();
            }
            self.model.remembered.agent_support = Some(resolution);
            self.model.dirty = true;
        }
        while let Ok(resolution) = self.channels.prompts.receiver.try_recv() {
            self.model.remembered.drawer_prompts = Some(resolution);
            self.keep_drawer_selection();
            self.model.dirty = true;
        }
        // A space this client asked for may have arrived; the tab that was
        // waiting for it goes in now rather than into whichever space is
        // selected.
        self.land_pending_agent_tab();
        while let Ok(resolution) = self.channels.preserved.receiver.try_recv() {
            self.model.remembered.preserved_pending = false;
            self.model.remembered.preserved_work = resolution.work;
            // The project in front may be one only this answer names.
            self.read_front_project();
            self.keep_work_selection();
            self.model.dirty = true;
        }
        self.absorb_checkouts();
        self.absorb_checkout_changes();
        self.absorb_task_evaluations();
        self.absorb_deliveries();
        self.absorb_task_mutations();
        self.schedule_task_evaluations();
        // Outcomes leave on their own clock, and the clock is drawn, so
        // this pass has to run while any of them is counting — not only
        // when one expires.
        if self.model.retire_toasts() || self.model.toasts_are_counting() {
            self.model.dirty = true;
        }
        // Contextual resolution: whatever the selection currently is, that
        // is what must be resolved. Keyed on `(harness, cwd)`, so this
        // fires exactly when the answer could have changed — a different
        // agent tab selected, or the server's live probe reporting the
        // pane moved — and never repeats for an answer already held.
        if let Some(key) = selected_agent_context(&self.model, &self.identities)
            && self.model.remembered.agent_support_pending.as_ref() != Some(&key)
            && self
                .model
                .remembered
                .agent_support
                .as_ref()
                .is_none_or(|resolution| resolution.key != key)
        {
            self.model.remembered.agent_support_pending = Some(key.clone());
            spawn_support_refresh(self.home, key, self.channels.support.sender.clone());
        }
        self.absorb_surface_answers();
        self.absorb_policy_regions();
        self.absorb_unspelled_gates();
        self.absorb_launchers();
        self.schedule_surface_reads();
        if self.model.expire_agent_activity(Instant::now()) {
            self.model.dirty = true;
        }
        if self.model.expire_press(Instant::now()) {
            self.model.dirty = true;
        }
        self.turn_activity_clock();
        Flow::Continue
    }
}
