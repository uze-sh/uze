//! What the space overview asks before it takes something away: a
//! worktree's directory, or a space's prompts.
//!
//! One question at a time, as a dialog over the whole client, answered by
//! its buttons, by the keys, or withdrawn by a click anywhere else. What
//! each answer does lives with the thing it takes away; this is only the
//! asking.

use super::*;
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};

/// The open question, and which answer the keys are on.
pub(super) struct OverviewQuestion {
    pub(super) asked: Asked,
    pub(super) on_confirm: bool,
}

/// What is about to be taken away.
pub(super) enum Asked {
    RemoveWorktree(WorktreeRemoval),
    /// The prompts asked in the space rooted at `root`.
    ClearPrompts {
        root: PathBuf,
    },
}

impl OverviewQuestion {
    /// `asked`, with the keys starting on the way out.
    pub(super) fn new(asked: Asked) -> Self {
        Self {
            asked,
            on_confirm: false,
        }
    }
}

impl WorkspaceModel {
    pub(super) fn asks_overview_question(&self) -> bool {
        self.space_overview
            .as_ref()
            .is_some_and(|overview| overview.question.is_some())
    }
}

impl Attach<'_> {
    /// Asks `asked` over the overview.
    pub(super) fn ask_overview_question(&mut self, asked: Asked) {
        if let Some(overview) = self.model.space_overview.as_mut() {
            overview.question = Some(OverviewQuestion::new(asked));
            self.model.dirty = true;
        }
    }

    /// The operator's answer, by click or by key: yes takes the thing
    /// away; no leaves it.
    pub(super) fn answer_overview_question(&mut self, confirm: bool) {
        let Some(question) = self
            .model
            .space_overview
            .as_mut()
            .and_then(|overview| overview.question.take())
        else {
            return;
        };
        self.model.dirty = true;
        if !confirm {
            return;
        }
        match question.asked {
            Asked::RemoveWorktree(removal) => self.remove_worktree(removal),
            Asked::ClearPrompts { root } => {
                spawn_clear_prompt_history(self.home, root, self.channels.prompts.sender.clone());
            }
        }
    }

    /// A key while the question is open: it answers, moves between the
    /// answers, or does nothing — the dialog seals everything behind it.
    pub(super) fn overview_question_action(&mut self, action: Action) {
        let Some(question) = self
            .model
            .space_overview
            .as_mut()
            .and_then(|overview| overview.question.as_mut())
        else {
            return;
        };
        match action {
            Action::Activate => {
                let confirm = question.on_confirm;
                self.answer_overview_question(confirm);
            }
            Action::Dismiss => self.answer_overview_question(false),
            Action::FocusNext | Action::FocusPrevious => {
                question.on_confirm = !question.on_confirm;
                self.model.dirty = true;
            }
            _ => {}
        }
    }

    /// A press while the question is open: on an answer it is that answer,
    /// on the question nothing, and anywhere else it withdraws the question.
    pub(super) fn overview_question_press(&mut self, column: u16, row: u16) {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::OverviewAnswer(confirm)) => self.answer_overview_question(confirm),
            Some(WorkspaceHit::OverviewQuestionBody) => {}
            _ => self.answer_overview_question(false),
        }
    }
}

/// The question, drawn over everything but the toasts.
pub(super) fn render_overview_question(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    question: &OverviewQuestion,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let subject_style = theme::fg(Token::TextCode);
    let (title, subject, body, confirm) = match &question.asked {
        Asked::RemoveWorktree(removal) => (
            "Remove this worktree?",
            format!("{} ({})", removal.name, bytes_to_say(removal.bytes)),
            "Its directory is deleted, build output included. Its branch stays, and so does \
             every commit on it.",
            "Remove",
        ),
        Asked::ClearPrompts { root } => (
            "Clear this space's prompts?",
            root.display().to_string(),
            "Every prompt asked in this space is forgotten. Agents and their work are not \
             touched.",
            "Clear",
        ),
    };
    crate::ui::widget::scrim::render(frame, area);
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: Tone::Danger,
            title,
            subject: Some(Line::from(Span::styled(subject, subject_style))),
            body: vec![body.to_owned()],
            confirm: Some(confirm),
            focus: Some(if question.on_confirm { 1 } else { CANCEL }),
            field: None,
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::OverviewAnswer(false),
        WorkspaceHit::OverviewAnswer(true),
    );
    // Before everything it covers, and the dialog's own body before that:
    // a click on the question is not a click on what is underneath.
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, WorkspaceHit::OverviewQuestionBody)]),
    );
}
