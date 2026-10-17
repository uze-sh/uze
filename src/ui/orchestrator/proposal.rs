//! Asking the operator about a machine fact an agent proposed: the toast
//! that says one waits, and the dialog that shows it and takes the answer.
//!
//! A machine fact reaches every agent in every project, so one an agent
//! wrote is held as a proposal until the operator reads it here. Like the
//! approval of a project's commands, the answer is a click or a keystroke
//! in this client — never something a pane can type.

use super::*;
use crate::ui::widget::dialog::{self, ALTERNATIVE, CANCEL, Dialog, Tone};
use uze_application::application::memory::Entry;

/// Which answer a proposal got.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProposalChoice {
    /// Put it away for now; it stays proposed.
    Leave,
    Discard,
    Accept,
}

/// The open question, and which answer the keyboard is on.
pub(super) struct ProposalDialog {
    pub(super) fact: Entry,
    pub(super) focus: usize,
}

/// What answering did.
pub(super) struct ProposalAnswerResolution {
    pub(super) name: String,
    pub(super) choice: ProposalChoice,
    pub(super) outcome: std::result::Result<(), String>,
}

/// The proposals the latest read found.
pub(super) struct ProposalsResolution {
    pub(super) proposals: Vec<Entry>,
}

impl WorkspaceModel {
    /// Keeps the proposals the read found and says, once a session for
    /// each, that one waits.
    pub(super) fn absorb_proposals(&mut self, resolution: ProposalsResolution) {
        self.remembered.memory_proposals_pending = false;
        for fact in &resolution.proposals {
            if self
                .remembered
                .memory_proposals_told
                .insert(fact.name.clone())
            {
                self.raise_toast(
                    ToastKind::Warned,
                    "machine fact proposed",
                    format!("{}: {}", fact.name, fact.description),
                    Some(("Review".to_owned(), WorkspaceHit::ReviewProposal)),
                );
            }
        }
        self.remembered.memory_proposals = resolution.proposals;
        self.dirty = true;
    }

    /// Opens the question about the proposal that has waited longest.
    pub(super) fn review_proposal(&mut self) {
        let Some(fact) = self.remembered.memory_proposals.first().cloned() else {
            return;
        };
        // On the way out, like every question whose yes reaches past this
        // client: an Enter pressed without reading accepts nothing.
        self.proposal = Some(ProposalDialog {
            fact,
            focus: CANCEL,
        });
        self.dirty = true;
    }

    /// What answering said, as an outcome.
    pub(super) fn absorb_proposal_answer(&mut self, resolution: ProposalAnswerResolution) {
        self.remembered
            .memory_proposals
            .retain(|fact| fact.name != resolution.name);
        let (kind, title) = match (&resolution.outcome, resolution.choice) {
            (Err(_), _) => (ToastKind::Failed, "fact not changed"),
            (Ok(()), ProposalChoice::Accept) => (ToastKind::Done, "machine fact kept"),
            (Ok(()), _) => (ToastKind::Done, "proposed fact discarded"),
        };
        let detail = match resolution.outcome {
            Ok(()) => resolution.name,
            Err(reason) => format!("{}: {reason}", resolution.name),
        };
        self.raise_toast(kind, title, detail, None);
    }
}

impl Attach<'_> {
    /// The operator's answer. Accepting moves the fact into the machine's
    /// memory and discarding deletes it, off the frame; leaving it does
    /// nothing, and the next session asks again.
    pub(super) fn answer_proposal(&mut self, choice: ProposalChoice) {
        let Some(open) = self.model.proposal.take() else {
            return;
        };
        self.model.dirty = true;
        if choice == ProposalChoice::Leave {
            return;
        }
        spawn_proposal_answer(
            self.home,
            open.fact.name,
            choice,
            self.channels.proposal_answers.sender.clone(),
        );
    }

    pub(super) fn proposal_action(&mut self, action: Action) {
        let focused = |focus: usize| match focus {
            CANCEL => ProposalChoice::Leave,
            ALTERNATIVE => ProposalChoice::Discard,
            _ => ProposalChoice::Accept,
        };
        match action {
            Action::Activate => {
                let choice = self
                    .model
                    .proposal
                    .as_ref()
                    .map_or(ProposalChoice::Leave, |open| focused(open.focus));
                self.answer_proposal(choice);
            }
            Action::Dismiss => self.answer_proposal(ProposalChoice::Leave),
            Action::FocusNext | Action::FocusPrevious => {
                if let Some(open) = self.model.proposal.as_mut() {
                    open.focus = if action == Action::FocusNext {
                        (open.focus + 1) % 3
                    } else {
                        (open.focus + 2) % 3
                    };
                    self.model.dirty = true;
                }
            }
            _ => {}
        }
    }

    pub(super) fn proposal_press(&mut self, column: u16, row: u16) {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::ProposalAnswer(choice)) => self.answer_proposal(choice),
            Some(WorkspaceHit::ProposalBody) => {}
            _ => self.answer_proposal(ProposalChoice::Leave),
        }
    }
}

/// The question, drawn over everything but the toasts.
pub(super) fn render_proposal(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ProposalDialog,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let fact = &open.fact;
    let by = fact
        .source
        .as_ref()
        .map_or_else(|| "an agent".to_owned(), |source| source.describe());
    crate::ui::widget::scrim::render(frame, area);
    let answers = dialog::render_choice(
        frame,
        area,
        &Dialog {
            tone: Tone::Caution,
            title: "Keep this fact for every project?",
            subject: Some(ratatui::text::Line::from(fact.name.clone())),
            body: vec![
                fact.description.clone(),
                format!(
                    "Proposed by {by}. Accepted, every agent in every project is given it; \
                     discarded, it is deleted. It is kept at {}.",
                    fact.path.display()
                ),
            ],
            confirm: Some("Accept"),
            focus: Some(open.focus),
            field: None,
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::ProposalAnswer(ProposalChoice::Leave),
        (
            "Discard",
            WorkspaceHit::ProposalAnswer(ProposalChoice::Discard),
        ),
        WorkspaceHit::ProposalAnswer(ProposalChoice::Accept),
    );
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, WorkspaceHit::ProposalBody)]),
    );
}
