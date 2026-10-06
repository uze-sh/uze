//! What the server says, folded into the model: the session, a pane's damage, a selection's text.

use super::*;

impl WorkspaceModel {
    pub(super) fn apply(&mut self, event: ClientEvent, identities: &[AgentIdentity]) {
        if !matches!(event, ClientEvent::Error { .. }) {
            self.error = None;
        }
        if let ClientEvent::Damage(damage) = event {
            self.dirty |= self.absorb_damage(damage, identities, Instant::now());
            return;
        }
        self.dirty = true;
        match event {
            ClientEvent::Snapshot { session } => {
                self.session = Some(session);
                self.panes.clear();
                self.note_strip_selection(identities);
                self.close_extension_left_behind();
                self.occupancy_stale = true;
            }
            ClientEvent::SessionUpdated { session } => {
                self.session = Some(session);
                self.note_typing_target();
                self.note_launcher_bypass();
                self.note_strip_selection(identities);
                self.close_extension_left_behind();
                self.prune_dragging_tab();
                self.prune_dragging_space();
                self.occupancy_stale = true;
            }
            ClientEvent::WorkspaceSetAside { kept_at, reason } => {
                // A toast, beside the one an adopted task store already
                // raises. The runtime is a different process from this
                // one, so without an event of its own the only record was
                // a log nobody had turned on — and an operator watched
                // every space disappear with nothing said anywhere.
                self.raise_toast(
                    ToastKind::Warned,
                    format!("kept at {}", kept_at.display()),
                    "the workspace could not be opened".to_owned(),
                    None,
                );
                tracing::warn!(%reason, kept_at = %kept_at.display(), "the workspace was set aside");
            }
            ClientEvent::Damage(_) => unreachable!("absorbed above"),
            ClientEvent::SelectionText { pane, text } => self.copy(pane, text),
            ClientEvent::Error { message } => self.error = Some(message),
            ClientEvent::Detached | ClientEvent::Stopped => {}
        }
    }
    /// Every pane the session holds, across its spaces.
    pub(super) fn pane_ids(&self) -> std::collections::BTreeSet<PaneId> {
        self.session
            .iter()
            .flat_map(|session| session.workspace.spaces.iter())
            .flat_map(|space| space.tabs.iter())
            .map(|tab| tab.pane.id)
            .collect()
    }

    /// Hands a waiting line to the shell it was waiting for, once the
    /// session reports a pane that was not there when it was asked.
    fn note_typing_target(&mut self) {
        let Some(typing) = self.typing.as_ref() else {
            return;
        };
        let Some(pane) = self
            .pane_ids()
            .into_iter()
            .find(|pane| !typing.known.contains(pane))
        else {
            return;
        };
        if let Some(typing) = self.typing.take() {
            self.typed = Some((pane, typing.text.into_bytes()));
        }
    }

    /// Folds a pane's damage into its snapshot, answering whether anything
    /// on screen changed: only the focused pane is drawn, so a background
    /// pane's paint is a frame for nobody unless it changed what the
    /// sidebar says about that agent.
    pub(super) fn absorb_damage(
        &mut self,
        damage: PaneDamage,
        identities: &[AgentIdentity],
        now: Instant,
    ) -> bool {
        let was_working = self.agent_is_working(damage.pane);
        if is_incremental_repaint(&damage) {
            self.note_agent_output(damage.pane, identities, now);
        }
        let seen =
            damage.pane == self.focused_pane() || was_working != self.agent_is_working(damage.pane);
        let entry = self
            .panes
            .entry(damage.pane)
            .or_insert_with(|| blank_pane(damage.pane, damage.columns, damage.rows));
        if entry.columns != damage.columns || entry.rows != damage.rows {
            *entry = blank_pane(damage.pane, damage.columns, damage.rows);
        }
        entry.cursor = damage.cursor;
        entry.alternate_screen = damage.alternate_screen;
        entry.mouse = damage.mouse;
        entry.bracketed_paste = damage.bracketed_paste;
        for (row, column, cell) in damage.changed {
            let index = usize::from(row) * usize::from(damage.columns) + usize::from(column);
            if let Some(slot) = entry.cells.get_mut(index) {
                *slot = cell;
            }
        }
        seen
    }

    /// Puts a released selection's text on the clipboard. A drag that
    /// covered only blanks copies nothing and says nothing, and the server
    /// has already put it away; an answer about a selection this client
    /// has since dropped is not one anybody is waiting for.
    pub(super) fn copy(&mut self, pane: uze_terminal::PaneId, text: String) {
        if !matches!(&self.selection, Some(Selection::Pane(selection)) if selection.pane == pane) {
            return;
        }
        if text.is_empty() {
            self.selection = None;
            return;
        }
        self.copy_selected(text);
    }

    /// Puts text a person marked on the clipboard, saying how much went —
    /// the text itself is what they just looked at.
    pub(super) fn copy_selected(&mut self, text: String) {
        let characters = text.chars().count();
        self.raise_toast(
            ToastKind::Done,
            "copied",
            format!(
                "{characters} character{} to the clipboard",
                if characters == 1 { "" } else { "s" }
            ),
            None,
        );
        self.clipboard = Some(text);
    }

    /// Clears an in-progress tab drag if the tab it names no longer exists
    /// — closed by another client, or by a concurrent `CloseTab`, while
    /// this one was mid-drag. Called on every `SessionUpdated`; leaves an
    /// unrelated drag (or none at all) alone.
    pub(super) fn prune_dragging_space(&mut self) {
        let Some(dragging) = self.dragging_space else {
            return;
        };
        let still_exists = self.session.as_ref().is_some_and(|session| {
            session
                .workspace
                .spaces
                .iter()
                .any(|space| space.id == dragging.space)
        });
        if !still_exists {
            self.dragging_space = None;
        }
    }

    pub(super) fn prune_dragging_tab(&mut self) {
        let Some(dragging) = self.dragging_tab else {
            return;
        };
        let still_exists = self.session.as_ref().is_some_and(|session| {
            session
                .workspace
                .spaces
                .iter()
                .any(|space| space.tabs.iter().any(|tab| tab.id == dragging.tab))
        });
        if !still_exists {
            self.dragging_tab = None;
        }
    }
}
