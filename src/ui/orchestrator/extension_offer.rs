//! Offering a project's extensions where it opens (`extension-consent`,
//! "Enabling is per project"): the toast that says an extension a plugin
//! the project declares runs nothing there yet, and the dialog that shows
//! its grants and takes the answer, as a project's commands are offered
//! (ADR-055).
//!
//! The answer is the operator's alone: the dialog seals the keyboard while
//! it is open, and putting it away is remembered, so the offer is not made
//! again on the next opening. The Extensions screen still makes it.

use super::*;
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};
use uze_application::{ExtensionCondition, ExtensionReport};

/// The open question: which extension, in which project, and which answer
/// the keyboard is on.
pub(super) struct ExtensionOfferDialog {
    pub(super) project: PathBuf,
    /// The project as every surface names it, read when the dialog opens.
    pub(super) named: String,
    pub(super) offer: ExtensionReport,
    pub(super) on_allow: bool,
}

/// What answering did: enabled, or the offer put away, or why neither.
pub(super) struct ExtensionOfferResolution {
    pub(super) project: PathBuf,
    pub(super) attribution: String,
    pub(super) name: String,
    pub(super) allowed: bool,
    pub(super) outcome: std::result::Result<(), String>,
}

impl WorkspaceModel {
    /// Keeps what `project` offers and says so, once a session per
    /// project: every extension a plugin it declares carries that waits
    /// for the operator there.
    pub(super) fn extensions_offered(&mut self, project: PathBuf, offers: Vec<ExtensionReport>) {
        if offers.is_empty() || self.remembered.extension_offers.contains_key(&project) {
            return;
        }
        let detail = match offers.as_slice() {
            [one] => format!(
                "{}: {} runs nothing there yet",
                self.project_named(&project),
                one.name
            ),
            many => format!(
                "{}: {} extensions run nothing there yet",
                self.project_named(&project),
                many.len()
            ),
        };
        self.remembered.extension_offers.insert(project, offers);
        self.raise_toast(
            ToastKind::Told,
            "extension to enable",
            detail,
            Some(("Review".to_owned(), WorkspaceHit::ReviewExtensions)),
        );
    }

    /// Opens the question about the first extension still offered.
    pub(super) fn review_extensions(&mut self) {
        let Some((project, offer)) = self
            .remembered
            .extension_offers
            .iter()
            .find_map(|(project, offers)| Some((project.clone(), offers.first()?.clone())))
        else {
            return;
        };
        self.extension_offer = Some(ExtensionOfferDialog {
            named: self.project_named(&project),
            project,
            offer,
            on_allow: false,
        });
        self.dirty = true;
    }

    /// Takes the answer off the dialog: the extension leaves the offers
    /// whichever way it went, since either answer is the operator's.
    pub(super) fn answer_extension_offer(
        &mut self,
        allow: bool,
    ) -> Option<(PathBuf, ExtensionReport, bool)> {
        let dialog = self.extension_offer.take()?;
        self.dirty = true;
        if let Some(offers) = self.remembered.extension_offers.get_mut(&dialog.project) {
            offers.retain(|offer| offer.identity() != dialog.offer.identity());
        }
        Some((dialog.project, dialog.offer, allow))
    }

    /// What answering said, as an outcome; the next extension offered,
    /// if one waits, is asked about straight after.
    pub(super) fn absorb_extension_offer(&mut self, resolution: ExtensionOfferResolution) {
        let project = self.project_named(&resolution.project);
        match (resolution.outcome, resolution.allowed) {
            (Ok(()), true) => self.raise_toast(
                ToastKind::Done,
                "extension enabled",
                format!("{} · {project}", resolution.name),
                None,
            ),
            (Ok(()), false) => {}
            (Err(reason), _) => self.raise_toast(
                ToastKind::Failed,
                "extension not enabled",
                format!("{}: {reason}", resolution.attribution),
                None,
            ),
        }
        if self.extension_offer.is_none()
            && self
                .remembered
                .extension_offers
                .get(&resolution.project)
                .is_some_and(|offers| !offers.is_empty())
        {
            self.review_extensions();
        }
    }
}

impl Attach<'_> {
    /// The operator's answer, by click or by key: allowing consents when
    /// it must and enables the extension in the project; putting it away
    /// is remembered there. Both off the frame: each writes a record.
    pub(super) fn answer_extension_offer(&mut self, allow: bool) {
        if let Some((project, offer, allow)) = self.model.answer_extension_offer(allow) {
            spawn_extension_offer_answer(
                self.home,
                project,
                offer,
                allow,
                self.asker,
                self.channels.extension_offer_answers.sender.clone(),
            );
        }
    }

    /// A key while the question is open: it answers, moves between the
    /// answers, or does nothing — the dialog seals everything behind it.
    pub(super) fn extension_offer_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                let allow = self
                    .model
                    .extension_offer
                    .as_ref()
                    .is_some_and(|open| open.on_allow);
                self.answer_extension_offer(allow);
            }
            Action::Dismiss => self.answer_extension_offer(false),
            Action::FocusNext | Action::FocusPrevious => {
                if let Some(open) = self.model.extension_offer.as_mut() {
                    open.on_allow = !open.on_allow;
                    self.model.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// A press while the question is open: on an answer it is that answer,
    /// on the question nothing, and anywhere else it puts the question
    /// away, the way a click outside closes every dialog here.
    pub(super) fn extension_offer_press(&mut self, column: u16, row: u16) {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::ExtensionOfferAnswer(allow)) => self.answer_extension_offer(allow),
            Some(WorkspaceHit::ExtensionOfferBody) => {}
            _ => self.answer_extension_offer(false),
        }
    }
}

/// The question, drawn over everything but the toasts: whose extension,
/// in which project, and every grant it wants with what it lets it do.
pub(super) fn render_extension_offer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ExtensionOfferDialog,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let offer = &open.offer;
    let consent = offer.condition != ExtensionCondition::NeedsEnabling;
    let mut body = vec![match offer.condition {
        ExtensionCondition::Held => {
            "Its update wants more than you allowed. Nothing of it runs until you allow it:"
                .to_owned()
        }
        ExtensionCondition::NeedsEnabling => format!(
            "You allowed it on this machine. Enabled here, it runs in {} only, with:",
            open.named
        ),
        _ => format!(
            "A plugin this project uses brings it. It runs a script inside the workspace, in {} \
             only, with:",
            open.named
        ),
    }];
    let grants = if offer.condition == ExtensionCondition::Held {
        &offer.new_grants
    } else {
        &offer.grants
    };
    if grants.is_empty() {
        body.push("nothing beyond drawing what its script answers".to_owned());
    }
    body.extend(
        grants
            .iter()
            .map(|grant| format!("{grant} · {}", uze_application::grant_meaning(grant))),
    );
    crate::ui::widget::scrim::render(frame, area);
    let title = format!("Enable {} in {}?", offer.name, open.named);
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: Tone::Caution,
            title: &title,
            subject: Some(Line::from(offer.attribution())),
            body,
            confirm: Some(if consent {
                "Allow and enable"
            } else {
                "Enable"
            }),
            focus: Some(if open.on_allow { 1 } else { CANCEL }),
            field: None,
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::ExtensionOfferAnswer(false),
        WorkspaceHit::ExtensionOfferAnswer(true),
    );
    // Before everything it covers, and the dialog's own body before that:
    // a click on the question is not a click on what is underneath.
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, WorkspaceHit::ExtensionOfferBody)]),
    );
}
