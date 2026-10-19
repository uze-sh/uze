//! Removing a free worktree from the space overview's Worktrees section.
//!
//! Asked first, as the overview's question (`overview_question`): a slot's directory
//! often holds gigabytes of build output, and nothing in the client gives
//! it back. Answered yes, the removal runs off the frame, where the
//! workspace judges the slot again under its locks and may refuse it: the
//! card was read before the question was, and what stands now decides.

use super::*;

/// The worktree asked about, named and sized as its card was.
pub(super) struct WorktreeRemoval {
    pub(super) path: PathBuf,
    pub(super) name: String,
    pub(super) bytes: u64,
}

/// What removing a worktree answered, for the overview of `space`.
pub(super) struct RemovalResolution {
    pub(super) space: SpaceId,
    pub(super) name: String,
    pub(super) bytes: u64,
    pub(super) outcome: std::result::Result<(), String>,
}

/// Whether the overview offers to remove `card`: a slot no task holds and
/// nobody works in. Everything else is somebody's, and is not offered.
pub(super) fn removable(card: &WorktreeCard<'_>) -> bool {
    card.state == WorktreeState::Free && card.checkout.task.is_none()
}

/// Why a card no agent is working in is not offered for removal, where
/// saying so tells the operator where to go instead.
pub(super) fn kept_because(card: &WorktreeCard<'_>) -> Option<&'static str> {
    match card.state {
        WorktreeState::Kept | WorktreeState::ToJoin => {
            Some("Its work is kept: discard or join it from recent.")
        }
        WorktreeState::Idle => Some("Its agent's task still holds it."),
        WorktreeState::Free if card.checkout.task.is_some() => {
            Some("Its last task still holds it.")
        }
        WorktreeState::Ready
        | WorktreeState::Working
        | WorktreeState::Free
        | WorktreeState::Unreadable => None,
    }
}

/// How the question names a worktree: under the project, the way the
/// operator would find it.
pub(super) fn removal_name(primary: &Path, path: &Path) -> String {
    path.strip_prefix(primary).map_or_else(
        |_| path.display().to_string(),
        |relative| relative.display().to_string(),
    )
}

impl WorkspaceModel {
    /// Whether the card selected in `overview`'s Worktrees section may be
    /// removed now.
    pub(super) fn offers_worktree_removal(&self, overview: &SpaceOverviewView) -> bool {
        overview.section == Section::Worktrees
            && !overview.removing
            && overview.project().is_some_and(|project| {
                self.worktree_cards(overview, project)
                    .get(overview.row())
                    .is_some_and(removable)
            })
    }
}

impl Attach<'_> {
    /// Asks about removing the worktree selected in the Worktrees
    /// section, when it is free and no removal is out.
    pub(super) fn ask_worktree_removal(&mut self) {
        let model = &self.model;
        let Some(overview) = model.space_overview.as_ref() else {
            return;
        };
        if !model.offers_worktree_removal(overview) {
            return;
        }
        let (Some(project), Some(Some(view))) = (overview.project(), &overview.worktrees.answer)
        else {
            return;
        };
        let cards = model.worktree_cards(overview, project);
        let Some(card) = cards.get(overview.row()) else {
            return;
        };
        let removal = WorktreeRemoval {
            path: card.checkout.path.clone(),
            name: removal_name(&view.primary, &card.checkout.path),
            bytes: card.checkout.bytes,
        };
        self.ask_overview_question(Asked::RemoveWorktree(removal));
    }

    /// Removes the worktree the operator said yes to, off the frame.
    pub(super) fn remove_worktree(&mut self, asked: WorktreeRemoval) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        overview.removing = true;
        let (space, root) = (overview.space, overview.root.clone());
        let occupied: Vec<PathBuf> = self
            .model
            .remembered
            .occupied_checkouts
            .iter()
            .cloned()
            .collect();
        self.model
            .set_busy_notice(format!("removing worktree {}", asked.name));
        spawn_worktree_removal(
            self.home,
            WorktreeRemovalRequest {
                space,
                root,
                path: asked.path,
                name: asked.name,
                bytes: asked.bytes,
                occupied,
            },
            self.channels.worktree_removals.sender.clone(),
        );
    }

    /// Removals that answered: said in a toast either way, and the
    /// section read again, since what is on disk is what changed.
    pub(super) fn absorb_worktree_removals(&mut self) {
        while let Ok(resolution) = self.channels.worktree_removals.receiver.try_recv() {
            self.model.absorb_worktree_removal(resolution);
        }
    }
}

impl WorkspaceModel {
    pub(super) fn absorb_worktree_removal(&mut self, resolution: RemovalResolution) {
        self.clear_busy_notice();
        let open = self
            .space_overview
            .as_mut()
            .filter(|overview| overview.space == resolution.space);
        let overview = match open {
            Some(overview) => Some(overview),
            None => self.put_away_overviews.get_mut(&resolution.space),
        };
        if let Some(overview) = overview {
            overview.removing = false;
            overview.worktrees.stale = true;
        }
        match resolution.outcome {
            Ok(()) => self.raise_toast(
                crate::ui::widget::ToastKind::Done,
                "worktree removed",
                format!(
                    "{} · {} freed",
                    resolution.name,
                    bytes_to_say(resolution.bytes)
                ),
                None,
            ),
            Err(reason) => self.raise_toast(
                crate::ui::widget::ToastKind::Failed,
                "worktree kept",
                format!("{}: {reason}", resolution.name),
                None,
            ),
        }
        self.dirty = true;
    }
}
