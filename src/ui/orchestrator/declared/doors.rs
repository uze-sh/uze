//! A declared extension's views and overlays as entries of the action
//! index: the pointer's way to a view is its button on the tab strip, and
//! the keyboard's is the index, as for every surface the host ships; an
//! overlay's is the index, or an `open` action naming it.
//!
//! Each view is one interned extension action, `open:<surface>`, under its
//! extension's owner. No manifest id holds a colon, so the name never meets
//! one of the extension's own actions, and no table binds it: it holds no
//! key until `jump` brings global keys for views (change 6).

use super::*;

/// What a view's door is named after its owner.
const OPEN: &str = uze_keys::extension::DOOR;

impl WorkspaceModel {
    /// The index entry of every view a declared extension that is switched
    /// on offers, labelled with its title and described with whose it is.
    pub(in crate::ui::orchestrator) fn declared_view_doors(&self) -> Vec<uze_keys::Action> {
        self.remembered
            .declared
            .slots
            .iter()
            .filter(|slot| (slot.is_view() || slot.is_overlay()) && self.offers_slot(slot))
            .filter_map(|slot| {
                let identity = slot.owner.declared()?;
                let name = uze_keys::extension::action_name(
                    &crate::keymap::extension_owner(identity),
                    &format!("{OPEN}{}", slot.surface.id),
                );
                let id = uze_keys::extension::intern(&name);
                uze_keys::extension::describe(
                    id,
                    uze_keys::extension::Described {
                        label: format!("Open {}", slot.surface.title),
                        description: crate::ui::extension_view::tree::attribution(identity),
                        destructive: false,
                    },
                );
                Some(uze_keys::Action::Extension(id))
            })
            .collect()
    }

    /// Opens the view `action` is the door of, and says whether it was
    /// one: any other extension action is the surface in front's to run.
    pub(in crate::ui::orchestrator) fn open_declared_door(
        &mut self,
        action: uze_keys::ExtensionActionId,
    ) -> bool {
        let local = uze_keys::extension::local(action);
        let Some(surface) = local.strip_prefix(OPEN) else {
            return false;
        };
        let owner = uze_keys::extension::owner(action);
        let door = self.remembered.declared.slots.iter().find(|slot| {
            (slot.is_view() || slot.is_overlay())
                && slot.surface.id == surface
                && slot
                    .owner
                    .declared()
                    .is_some_and(|identity| crate::keymap::extension_owner(identity) == owner)
        });
        match door.map(|slot| (slot.id, slot.is_overlay())) {
            Some((id, true)) => self.open_overlay(id),
            Some((id, false)) => {
                self.close_overlay();
                open_surface(self, id);
            }
            None => {}
        }
        true
    }
}
