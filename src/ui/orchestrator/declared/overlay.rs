//! A plugin's overlay: a card over the frame, opened by an `open` action
//! naming it or by its entry in the action index, under the host's
//! attribution strip.
//!
//! It is modal while it stands: every key and click is its, a click
//! outside it or Esc puts it away, and whatever had the keys before it has
//! them again, since nothing behind it moved while it was up.

use super::*;

impl WorkspaceModel {
    /// The overlay standing over the frame, while its extension is on.
    pub(in crate::ui::orchestrator) fn overlay_slot(&self) -> Option<&HostedSlot> {
        self.remembered
            .declared
            .slot(self.open_overlay?)
            .filter(|slot| slot.is_overlay() && self.offers_declared(&slot.owner))
    }

    /// Opens the overlay in slot `id` over whatever is on the frame,
    /// about the checkout in front. Another one standing is put away
    /// first: one overlay at a time.
    pub(in crate::ui::orchestrator) fn open_overlay(&mut self, id: SlotId) {
        let Some(slot) = self.remembered.declared.slot(id) else {
            return;
        };
        if !slot.is_overlay() || !self.offers_declared(&slot.owner) {
            return;
        }
        self.close_overlay();
        self.action_index = None;
        self.context_menu = None;
        let checkout = self.focused_cwd().map(|root| Checkout {
            display: crate::ui::display_project_path(&root),
            target: self.remembered.targets.get(&evaluation_key(&root)).cloned(),
            root,
        });
        if let (Some(checkout), Some((surface, engine))) =
            (checkout, self.remembered.declared.slot_engine_mut(id))
        {
            engine.open(&surface, &checkout);
        }
        self.open_overlay = Some(id);
        self.overlay_notice = None;
        self.dirty = true;
    }

    /// Puts the overlay away, its engine told so.
    pub(in crate::ui::orchestrator) fn close_overlay(&mut self) {
        let Some(id) = self.open_overlay.take() else {
            return;
        };
        self.overlay_notice = None;
        if let Some((surface, engine)) = self.remembered.declared.slot_engine_mut(id) {
            engine.close(&surface);
        }
        self.dirty = true;
    }

    /// Puts away an overlay whose extension was switched off.
    pub(in crate::ui::orchestrator) fn close_overlay_switched_off(&mut self) {
        if self.open_overlay.is_some() && self.overlay_slot().is_none() {
            self.close_overlay();
        }
    }

    /// The room the overlay's view had on the last frame.
    pub(in crate::ui::orchestrator) fn overlay_space(&self) -> Size {
        self.open_overlay
            .and_then(|id| self.slot_spaces.borrow().get(&id).copied())
            .unwrap_or_default()
    }

    /// What the overlay draws in `space`, with the host's last word from
    /// its engine, and no footer close: the card's own mark closes it.
    pub(in crate::ui::orchestrator) fn overlay_view(&self, space: Size) -> Option<View> {
        let slot = self.overlay_slot()?;
        let mut view = self.surface_view(slot, space);
        view.footer.retain(|command| *command != Command::Close);
        if let Some(notice) = &self.overlay_notice {
            view.notice = Some(notice.clone());
        }
        Some(view)
    }

    /// Hands the overlay's engine a command.
    pub(in crate::ui::orchestrator) fn tell_overlay(&mut self, command: Command) {
        let Some(id) = self.open_overlay else {
            return;
        };
        let space = self.overlay_space();
        let outcome = self
            .remembered
            .declared
            .slot_engine_mut(id)
            .map(|(surface, engine)| engine.command(&surface, command, space))
            .unwrap_or(Outcome::Stay);
        self.follow_overlay(outcome);
    }

    /// What the overlay's engine answered, followed. Leaving for a view
    /// or a path puts it away first, since it covers where they open.
    pub(in crate::ui::orchestrator) fn follow_overlay(&mut self, outcome: Outcome) {
        let id = self.open_overlay;
        match outcome {
            Outcome::Stay | Outcome::OpenCommit { .. } => {}
            Outcome::Close => self.close_overlay(),
            Outcome::Notice(span) => self.overlay_notice = Some(span),
            Outcome::OpenPath { project, target } => {
                self.close_overlay();
                open_code_at(self, &project, &target);
            }
            Outcome::Copy(text) => self.copy_answered_by(text, id),
            Outcome::Toast(text) => {
                if let Some(owner) = self.overlay_slot().map(|slot| slot.owner.clone()) {
                    self.say_told(&owner, text);
                }
            }
            Outcome::Show(surface) => {
                if let Some(id) = id {
                    self.show_surface(id, &surface);
                }
            }
            Outcome::Failed { title, detail } => {
                let detail = self.attributed(id, detail);
                self.say_failed(&title, detail);
            }
        }
        self.dirty = true;
    }
}
