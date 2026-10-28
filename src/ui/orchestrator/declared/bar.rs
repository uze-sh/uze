//! The bar under the agent area, the tab strip's mirror at the column's
//! foot: what plugins add to the workspace, apart from what UZE ships. Its
//! left holds the doors to their views and drawers, then the items that
//! stand on the left; its right, the items that stand on the right. Items
//! keep the order their extensions were installed in, so none can claim
//! the bar's start.

use super::*;

impl WorkspaceModel {
    /// The items of the bar the checkout in front shows: as a status row,
    /// only where its extension is enabled.
    pub(in crate::ui::orchestrator) fn bar_items(&self) -> Vec<&HostedSlot> {
        let in_front = self.project_of_pane(self.focused_pane());
        self.remembered
            .declared
            .slots
            .iter()
            .filter(|slot| {
                slot.is_bar()
                    && slot.owner.declared().is_some()
                    && self.offers_declared(&slot.owner)
                    && in_front
                        .as_ref()
                        .is_some_and(|project| self.enabled_in(&slot.owner, project))
            })
            .collect()
    }

    /// Whether a plugin puts anything in the bar: a door or an item.
    pub(in crate::ui::orchestrator) fn has_bar(&self) -> bool {
        self.declared_views().next().is_some()
            || self.declared_drawers().next().is_some()
            || !self.bar_items().is_empty()
    }

    /// What a click on `item` does: the view or the drawer of its own
    /// extension that its `opens` names.
    pub(in crate::ui::orchestrator) fn bar_target(
        &self,
        item: &HostedSlot,
    ) -> Option<WorkspaceHit> {
        let opens = item.surface.opens.as_ref()?;
        let target = self
            .remembered
            .declared
            .slots
            .iter()
            .find(|slot| slot.owner == item.owner && &slot.surface.id == opens)?;
        match target.surface.slot {
            SurfaceSlot::View => Some(WorkspaceHit::OpenDeclared(target.id)),
            SurfaceSlot::Drawer => Some(WorkspaceHit::ToggleDrawer(target.id)),
            _ => None,
        }
    }
}
