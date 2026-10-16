//! Asking before an agent mid-turn is continued on another harness: the
//! relaunch ends its process, and with it whatever the turn was doing. A
//! dialog over the whole client rather than a row of the menu, because an
//! answer here costs work in flight, and is asked the way the client asks
//! every question of that weight.

use super::*;
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};

/// The open question: which agent, which harness it would continue in, and
/// which answer the keyboard is on.
pub(super) struct InterruptDialog {
    pub(super) target: MenuTarget,
    pub(super) label: String,
    pub(super) choice: HarnessChoice,
    pub(super) on_confirm: bool,
}

impl Attach<'_> {
    /// The operator's answer, by click or by key: yes continues the agent
    /// on the harness the menu named; no leaves it running.
    pub(super) fn answer_interrupt(&mut self, confirm: bool) {
        let Some(open) = self.model.interrupt.take() else {
            return;
        };
        self.model.dirty = true;
        if confirm {
            self.continue_in(open.target, open.choice, true);
        }
    }

    /// A key while the question is open: it answers, moves between the
    /// answers, or does nothing — the dialog seals everything behind it.
    pub(super) fn interrupt_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                let confirm = self
                    .model
                    .interrupt
                    .as_ref()
                    .is_some_and(|open| open.on_confirm);
                self.answer_interrupt(confirm);
            }
            Action::Dismiss => self.answer_interrupt(false),
            Action::FocusNext | Action::FocusPrevious => {
                if let Some(open) = self.model.interrupt.as_mut() {
                    open.on_confirm = !open.on_confirm;
                    self.model.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// A press while the question is open: on an answer it is that answer,
    /// on the question nothing, and anywhere else it puts the question
    /// away unanswered.
    pub(super) fn interrupt_press(&mut self, column: u16, row: u16) {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::InterruptAnswer(confirm)) => self.answer_interrupt(confirm),
            Some(WorkspaceHit::InterruptBody) => {}
            _ => self.answer_interrupt(false),
        }
    }
}

/// The question, drawn over everything but the toasts.
pub(super) fn render_interrupt(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &InterruptDialog,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let title = format!("Interrupt {}?", open.label);
    let confirm = format!("Continue in {}", open.choice.display_name);
    crate::ui::widget::scrim::render(frame, area);
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: Tone::Caution,
            title: &title,
            subject: None,
            body: vec![format!(
                "It is in the middle of a turn. Continuing it in {} ends what it is doing now; \
                 its conversation is kept, and the new harness is briefed on it.",
                open.choice.display_name
            )],
            confirm: Some(&confirm),
            focus: Some(if open.on_confirm { 1 } else { CANCEL }),
            field: None,
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::InterruptAnswer(false),
        WorkspaceHit::InterruptAnswer(true),
    );
    // Before everything it covers, and the dialog's own body before that:
    // a click on the question is not a click on what is underneath.
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, WorkspaceHit::InterruptBody)]),
    );
}
