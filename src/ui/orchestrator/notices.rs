//! The header's one line about work in flight, and the toasts that say what happened.

use super::*;

impl WorkspaceModel {
    /// A hint for work that has started and not finished: it keeps a
    /// spinner and stays until the work ends.
    ///
    /// The header's one message, and only ever this. Outcomes — what
    /// worked, what did not, what needs the reader — are toasts: they
    /// arrive whether or not the reader is looking at the strip, they
    /// stack, and the one that failed can carry the offer to try again.
    /// A header that said both had to choose between them, and what it
    /// dropped was whichever arrived second.
    pub(super) fn set_busy_notice(&mut self, text: String) {
        self.note(text);
    }

    /// The work the hint was about has ended. Called where the operation's
    /// own pending flag is cleared rather than where its outcome is said,
    /// because an operation that ends with nothing to say still ends.
    pub(super) fn clear_busy_notice(&mut self) {
        if self.remembered.notice.take().is_some() {
            self.dirty = true;
        }
    }

    /// Same as `set_notice`, but attributed to one task: shown label-free
    /// when that task's tab is the one in front of the operator, since the
    /// tab already says whose agent this is, and labeled when it is not.
    pub(super) fn note(&mut self, text: String) {
        self.remembered.notice = Some(Notice { text });
        self.dirty = true;
    }

    /// Says, once per pane, that a harness is running there without the
    /// workspace's shim: something in the pane's shell put another copy
    /// ahead of it on `PATH`, and what the shim carries (the conversation
    /// resumed after a restart, the project's own skills and agents for a
    /// harness that does not read them) is lost for it.
    pub(super) fn note_launcher_bypass(&mut self) {
        let Some(launchers) = self.remembered.launchers.clone() else {
            return;
        };
        let Some(session) = &self.session else {
            return;
        };
        let bypassed: Vec<(uze_terminal::PaneId, String)> = session
            .workspace
            .spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .map(|tab| &tab.pane)
            .filter(|pane| !pane.through_launcher && launchers.contains(&pane.process))
            .map(|pane| (pane.id, pane.process.clone()))
            .collect();
        for (pane, harness) in bypassed {
            if self.remembered.bypass_reported.insert(pane) {
                self.raise_toast(
                    ToastKind::Warned,
                    format!("{harness} started without the workspace's launcher"),
                    "something in this pane's shell put another copy first on PATH, so its \
                     conversation will not resume after a restart and the project's own skills \
                     may not reach it"
                        .to_owned(),
                    None,
                );
            }
        }
    }

    /// Raises an outcome for the reader. It leaves on its own clock unless
    /// `offer` is something to answer, in which case it stays until it is
    /// answered or put away — a message with a button that vanished while
    /// the reader reached for it is worse than no button.
    pub(super) fn raise_toast(
        &mut self,
        kind: crate::ui::widget::ToastKind,
        text: impl Into<String>,
        detail: impl Into<String>,
        offer: Option<(String, WorkspaceHit)>,
    ) {
        // Oldest first out: the reader is looking at the top of the stack,
        // and a queue that dropped the newest would hide exactly what just
        // happened.
        while self.remembered.toasts.len() >= MAX_TOASTS {
            self.remembered.toasts.pop_front();
        }
        self.remembered.toasts.push_back(RaisedToast {
            kind,
            text: text.into(),
            detail: detail.into(),
            raised: Instant::now(),
            stays: offer.is_some(),
            offer,
        });
        self.dirty = true;
    }

    /// Puts one away by its place in the stack **as drawn**, which is the
    /// only index a click can carry.
    ///
    /// The stack is drawn newest-first and the queue holds them
    /// oldest-first, so the two count in opposite directions: taking the
    /// click's index straight to the queue dismissed the toast at the
    /// other end of the column from the one that was pressed.
    pub(super) fn dismiss_toast(&mut self, index: usize) {
        let Some(at) = self.remembered.toasts.len().checked_sub(index + 1) else {
            return;
        };
        self.remembered.toasts.remove(at);
        self.dirty = true;
    }

    /// What one toast's offer answers with, by its place in the stack.
    pub(super) fn toast_offer(&self, index: usize) -> Option<WorkspaceHit> {
        self.remembered
            .toasts
            .iter()
            .rev()
            .nth(index)
            .and_then(|toast| toast.offer.as_ref())
            .map(|(_, hit)| *hit)
    }

    /// Whether any outcome is still counting down, which is what keeps the
    /// frame redrawing while the seconds it shows are changing.
    pub(super) fn toasts_are_counting(&self) -> bool {
        self.remembered.toasts.iter().any(|toast| !toast.stays)
    }

    /// Drops the ones whose clock ran out. Answers whether anything left,
    /// so the caller can mark the frame dirty exactly when it changed.
    pub(super) fn retire_toasts(&mut self) -> bool {
        let before = self.remembered.toasts.len();
        self.remembered
            .toasts
            .retain(|toast| toast.stays || toast.raised.elapsed() < TOAST_TTL);
        before != self.remembered.toasts.len()
    }

    /// The stack as the frame draws it, newest at the top, each with the
    /// seconds it has left.
    pub(in crate::ui) fn toast_stack(&self) -> Vec<crate::ui::widget::Toast> {
        self.remembered
            .toasts
            .iter()
            .rev()
            .map(|raised| {
                let remaining = (!raised.stays).then(|| {
                    TOAST_TTL
                        .saturating_sub(raised.raised.elapsed())
                        .as_secs()
                        .saturating_add(1)
                        .min(TOAST_TTL.as_secs())
                });
                let mut toast = crate::ui::widget::Toast::new(
                    raised.kind,
                    raised.text.clone(),
                    raised.detail.clone(),
                )
                .remaining(remaining);
                if let Some((label, _)) = &raised.offer {
                    toast = toast.action(label.clone());
                }
                toast
            })
            .collect()
    }

    /// Whether something the workspace is showing a notice for is still
    /// running — what keeps the spinner's clock turning (see
    /// `workspace_has_active_agent_operation`).
    pub(super) fn notice_is_busy(&self) -> bool {
        self.remembered.notice.is_some()
    }

    /// The active notice as the header's message zone draws it, or nothing
    /// when there is none. One surface: a message about the task on
    /// screen, one about a task that is not, and a workspace-wide one all
    /// land here, the middle one carrying the label that names it.
    pub(in crate::ui) fn notice_chip(&self) -> Option<NoticeChip> {
        let notice = self.remembered.notice.as_ref()?;
        Some(NoticeChip {
            text: text::elide(&notice.text, NOTICE_WIDTH),
            busy: true,
        })
    }
}
