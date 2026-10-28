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
        self.remembered
            .declared
            .slots
            .iter()
            .filter(|slot| {
                slot.is_bar()
                    && slot.owner.declared().is_some()
                    && self.offers_declared(&slot.owner)
                    && self.enabled_in_front(&slot.owner)
            })
            .collect()
    }

    /// The views and drawers the bar has a door for: those of an extension
    /// enabled in the project in front, as its items are. A door to an
    /// extension that runs nothing here was a row of bare titles, one per
    /// surface, with nothing to say what they were; the way to enable it is
    /// the offer and the Extensions screen.
    pub(in crate::ui::orchestrator) fn bar_doors(&self) -> Vec<&HostedSlot> {
        self.declared_views()
            .chain(self.declared_drawers())
            .filter(|slot| self.enabled_in_front(&slot.owner))
            .collect()
    }

    /// Whether `owner` runs in the project of the pane in front.
    fn enabled_in_front(&self, owner: &Owner) -> bool {
        self.project_of_pane(self.focused_pane())
            .as_ref()
            .is_some_and(|project| self.enabled_in(owner, project))
    }

    /// Whether a plugin puts anything in the bar: a door or an item.
    pub(in crate::ui::orchestrator) fn has_bar(&self) -> bool {
        !self.bar_doors().is_empty() || !self.bar_items().is_empty()
    }

    /// The surfaces an item of the bar in front opens: their item is their
    /// door, so the bar draws them none of their own.
    pub(in crate::ui::orchestrator) fn opened_by_items(&self) -> Vec<SlotId> {
        self.bar_items()
            .into_iter()
            .filter_map(|item| match self.bar_target(item)? {
                WorkspaceHit::OpenDeclared(slot) | WorkspaceHit::ToggleDrawer(slot) => Some(slot),
                _ => None,
            })
            .collect()
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
