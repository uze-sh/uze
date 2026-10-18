//! Removing a free worktree from the space overview's Worktrees section.
//!
//! Asked first, as a dialog over the whole client: a slot's directory
//! often holds gigabytes of build output, and nothing in the client gives
//! it back. Answered yes, the removal runs off the frame, where the
//! workspace judges the slot again under its locks and may refuse it: the
//! card was read before the question was, and what stands now decides.

use super::*;
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};

/// The open question: which worktree, named and sized as its card was.
pub(super) struct WorktreeRemoval {
    pub(super) path: PathBuf,
    pub(super) name: String,
    pub(super) bytes: u64,
    pub(super) on_confirm: bool,
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
    pub(super) fn asks_worktree_removal(&self) -> bool {
        self.space_overview
            .as_ref()
            .is_some_and(|overview| overview.removal.is_some())
    }

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
            on_confirm: false,
        };
        if let Some(overview) = self.model.space_overview.as_mut() {
            overview.removal = Some(removal);
            self.model.dirty = true;
        }
    }

    /// The operator's answer, by click or by key: yes removes the worktree
    /// off the frame; no leaves it.
    pub(super) fn answer_worktree_removal(&mut self, confirm: bool) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        let Some(asked) = overview.removal.take() else {
            return;
        };
        self.model.dirty = true;
        if !confirm {
            return;
        }
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

    /// A key while the question is open: it answers, moves between the
    /// answers, or does nothing — the dialog seals everything behind it.
    pub(super) fn worktree_removal_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                let confirm = self
                    .model
                    .space_overview
                    .as_ref()
                    .and_then(|overview| overview.removal.as_ref())
                    .is_some_and(|asked| asked.on_confirm);
                self.answer_worktree_removal(confirm);
            }
            Action::Dismiss => self.answer_worktree_removal(false),
            Action::FocusNext | Action::FocusPrevious => {
                if let Some(asked) = self
                    .model
                    .space_overview
                    .as_mut()
                    .and_then(|overview| overview.removal.as_mut())
                {
                    asked.on_confirm = !asked.on_confirm;
                    self.model.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// A press while the question is open: on an answer it is that answer,
    /// on the question nothing, and anywhere else it withdraws the question.
    pub(super) fn worktree_removal_press(&mut self, column: u16, row: u16) {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::OverviewRemovalAnswer(confirm)) => {
                self.answer_worktree_removal(confirm);
            }
            Some(WorkspaceHit::OverviewRemovalBody) => {}
            _ => self.answer_worktree_removal(false),
        }
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

/// The question, drawn over everything but the toasts.
pub(super) fn render_worktree_removal(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    asked: &WorktreeRemoval,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    crate::ui::widget::scrim::render(frame, area);
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: Tone::Danger,
            title: "Remove this worktree?",
            subject: Some(Line::from(Span::styled(
                format!("{} ({})", asked.name, bytes_to_say(asked.bytes)),
                theme::fg(Token::TextCode),
            ))),
            body: vec![
                "Its directory is deleted, build output included. Its branch stays, and so \
                 does every commit on it."
                    .to_owned(),
            ],
            confirm: Some("Remove"),
            focus: Some(if asked.on_confirm { 1 } else { CANCEL }),
            field: None,
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::OverviewRemovalAnswer(false),
        WorkspaceHit::OverviewRemovalAnswer(true),
    );
    // Before everything it covers, and the dialog's own body before that:
    // a click on the question is not a click on what is underneath.
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, WorkspaceHit::OverviewRemovalBody)]),
    );
}
