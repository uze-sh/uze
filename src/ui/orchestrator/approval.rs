//! Asking the operator about a project's commands (ADR-055): the toast
//! that says a project's `setup` and `gate` wait for an answer, and the
//! dialog that shows the exact lines and takes it.
//!
//! The answer is the operator's alone. Nothing a pane sends reaches this
//! dialog — it seals the keyboard while it is open — and approving is a
//! click or a keystroke in this client, never a command an agent can type.

use super::*;
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};
use uze_application::{CommandsAwaitingApproval, PolicyStep};

/// The open question: which project's commands, and which answer the
/// keyboard is on.
pub(super) struct ApprovalDialog {
    pub(super) awaiting: CommandsAwaitingApproval,
    pub(super) on_approve: bool,
}

/// What approving answered: the commands are recorded as approved, and
/// what preparing the checkout that waited for them could not do.
pub(super) struct ApprovalResolution {
    pub(super) project: PathBuf,
    pub(super) outcome: std::result::Result<Vec<String>, String>,
}

impl WorkspaceModel {
    /// Keeps `awaiting` as the question to ask about its project and says
    /// so — once per project a session unless `again`, which is a
    /// placement or a delivery that just went without the commands: a
    /// consequence the operator is owed even when they put the question
    /// away before.
    pub(super) fn commands_await(&mut self, awaiting: CommandsAwaitingApproval, again: bool) {
        let project = awaiting.project.clone();
        let known = self.remembered.commands_awaiting.contains_key(&project);
        let changed = awaiting.changed;
        let steps = steps_named(&awaiting);
        // A placement's question names the checkout that went without its
        // setup; a later read of the same project does not take that away.
        let awaiting = match self.remembered.commands_awaiting.get(&project) {
            Some(held) if awaiting.checkout.is_none() && held.checkout.is_some() => {
                awaiting.for_checkout(held.checkout.clone().unwrap_or_default())
            }
            _ => awaiting,
        };
        self.remembered
            .commands_awaiting
            .insert(project.clone(), awaiting);
        if known && !again {
            return;
        }
        let title = if changed {
            "agents.yaml's commands changed"
        } else {
            "agents.yaml asks to run commands"
        };
        self.raise_toast(
            ToastKind::Warned,
            title,
            format!(
                "{}: its {steps} wait for your approval before they run",
                project_name(&project)
            ),
            Some(("Review".to_owned(), WorkspaceHit::ReviewCommands)),
        );
    }

    /// Opens the question about the project that has waited longest.
    pub(super) fn review_commands(&mut self) {
        let Some(awaiting) = self.remembered.commands_awaiting.values().next().cloned() else {
            return;
        };
        self.approval = Some(ApprovalDialog {
            awaiting,
            on_approve: false,
        });
        self.dirty = true;
    }

    /// Takes the answer off the dialog: the commands to approve when it was
    /// yes, nothing when it was not. Declining keeps the question for the
    /// next placement to raise again.
    pub(super) fn answer_approval(&mut self, approve: bool) -> Option<CommandsAwaitingApproval> {
        let dialog = self.approval.take()?;
        self.dirty = true;
        if !approve {
            return None;
        }
        self.remembered
            .commands_awaiting
            .remove(&dialog.awaiting.project);
        if let Some(checkout) = &dialog.awaiting.checkout {
            self.note(format!("preparing {}", checkout.display()));
        }
        Some(dialog.awaiting)
    }

    /// What approving said, as an outcome.
    pub(super) fn absorb_approval(&mut self, resolution: ApprovalResolution) {
        self.clear_busy_notice();
        let project = project_name(&resolution.project);
        match resolution.outcome {
            Ok(warnings) => match warnings.first() {
                None => self.raise_toast(
                    ToastKind::Done,
                    "commands approved",
                    format!("{project}: its commands run from now on"),
                    None,
                ),
                Some(warning) => self.raise_toast(
                    ToastKind::Warned,
                    "commands approved",
                    format!("{project}: {warning}"),
                    None,
                ),
            },
            Err(reason) => self.raise_toast(
                ToastKind::Failed,
                "the approval could not be recorded",
                format!("{project}: {reason}"),
                None,
            ),
        }
    }
}

impl Attach<'_> {
    /// The operator's answer, by click or by key. Yes records the lines
    /// they read, off the frame: it writes a record and may run the
    /// project's setup in the checkout that went without it.
    pub(super) fn answer_approval(&mut self, approve: bool) {
        if let Some(awaiting) = self.model.answer_approval(approve) {
            spawn_command_approval(self.home, awaiting, self.channels.approvals.sender.clone());
        }
    }

    /// A key while the question is open: it answers, moves between the
    /// answers, or does nothing — the dialog seals everything behind it.
    pub(super) fn approval_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                let approve = self
                    .model
                    .approval
                    .as_ref()
                    .is_some_and(|open| open.on_approve);
                self.answer_approval(approve);
            }
            Action::Dismiss => self.answer_approval(false),
            Action::FocusNext | Action::FocusPrevious => {
                if let Some(open) = self.model.approval.as_mut() {
                    open.on_approve = !open.on_approve;
                    self.model.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// A press while the question is open: on an answer it is that answer,
    /// on the question nothing, and anywhere else it puts the question
    /// away unanswered, the way a click outside closes every dialog here.
    pub(super) fn approval_press(&mut self, column: u16, row: u16) {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::ApprovalAnswer(approve)) => self.answer_approval(approve),
            Some(WorkspaceHit::ApprovalBody) => {}
            _ => self.answer_approval(false),
        }
    }
}

/// Which of the lists a question is about, as a sentence names them.
fn steps_named(awaiting: &CommandsAwaitingApproval) -> &'static str {
    let has = |step| awaiting.commands.iter().any(|command| command.step == step);
    match (has(PolicyStep::Setup), has(PolicyStep::Gate)) {
        (true, true) => "setup and gate",
        (true, false) => "setup commands",
        _ => "gate commands",
    }
}

fn project_name(project: &Path) -> String {
    project.file_name().map_or_else(
        || project.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The question, drawn over everything but the toasts: the project, what
/// approving means, and every line exactly as it would run, controls
/// written out.
pub(super) fn render_approval(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ApprovalDialog,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let awaiting = &open.awaiting;
    let mut body = vec![if awaiting.changed {
        "These commands changed since you approved them. They run with your permissions: \
         setup in every checkout the workspace prepares, gate before any work is delivered. \
         Approve only what you would run yourself."
            .to_owned()
    } else {
        "This project's agents.yaml asks uze to run these with your permissions: setup in every \
         checkout the workspace prepares, gate before any work is delivered. Approve only what \
         you would run yourself."
            .to_owned()
    }];
    body.extend(awaiting.commands.iter().map(|command| {
        let step = match command.step {
            PolicyStep::Setup => "setup",
            PolicyStep::Gate => "gate",
        };
        format!("{step}  {}", command.line)
    }));
    crate::ui::widget::scrim::render(frame, area);
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: Tone::Caution,
            title: "Run this project's commands?",
            subject: Some(Line::from(project_name(&awaiting.project))),
            body,
            confirm: Some("Approve"),
            focus: Some(if open.on_approve { 1 } else { CANCEL }),
            field: None,
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::ApprovalAnswer(false),
        WorkspaceHit::ApprovalAnswer(true),
    );
    // Before everything it covers, and the dialog's own body before that:
    // a click on the question is not a click on what is underneath.
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, WorkspaceHit::ApprovalBody)]),
    );
}
