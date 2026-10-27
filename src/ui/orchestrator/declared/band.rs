//! A plugin's bands, laid along the top or the bottom of the agent's pane.
//!
//! A band's rows are the manifest's, not its answer's: the layout takes
//! them before the pane's terminal is sized (`compute_layout`), whenever
//! the agent in front works in a project the extension is enabled in, so
//! nothing a band answers, nor answering nothing, ever resizes the agent.
//! The pane's mouse and selection are measured from the rect that is left,
//! which is what offsets them by the band.

use super::*;
use crate::ui::orchestrator::render::{BandRows, Reserved};

impl WorkspaceModel {
    /// What the layout takes from the agent area before the pane is sized.
    pub(in crate::ui::orchestrator) fn reserved(&self) -> Reserved {
        Reserved {
            dock: self.dock(),
            bands: self
                .bands_in_front()
                .into_iter()
                .filter_map(|slot| match slot.surface.slot {
                    SurfaceSlot::Band { rows, bottom } => Some(BandRows {
                        slot: slot.id,
                        rows: u16::from(rows),
                        bottom,
                    }),
                    _ => None,
                })
                .collect(),
        }
    }

    /// The bands over the pane in front, in slot order: while the agent
    /// whose context is in front has its own tab selected, every band of
    /// an extension switched on and enabled in the project it works in.
    pub(in crate::ui::orchestrator) fn bands_in_front(&self) -> Vec<&HostedSlot> {
        let Some(project) = self.agent_band_project() else {
            return Vec::new();
        };
        self.remembered
            .declared
            .slots
            .iter()
            .filter(|slot| {
                matches!(slot.surface.slot, SurfaceSlot::Band { .. })
                    && self.offers_slot(slot)
                    && self.enabled_in(&slot.owner, &project)
            })
            .collect()
    }

    /// The project of the agent in front, while its own tab is the one
    /// selected: a shell beside it is no agent's pane.
    fn agent_band_project(&self) -> Option<PathBuf> {
        let agent = self.agent_tab_in_front?;
        if self.selected_tab() != Some(agent) {
            return None;
        }
        self.projects.get(self.agent_directory_in_front()?).cloned()
    }

    /// Whether the operator's record has `owner` enabled in `project`, as
    /// the last discovery read it. One that runs nothing is on wherever
    /// its switch is.
    pub(in crate::ui::orchestrator) fn enabled_in(&self, owner: &Owner, project: &Path) -> bool {
        let Some(extension) = owner
            .declared()
            .and_then(|identity| self.remembered.declared.extensions.get(identity))
        else {
            return false;
        };
        if !extension.per_project {
            return true;
        }
        // Compared as written: the record keeps each project by its
        // canonical root, which is what the session placed the agent in,
        // and nothing drawn may ask the filesystem.
        extension
            .record
            .as_ref()
            .and_then(Option::as_ref)
            .is_some_and(|consent| consent.enabled_in.contains(project))
    }

    /// What the band in `slot` draws in `space`: its engine's answer, or
    /// nothing while it has none, which leaves its rows blank.
    pub(in crate::ui::orchestrator) fn band_view(&self, slot: SlotId, space: Size) -> Option<View> {
        let slot = self.remembered.declared.slot(slot)?;
        self.remembered
            .declared
            .engine(&slot.owner)?
            .view(&slot.surface.id, space)
            .and_then(drawn_tree)
    }
}
