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

    /// Follows the tab in front: the surface standing for another agent is
    /// put away for it, and the agent in front gets back what it was left
    /// with, unless the overview holds the pane. Another tab of the same
    /// agent — its own, or a shell beside it — is a terminal of the agent
    /// in front chosen, which takes the pane back as choosing the tab in
    /// front does: the sidebar marks the agent whichever of its tabs is in
    /// front, so a surface kept per tab was one the operator could not
    /// tell was waiting on the shell. Asked after every session the server
    /// sends, and when the overview is dismissed over the tab it stood on
    /// — never on a frame, where a tab being left would get its surface
    /// back for the round trip before the move is confirmed.
    pub(super) fn settle_pane(&mut self) {
        let in_front = self.in_front();
        if in_front != self.extension_tab {
            let same_agent =
                in_front.map(|front| front.agent) == self.extension_tab.map(|front| front.agent);
            if same_agent {
                self.close_surface();
                if let Some(front) = in_front {
                    self.remembered.declared.standing.remove(&front.agent);
                }
            } else {
                self.suspend_surface();
            }
            self.extension_tab = in_front;
        }
        self.forget_surfaces_of_closed_tabs();
        if self.open_slot.is_some() || self.space_overview.is_some() {
            return;
        }
        if let Some(front) = in_front {
            self.restore_surface(front.agent);
        }
    }

    /// The tab in front and the agent it belongs to.
    fn in_front(&self) -> Option<InFront> {
        let tab = self.selected_tab()?;
        let agent = self.tab(tab).and_then(|tab| tab.agent).unwrap_or(tab);
        Some(InFront { tab, agent })
    }

    /// Puts the open surface away for the agent it stands in for, holding
    /// what it keeps for that agent's return.
    pub(super) fn suspend_surface(&mut self) {
        if self.open_slot.is_none() {
            return;
        }
        let (Some(front), Some(slot)) = (self.extension_tab, self.slot_in_pane()) else {
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
        let context = self.open_context.clone();
        let root = self.open_root.clone();
        self.close_surface();
        if let Some(root) = root {
            self.remembered.declared.standing.insert(
                front.agent,
                Standing {
                    owner,
                    surface,
                    root,
                    context,
                    held,
                    navigator,
                    inspecting,
                },
            );
        }
    }

    /// Stands in the pane what was put away for `agent`, when the tab in
    /// front still answers for the agent directory it was opened on and
    /// its extension still declares the surface and is offered.
    fn restore_surface(&mut self, agent: TabId) {
        let Some(standing) = self.remembered.declared.standing.remove(&agent) else {
            return;
        };
        if standing.context != self.focused_cwd() {
            return;
        }
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
        if self.remembered.declared.standing.is_empty() {
            return;
        }
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

    /// Choosing the tab or the space already in front: the pane is had
    /// back from whatever stands in it, and what its agent kept is let go.
    pub(super) fn take_pane_back(&mut self) {
        self.put_away_space_overview();
        self.close_surface();
        if let Some(front) = self.in_front() {
            self.remembered.declared.standing.remove(&front.agent);
        }
    }

    /// The overview put away by the operator, staying on the tab it stood
    /// over: what the tab kept stands in the pane again at once.
    pub(super) fn dismiss_space_overview(&mut self) {
        self.put_away_space_overview();
        self.settle_pane();
    }

    /// Notes the agent in front as the one a surface opening now stands
    /// in for, and what it opens as what the agent keeps from now on.
    pub(super) fn stand_extension_in_front(&mut self) {
        self.put_away_space_overview();
        self.extension_tab = self.in_front();
        if let Some(front) = self.extension_tab {
            self.remembered.declared.standing.remove(&front.agent);
        }
    }
}

/// The tab the pane is settled for, and the agent it belongs to: its own
/// id for an agent's tab or a tab of the space, the agent's for a shell
/// opened beside one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InFront {
    tab: TabId,
    agent: TabId,
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
