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
        let kept: Vec<TabId> = self
            .remembered
            .declared
            .standing
            .iter()
            .filter(|(_, standing)| !self.offers_declared(&standing.owner))
            .map(|(tab, _)| *tab)
            .collect();
        for tab in kept {
            self.remembered.declared.standing.remove(&tab);
        }
        self.close_drawers_switched_off();
        self.close_overlay_switched_off();
        self.register_extension_keys();
        self.dirty = true;
    }

    /// Follows the tab in front: the surface standing for another tab is
    /// put away for it, and the tab in front gets back what it was left
    /// with, unless the overview holds the pane. Answers the session and
    /// every frame alike, so however the pane was given back — the overview
    /// put away, a reattach — what the tab kept stands in it again.
    pub(super) fn settle_pane(&mut self) {
        let in_front = self.selected_tab();
        if in_front != self.extension_tab {
            self.suspend_surface();
            self.extension_tab = in_front;
        }
        self.forget_surfaces_of_closed_tabs();
        if self.open_slot.is_some() || self.space_overview.is_some() {
            return;
        }
        if let Some(tab) = in_front {
            self.restore_surface(tab);
        }
    }

    /// Puts the open surface away for the tab it stands in, holding what
    /// it keeps for that tab's return.
    pub(super) fn suspend_surface(&mut self) {
        if self.open_slot.is_none() {
            return;
        }
        let (Some(tab), Some(slot)) = (self.extension_tab, self.slot_in_pane()) else {
            self.close_surface();
            return;
        };
        let (owner, surface) = (slot.owner.clone(), slot.surface.id.clone());
        let held = self
            .remembered
            .declared
            .engine(&owner)
            .and_then(|engine| engine.hold(&surface));
        let navigator = std::mem::take(&mut self.navigator_scroll);
        let inspecting = self.inspecting;
        let root = self.open_root.clone();
        self.close_surface();
        if let Some(root) = root {
            self.remembered.declared.standing.insert(
                tab,
                Standing {
                    owner,
                    surface,
                    root,
                    held,
                    navigator,
                    inspecting,
                },
            );
        }
    }

    /// Stands in `tab`'s pane what was put away for it, when its extension
    /// still declares the surface and is offered.
    fn restore_surface(&mut self, tab: TabId) {
        let Some(standing) = self.remembered.declared.standing.remove(&tab) else {
            return;
        };
        let slot = self
            .remembered
            .declared
            .slots
            .iter()
            .find(|slot| slot.owner == standing.owner && slot.surface.id == standing.surface)
            .map(|slot| slot.id);
        let Some(slot) = slot else {
            return;
        };
        if open_surface_with(self, slot, standing.root, Resumed::Held(standing.held)) {
            self.navigator_scroll = standing.navigator;
            self.inspecting = standing.inspecting;
        }
    }

    fn forget_surfaces_of_closed_tabs(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let live: std::collections::BTreeSet<TabId> = session
            .workspace
            .spaces
            .iter()
            .flat_map(|space| space.tabs.iter().map(|tab| tab.id))
            .collect();
        self.remembered
            .declared
            .standing
            .retain(|tab, _| live.contains(tab));
    }

    /// Notes the tab in front as the one a surface opening now stands in
    /// for.
    pub(super) fn stand_extension_in_front(&mut self) {
        self.put_away_space_overview();
        self.extension_tab = self.selected_tab();
        if let Some(tab) = self.extension_tab {
            self.remembered.declared.standing.remove(&tab);
        }
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
