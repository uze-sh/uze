//! The bar under the agent area, the tab strip's mirror at the column's
//! foot: what extensions add to the workspace. The tab strip is UZE's own
//! and the bundled extensions'; the bar takes every extension's items, a
//! plugin's and a bundled one's alike, since it is the one place a plugin
//! may stand. Its left holds the doors to plugins' views and drawers, then
//! the items that stand on the left; its right, the items that stand on
//! the right. Items keep the order their extensions were installed in, so
//! none can claim the bar's start.

use super::*;

impl WorkspaceModel {
    /// The items of the bar the checkout in front shows: as a status row,
    /// only where its extension is switched on and runs.
    pub(in crate::ui::orchestrator) fn bar_items(&self) -> Vec<&HostedSlot> {
        self.remembered
            .declared
            .slots
            .iter()
            .filter(|slot| {
                slot.is_bar()
                    && self.offers_declared(&slot.owner)
                    && self.runs_in_front(&slot.owner)
            })
            .collect()
    }

    /// Whether `owner` runs for the pane in front: a bundled extension
    /// wherever there is a checkout in front, a plugin's where it is
    /// enabled in that pane's project.
    fn runs_in_front(&self, owner: &Owner) -> bool {
        match owner {
            Owner::Builtin(_) => self.focused_cwd().is_some(),
            Owner::Declared(_) => self.enabled_in_front(owner),
        }
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

    /// Whether anything stands in the bar: a plugin's door or item, by
    /// what is installed, so the pane is not resized by what one answers;
    /// a bundled extension's item only while it has something to say,
    /// since one is always installed and an empty row on every agent is a
    /// row taken from each for nothing.
    pub(in crate::ui::orchestrator) fn has_bar(&self) -> bool {
        self.release_waiting().is_some()
            || !self.bar_doors().is_empty()
            || self
                .bar_items()
                .into_iter()
                .any(|slot| slot.owner.declared().is_some() || self.item_draws(slot))
    }

    /// Whether `item` has anything to draw now.
    fn item_draws(&self, item: &HostedSlot) -> bool {
        self.remembered
            .declared
            .engine(&item.owner)
            .and_then(|engine| {
                engine.view(
                    &item.surface.id,
                    uze_extensions::view::Size {
                        width: crate::ui::orchestrator::render::BAR_ITEM_MAX,
                        height: 1,
                    },
                )
            })
            .and_then(super::status::drawn_tree)
            .is_some()
    }

    /// The release installed and waiting on a restart, which the bar offers.
    pub(in crate::ui::orchestrator) fn release_waiting(&self) -> Option<&str> {
        self.release.as_ref()?.waiting.as_deref()
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
            SurfaceSlot::Drawer(_) => Some(WorkspaceHit::ToggleDrawer(target.id)),
            _ => None,
        }
    }
}
