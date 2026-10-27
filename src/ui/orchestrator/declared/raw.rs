//! Raw input to a focused resident surface (`extension-media-and-resident`
//! M5): the one place an extension is handed keys and the pointer rather
//! than the host's commands.
//!
//! A view in the pane or an overlay showing a resident source reads them
//! while nothing stands over it. Esc, the host's escape chord, lets go of
//! it (the next Esc puts it away, as a docked drawer's does); F1 and F8,
//! the index and the pause, stay the host's. Neither ever reaches the
//! extension, and neither can be rebound for it. A press on the surface
//! takes it back.

use super::*;
use uze_extensions::contract::RawInput;

impl WorkspaceModel {
    /// The resident surface that would read raw input now, whether or not
    /// the operator let go of it: the overlay standing with nothing over
    /// it, or the view in the pane with nothing over it.
    pub(in crate::ui::orchestrator) fn resident_slot(&self) -> Option<SlotId> {
        let id = match self.open_overlay {
            Some(_) if self.anything_over_the_overlay() => return None,
            Some(id) => id,
            None if !self.chrome_answers() || self.dock_focused => return None,
            None => self.open_slot?,
        };
        let slot = self.remembered.declared.slot(id)?;
        self.remembered
            .declared
            .engine(&slot.owner)?
            .reads_raw_input(&slot.surface.id)
            .then_some(id)
    }

    /// The resident surface reading raw input now.
    pub(in crate::ui::orchestrator) fn raw_input_slot(&self) -> Option<SlotId> {
        self.resident_slot().filter(|_| !self.resident_released)
    }

    /// What stands over an overlay and takes its keys first.
    fn anything_over_the_overlay(&self) -> bool {
        self.action_index.is_some()
            || self.release_notes.is_some()
            || self.approval.is_some()
            || self.extension_offer.is_some()
            || self.interrupt.is_some()
            || self.proposal.is_some()
            || self.asks_overview_question()
            || self.shows_space_issues()
    }

    /// Esc on a focused resident surface: its keys are the host's again.
    pub(in crate::ui::orchestrator) fn release_resident(&mut self) {
        self.resident_released = true;
        self.dirty = true;
    }

    /// Hands `input` to the resident surface in slot `id`, which reads it
    /// from now on.
    pub(in crate::ui::orchestrator) fn give_raw_input(&mut self, id: SlotId, input: RawInput) {
        self.resident_released = false;
        if let Some((surface, engine)) = self.remembered.declared.slot_engine_mut(id) {
            engine.raw_input(&surface, input);
        }
    }

    /// Where the content of the resident surface in slot `id` stood on
    /// the last frame.
    pub(in crate::ui::orchestrator) fn raw_area(&self, id: SlotId) -> Rect {
        if self.open_overlay == Some(id) {
            self.overlay_area.get()
        } else {
            self.surface_frame.content_area
        }
    }
}

/// A chord as the keymap spells it, apart: the key alone, and its
/// modifiers in the keymap's order.
pub(in crate::ui::orchestrator) fn raw_key(chord: uze_keys::Chord) -> RawInput {
    let key = uze_keys::Chord::new(uze_keys::Mods::default(), chord.key).to_string();
    let mods = [
        (chord.mods.ctrl, "ctrl"),
        (chord.mods.alt, "alt"),
        (chord.mods.shift, "shift"),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .map(|(_, name)| name.to_owned())
    .collect();
    RawInput::Key { key, mods }
}
