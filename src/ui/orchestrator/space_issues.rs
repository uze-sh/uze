//! The space overview's list of what to check: the count in its foot opens
//! it over the whole client, the way the management's health status opens
//! its own, and anything but a click on the list puts it away.

use super::*;
use crate::ui::widget::dialog;

impl WorkspaceModel {
    pub(super) fn shows_space_issues(&self) -> bool {
        self.space_overview
            .as_ref()
            .is_some_and(|overview| overview.issues_open)
    }

    pub(super) fn set_space_issues(&mut self, open: bool) {
        if let Some(overview) = self.space_overview.as_mut() {
            overview.issues_open = open;
            self.dirty = true;
        }
    }
}

impl Attach<'_> {
    /// An action while the list is open: it only closes.
    pub(super) fn space_issues_action(&mut self, action: Action) {
        if matches!(action, Action::Dismiss | Action::Activate) {
            self.model.set_space_issues(false);
        }
    }

    /// A press while the list is open: on it nothing, anywhere else it
    /// closes.
    pub(super) fn space_issues_press(&mut self, column: u16, row: u16) {
        if self.model.hit_at(column, row) != Some(WorkspaceHit::OverviewIssuesBody) {
            self.model.set_space_issues(false);
        }
    }
}

/// The list, drawn over everything but the toasts.
pub(super) fn render_space_issues(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overview: &SpaceOverviewView,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    crate::ui::widget::scrim::render(frame, area);
    let popup = crate::ui::widget::alerts::render(
        frame,
        area,
        "To check",
        &overview.issues(),
        dialog::border_hint(
            &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
            &[(Action::Dismiss, "close")],
        ),
    );
    hits.insert(0, (popup, WorkspaceHit::OverviewIssuesBody));
}
