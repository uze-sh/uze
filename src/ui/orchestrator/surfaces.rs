//! Opening and closing the extension surfaces over the workspace, every one hosted in a slot: code, architect, spec and the views plugins declare.

use super::*;
use uze_extensions::contract::{Entry, Outcome};

impl WorkspaceModel {
    pub(super) fn offers_extension(&self, id: &str) -> bool {
        !self.disabled_extensions.contains(id)
    }

    pub(super) fn offers_action(&self, action: Action) -> bool {
        crate::ui::extension_switch::offered(action, &self.disabled_extensions)
    }

    /// Takes the operator's latest switch into this client: a surface
    /// standing in the pane for an extension switched off closes, and what
    /// its sidebar section was drawn from is let go so nothing of it stays
    /// on screen until the next read — which it no longer schedules.
    pub(super) fn follow_extension_switch(&mut self, disabled: std::collections::BTreeSet<String>) {
        if disabled == self.disabled_extensions {
            return;
        }
        self.disabled_extensions = disabled;
        if !self.offers_extension(code::CATALOG.id) {
            self.remembered.git_badge = None;
            self.commit_detail = None;
            self.commit_detail_pending = None;
        }
        // What a section switched off was drawn from goes now, not on
        // the next pass.
        let offered: Vec<bool> = self
            .remembered
            .declared
            .engines
            .keys()
            .map(|owner| self.offers_declared(owner))
            .collect();
        for (engine, offered) in self.remembered.declared.engines.values_mut().zip(offered) {
            if !offered {
                engine.focus(None);
            }
        }
        if self
            .slot_in_pane()
            .is_some_and(|slot| !self.offers_declared(&slot.owner))
        {
            self.close_surface();
        }
        self.close_drawers_switched_off();
        self.register_extension_keys();
        self.dirty = true;
    }

    /// Closes whichever surface is standing in the pane.
    pub(super) fn close_extension(&mut self) {
        self.close_surface();
    }

    /// Closes the open surface once its tab is no longer the one in front.
    pub(super) fn close_extension_left_behind(&mut self) {
        let in_front = self
            .session
            .as_ref()
            .map(|session| session.selected_tab().id);
        if in_front != self.extension_tab {
            self.close_extension();
        }
    }

    /// Notes the tab in front as the one a surface opening now stands in
    /// for.
    pub(super) fn stand_extension_in_front(&mut self) {
        self.put_away_space_overview();
        self.extension_tab = self
            .session
            .as_ref()
            .map(|session| session.selected_tab().id);
    }
}

pub(super) fn open_architect(model: &mut WorkspaceModel) {
    let slot = model
        .remembered
        .declared
        .builtin_slot(architect::CATALOG.id, architect::SURFACE);
    if let Some(slot) = slot {
        open_surface(model, slot);
    }
}

pub(super) fn open_spec(model: &mut WorkspaceModel) {
    let slot = model
        .remembered
        .declared
        .builtin_slot(spec::CATALOG.id, spec::DOCUMENT);
    if let Some(slot) = slot {
        open_surface(model, slot);
    }
}

/// The code surface, opened on a path a diagram pointed at: the last
/// step down from an architecture is the file, and this is that step.
///
/// Rooted at the project rather than at the tab's directory, because the
/// path was written relative to the project and may sit outside a tab
/// that is somewhere below it.
pub(super) fn open_code_at(model: &mut WorkspaceModel, project: &Path, target: &Path) {
    let Some(slot) = code_slot(model) else {
        return;
    };
    if !model.offers_extension(code::CATALOG.id) {
        return;
    }
    let entry = Entry::Path {
        target: target.to_path_buf(),
        directory: target.is_dir(),
    };
    if let Some(engine) = code_engine(model) {
        engine.enter(code::SURFACE, entry);
    }
    open_surface_on(model, slot, project.to_path_buf());
}

/// The code surface through one of its doors, on the active tab's
/// checkout. Two doors rather than one because a person knows whether
/// they are reviewing or navigating before they press anything; pressed
/// while the surface stands, the door already open shuts it and the other
/// shows that half.
pub(super) fn open_code(model: &mut WorkspaceModel, door: &str) {
    let Some(slot) = code_slot(model) else {
        return;
    };
    let standing = model.open_slot == Some(slot);
    if !standing && (!model.offers_extension(code::CATALOG.id) || model.focused_cwd().is_none()) {
        return;
    }
    let entered =
        code_engine(model).map(|engine| engine.enter(code::SURFACE, Entry::Door(door.to_owned())));
    match entered {
        Some(Outcome::Close) if standing => model.close_surface(),
        _ if standing => model.dirty = true,
        _ => open_surface(model, slot),
    }
}

fn code_slot(model: &WorkspaceModel) -> Option<SlotId> {
    model
        .remembered
        .declared
        .builtin_slot(code::CATALOG.id, code::SURFACE)
}

fn code_engine(model: &mut WorkspaceModel) -> Option<&mut Box<dyn Hosted>> {
    model
        .remembered
        .declared
        .engines
        .get_mut(&Owner::Builtin(code::CATALOG.id))
}
