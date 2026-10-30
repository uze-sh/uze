//! Offering a project's extensions where it opens (`extension-consent`,
//! "Enabling is per project"): the toast that says what a plugin the
//! project declares brings and runs nothing there yet, and the one dialog
//! that lists them all to choose from, as a project's commands are offered
//! (ADR-055).
//!
//! The answer is the operator's alone: the dialog seals the keyboard while
//! it is open. Putting it away answers nothing ("not now"): the offer is
//! made again the next time the project opens. A row left off is the same.
//! Programs are asked about on a step of their own, once for every row
//! chosen, because agreeing to "reads the checkout" must never also agree
//! to a program running (`extension-programs-and-confinement` D22).

use std::cell::Cell;

use super::*;
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};
use crate::ui::widget::{Button, Scrollbar, button_row, text};
use uze_application::{ExtensionCondition, ExtensionReport};

/// The rows the list shows at least and at most: below the least the
/// dialog cannot be read, and past the most it scrolls, as every list here
/// does.
const LIST_ROWS: (u16, u16) = (4, 14);
/// The rows the marked extension's detail keeps whatever it holds, so the
/// dialog never changes height as the cursor moves: its heading, then what
/// it asks for. Fewer on a short terminal, never fewer than the least.
const DETAIL_ROWS: (u16, u16) = (3, 6);
/// The narrowest and widest the list is drawn: wider than a question,
/// because a row carries a name, a plugin and what it asks.
const WIDTH: (u16, u16) = (60, 96);
/// The column a grant's name takes in the detail, its meaning beside it.
const GRANT_COLUMN: usize = 20;

/// Which part of the offer the dialog is on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OfferStep {
    /// The list to choose from.
    List,
    /// Everything the marked row asks for, in the list's place, scrolled
    /// from `first`.
    Whole { first: usize },
    /// The programs of every row chosen that runs any, before anything is
    /// recorded.
    Programs,
}

/// The open question: the project, every extension it offers, which are
/// chosen, where the answer holds, and which answer the keyboard is on.
pub(super) struct ExtensionOfferDialog {
    pub(super) project: PathBuf,
    /// The project as every surface names it, read when the dialog opens.
    pub(super) named: String,
    pub(super) offers: Vec<ExtensionReport>,
    pub(super) chosen: Vec<bool>,
    pub(super) cursor: usize,
    /// The first row the list shows: moved by the wheel without moving the
    /// cursor, and by the cursor when it would leave the window.
    pub(super) first: usize,
    /// How many rows the last frame showed, which keeping the cursor in
    /// view measures by.
    shown: Cell<usize>,
    /// How many rows the last frame gave the detail.
    detail: Cell<u16>,
    pub(super) everywhere: bool,
    pub(super) on_allow: bool,
    pub(super) step: OfferStep,
}

impl ExtensionOfferDialog {
    /// Every row chosen but an update that wants more than was allowed:
    /// widening what an extension reaches is never the default.
    fn new(project: PathBuf, named: String, offers: Vec<ExtensionReport>) -> Self {
        let chosen = offers
            .iter()
            .map(|offer| offer.condition != ExtensionCondition::Held)
            .collect();
        Self {
            project,
            named,
            offers,
            chosen,
            cursor: 0,
            first: 0,
            shown: Cell::new(usize::from(LIST_ROWS.1)),
            detail: Cell::new(DETAIL_ROWS.1),
            everywhere: false,
            on_allow: true,
            step: OfferStep::List,
        }
    }

    fn chosen_offers(&self) -> Vec<ExtensionReport> {
        self.offers
            .iter()
            .zip(&self.chosen)
            .filter(|(_, chosen)| **chosen)
            .map(|(offer, _)| offer.clone())
            .collect()
    }

    fn chosen_count(&self) -> usize {
        self.chosen.iter().filter(|chosen| **chosen).count()
    }

    /// The rows chosen whose programs nobody accepted yet.
    fn owing_programs(&self) -> Vec<usize> {
        (0..self.offers.len())
            .filter(|&index| self.chosen[index] && owes_programs(&self.offers[index]))
            .collect()
    }

    pub(super) fn move_cursor(&mut self, down: bool) {
        let last = self.offers.len().saturating_sub(1);
        self.cursor = if down {
            (self.cursor + 1).min(last)
        } else {
            self.cursor.saturating_sub(1)
        };
        let shown = self.shown.get().max(1);
        if self.cursor < self.first {
            self.first = self.cursor;
        } else if self.cursor >= self.first + shown {
            self.first = self.cursor + 1 - shown;
        }
    }

    pub(super) fn toggle(&mut self, index: usize) {
        if let Some(chosen) = self.chosen.get_mut(index) {
            *chosen = !*chosen;
        }
    }

    fn toggle_all(&mut self) {
        let all = self.chosen.iter().all(|chosen| *chosen);
        self.chosen.iter_mut().for_each(|chosen| *chosen = !all);
    }

    /// The wheel over the list: the window moves, the cursor stays.
    fn scroll(&mut self, down: bool) {
        match &mut self.step {
            OfferStep::List => {
                let last = self.offers.len().saturating_sub(self.shown.get());
                self.first = if down {
                    (self.first + 1).min(last)
                } else {
                    self.first.saturating_sub(1)
                };
            }
            OfferStep::Whole { first } => {
                *first = if down {
                    *first + 1
                } else {
                    first.saturating_sub(1)
                };
            }
            OfferStep::Programs => {}
        }
    }
}

/// Whether allowing `offer` asks for consent rather than only enabling.
fn consents(offer: &ExtensionReport) -> bool {
    offer.condition != ExtensionCondition::NeedsEnabling
}

/// Whether allowing `offer` owes the programs' step.
fn owes_programs(offer: &ExtensionReport) -> bool {
    consents(offer) && !offer.programs.is_empty()
}

/// What an answer asks for: the extensions to enable, and where.
pub(super) struct OfferAnswer {
    pub(super) project: PathBuf,
    pub(super) enable: Vec<ExtensionReport>,
    pub(super) everywhere: bool,
}

/// What answering did, extension by extension.
pub(super) struct ExtensionOfferResolution {
    pub(super) project: PathBuf,
    pub(super) everywhere: bool,
    /// The names enabled where the answer said.
    pub(super) enabled: Vec<String>,
    /// The names enabled in the project alone although every project was
    /// asked for: they reach agents.
    pub(super) kept_here: Vec<String>,
    /// Each one not enabled, by attribution, and why.
    pub(super) failed: Vec<(String, String)>,
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

    /// Whether an extension is still offered somewhere, for the index.
    pub(super) fn offers_waiting(&self) -> bool {
        self.remembered
            .extension_offers
            .values()
            .any(|offers| !offers.is_empty())
    }

    /// Opens the question about everything a project still offers.
    pub(super) fn review_extensions(&mut self) {
        let Some((project, offers)) = self
            .remembered
            .extension_offers
            .iter()
            .find(|(_, offers)| !offers.is_empty())
            .map(|(project, offers)| (project.clone(), offers.clone()))
        else {
            return;
        };
        let named = self.project_named(&project);
        self.extension_offer = Some(ExtensionOfferDialog::new(project, named, offers));
        self.dirty = true;
    }

    /// Whether the open offer is choosing rows, which its own keys answer,
    /// rather than asking about programs, which a question's keys do.
    pub(super) fn extension_offer_chooses(&self) -> bool {
        self.extension_offer
            .as_ref()
            .is_some_and(|open| open.step != OfferStep::Programs)
    }

    /// Takes an answer off the dialog. Allowing the list moves on to the
    /// programs when a row chosen runs any; allowing them, or a list that
    /// owes none, answers. Putting the list away answers nothing, and the
    /// project offers it all again the next time it opens; putting the
    /// programs away leaves their rows off and enables the rest.
    pub(super) fn answer_extension_offer(&mut self, allow: bool) -> Option<OfferAnswer> {
        self.dirty = true;
        let open = self.extension_offer.as_mut()?;
        match (open.step, allow) {
            (OfferStep::Whole { .. }, false) => {
                open.step = OfferStep::List;
                return None;
            }
            (OfferStep::List | OfferStep::Whole { .. }, true) => {
                if open.chosen_count() == 0 {
                    return None;
                }
                if !open.owing_programs().is_empty() {
                    open.step = OfferStep::Programs;
                    open.on_allow = false;
                    return None;
                }
            }
            (OfferStep::Programs, false) => {
                for index in open.owing_programs() {
                    open.chosen[index] = false;
                }
            }
            (OfferStep::List, false) | (OfferStep::Programs, true) => {}
        }
        let dialog = self.extension_offer.take()?;
        // Answered for this session either way; what was left off waits
        // for the next opening.
        if let Some(offers) = self.remembered.extension_offers.get_mut(&dialog.project) {
            offers.clear();
        }
        let enable = if allow || dialog.step == OfferStep::Programs {
            dialog.chosen_offers()
        } else {
            Vec::new()
        };
        (!enable.is_empty()).then_some(OfferAnswer {
            project: dialog.project,
            enable,
            everywhere: dialog.everywhere,
        })
    }

    /// What answering said, in one toast for what was enabled and one for
    /// what was not.
    pub(super) fn absorb_extension_offer(&mut self, resolution: ExtensionOfferResolution) {
        let project = self.project_named(&resolution.project);
        let enabled = resolution.enabled.len() + resolution.kept_here.len();
        if enabled > 0 {
            let names: Vec<&String> = resolution
                .enabled
                .iter()
                .chain(&resolution.kept_here)
                .collect();
            let place = if resolution.everywhere && resolution.kept_here.is_empty() {
                "every project".to_owned()
            } else if resolution.everywhere {
                format!(
                    "every project, {} in {project} alone",
                    listed(&resolution.kept_here)
                )
            } else {
                project.clone()
            };
            let (title, detail) = match names.as_slice() {
                [one] => ("extension enabled".to_owned(), format!("{one} · {place}")),
                _ => (
                    format!("{enabled} extensions enabled"),
                    format!("{place} · {}", listed(&names)),
                ),
            };
            self.raise_toast(ToastKind::Done, title, detail, None);
        }
        if let Some((attribution, reason)) = resolution.failed.first() {
            let title = match resolution.failed.len() {
                1 => "extension not enabled".to_owned(),
                n => format!("{n} extensions not enabled"),
            };
            self.raise_toast(
                ToastKind::Failed,
                title,
                format!("{attribution}: {reason}"),
                None,
            );
        }
    }
}

/// `a, b, c and 2 more`: names a toast can hold.
fn listed(names: &[impl AsRef<str>]) -> String {
    const NAMED: usize = 3;
    let shown: Vec<&str> = names.iter().take(NAMED).map(AsRef::as_ref).collect();
    match names.len().saturating_sub(NAMED) {
        0 => shown.join(", "),
        more => format!("{} and {more} more", shown.join(", ")),
    }
}

impl Attach<'_> {
    /// The operator's answer, by click or by key: each extension chosen is
    /// consented to when it must be and enabled where the answer said,
    /// off the frame, since each writes a record.
    pub(super) fn answer_extension_offer(&mut self, allow: bool) {
        if let Some(answer) = self.model.answer_extension_offer(allow) {
            spawn_extension_offer_answer(
                self.home,
                answer,
                self.asker,
                self.channels.extension_offer_answers.sender.clone(),
            );
        }
    }

    /// A key while the question is open: it moves, switches, answers, or
    /// does nothing — the dialog seals everything behind it.
    pub(super) fn extension_offer_action(&mut self, action: Action) {
        let Some(open) = self.model.extension_offer.as_mut() else {
            return;
        };
        self.model.dirty = true;
        match (open.step, action) {
            (_, Action::Activate) => {
                let allow = open.on_allow;
                self.answer_extension_offer(allow);
            }
            (_, Action::Dismiss) => self.answer_extension_offer(false),
            (OfferStep::Programs, Action::FocusNext | Action::FocusPrevious)
            | (OfferStep::List | OfferStep::Whole { .. }, Action::FocusNext) => {
                open.on_allow = !open.on_allow;
            }
            (OfferStep::List, Action::SelectNext | Action::SelectPrevious) => {
                open.move_cursor(action == Action::SelectNext);
            }
            (OfferStep::Whole { .. }, Action::SelectNext | Action::SelectPrevious) => {
                open.scroll(action == Action::SelectNext);
            }
            (OfferStep::List, Action::ToggleChoice) => open.toggle(open.cursor),
            (OfferStep::List, Action::ToggleAllChoices) => open.toggle_all(),
            (
                OfferStep::List | OfferStep::Whole { .. },
                Action::NextValue | Action::PreviousValue,
            ) => {
                open.everywhere = !open.everywhere;
            }
            (OfferStep::List, Action::Expand) => open.step = OfferStep::Whole { first: 0 },
            (OfferStep::Whole { .. }, Action::Expand) => open.step = OfferStep::List,
            _ => {}
        }
    }

    /// A press while the question is open: on an answer it is that answer,
    /// on a row's mark it switches the row, on a row it marks it, and
    /// anywhere outside the dialog it puts the question away, the way a
    /// click outside closes every dialog here.
    pub(super) fn extension_offer_press(&mut self, column: u16, row: u16) {
        let hit = self.model.hit_at(column, row);
        let Some(open) = self.model.extension_offer.as_mut() else {
            return;
        };
        self.model.dirty = true;
        match hit {
            Some(WorkspaceHit::ExtensionOfferAnswer(allow)) => self.answer_extension_offer(allow),
            Some(WorkspaceHit::ExtensionOfferMark(index)) => {
                open.cursor = index;
                open.toggle(index);
            }
            Some(WorkspaceHit::ExtensionOfferRow(index)) => open.cursor = index,
            Some(WorkspaceHit::ExtensionOfferWhere) => open.everywhere = !open.everywhere,
            Some(WorkspaceHit::ExtensionOfferAll) => open.toggle_all(),
            Some(WorkspaceHit::ExtensionOfferWhole) => {
                open.step = match open.step {
                    OfferStep::List => OfferStep::Whole { first: 0 },
                    _ => OfferStep::List,
                };
            }
            Some(WorkspaceHit::ExtensionOfferBar(first)) => open.first = first,
            Some(WorkspaceHit::ExtensionOfferBody) => {}
            _ => self.answer_extension_offer(false),
        }
    }

    /// The wheel over the question scrolls what it shows, never the
    /// cursor.
    pub(super) fn extension_offer_wheel(&mut self, down: bool) {
        if let Some(open) = self.model.extension_offer.as_mut() {
            open.scroll(down);
            self.model.dirty = true;
        }
    }
}

/// The question, drawn over everything but the toasts.
pub(super) fn render_extension_offer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ExtensionOfferDialog,
    hovered: Option<WorkspaceHit>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    crate::ui::widget::scrim::render(frame, area);
    let mut own = Vec::new();
    match open.step {
        OfferStep::Programs => render_programs(frame, area, open, &mut own),
        OfferStep::List | OfferStep::Whole { .. } => {
            render_list(frame, area, open, Lit(hovered), &mut own)
        }
    }
    // Before everything it covers, each target before the dialog's own
    // body: a click on the question is not a click on what is underneath.
    hits.splice(0..0, own);
}

/// The programs of every row chosen that runs any: where each was found
/// and what running it means on this machine, the same words the
/// Extensions screen asks with.
fn render_programs(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ExtensionOfferDialog,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let owing: Vec<&ExtensionReport> = open
        .owing_programs()
        .into_iter()
        .map(|index| &open.offers[index])
        .collect();
    let confined = owing.iter().all(|offer| offer.confined.unwrap_or(false));
    let programs: Vec<uze_application::ProgramPin> = owing
        .iter()
        .flat_map(|offer| offer.programs.iter().cloned())
        .collect();
    let mut body = crate::ui::overlay::programs_lines(&programs, confined);
    let subject = match owing.as_slice() {
        [one] => one.attribution(),
        many => format!("{} programs of {} extensions", programs.len(), many.len()),
    };
    if open.chosen_count() > owing.len() {
        body.push("Cancel leaves these off and enables the rest.".to_owned());
    }
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: if confined {
                Tone::Caution
            } else {
                Tone::Danger
            },
            title: if confined {
                "Allow these programs to run, confined?"
            } else {
                "Allow these programs to run as you?"
            }
            .into(),
            subject: Some(Line::from(subject)),
            body,
            confirm: Some("Allow and enable"),
            focus: Some(if open.on_allow { 1 } else { CANCEL }),
            field: None,
            ..Default::default()
        },
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        WorkspaceHit::ExtensionOfferAnswer(false),
        WorkspaceHit::ExtensionOfferAnswer(true),
    );
    hits.extend(answers.buttons);
    hits.push((answers.popup, WorkspaceHit::ExtensionOfferBody));
}

/// What the pointer is over: a target under it is drawn lit, the colour
/// change every hover here is.
#[derive(Clone, Copy)]
struct Lit(Option<WorkspaceHit>);

impl Lit {
    fn tone(self, hit: WorkspaceHit, resting: Token) -> Token {
        if self.0 == Some(hit) {
            Token::TextBright
        } else {
            resting
        }
    }
}

/// One grant, or another thing an extension asks for, as the detail lists
/// it: its name, what it means, and whether it is more than was allowed.
struct Asked {
    name: String,
    meaning: String,
    wider: bool,
}

/// Everything `offer` asks for, in the order the detail lists it: its
/// grants but its programs, the rows its bands take, then its programs,
/// which the next step asks about.
fn asked_of(offer: &ExtensionReport) -> Vec<Asked> {
    let mut asked: Vec<Asked> = offer
        .grants
        .iter()
        .filter(|grant| !grant.starts_with("exec:"))
        .map(|grant| Asked {
            name: grant.clone(),
            meaning: uze_application::grant_meaning(grant).to_owned(),
            wider: offer.new_grants.contains(grant),
        })
        .collect();
    asked.extend(offer.band_rows.map(|rows| Asked {
        name: "band".to_owned(),
        meaning: crate::ui::overlay::band_rows_said(rows),
        wider: false,
    }));
    asked.extend(offer.programs.iter().map(|program| Asked {
        name: program.name.clone(),
        meaning: if owes_programs(offer) {
            "runs a program, asked about on the next step".to_owned()
        } else {
            "runs a program you allowed".to_owned()
        },
        wider: false,
    }));
    asked
}

/// `notes · new · 2 grants`: the marked row's heading.
fn standing_of(offer: &ExtensionReport, everywhere: bool) -> String {
    let standing = match offer.condition {
        ExtensionCondition::Held => "its update wants more than you allowed",
        ExtensionCondition::NeedsEnabling => "you allowed it on this machine",
        _ => "new",
    };
    let grants = offer
        .grants
        .iter()
        .filter(|grant| !grant.starts_with("exec:"))
        .count();
    let mut said = format!(
        "{standing} · {grants} {}",
        if grants == 1 { "grant" } else { "grants" }
    );
    if everywhere && offer.reaches_agents {
        said.push_str(" · reaches agents, so this project only");
    }
    said
}

/// What a row says against its end: what is notable about it.
fn tag_of(offer: &ExtensionReport) -> (String, Token) {
    if offer.condition == ExtensionCondition::Held {
        return ("wants more".to_owned(), Token::StateWarning);
    }
    if owes_programs(offer) {
        let names: Vec<&str> = offer
            .programs
            .iter()
            .map(|program| program.name.as_str())
            .collect();
        return (format!("runs {}", names.join(", ")), Token::StateWarning);
    }
    if offer.condition == ExtensionCondition::NeedsEnabling {
        return ("allowed".to_owned(), Token::TextDim);
    }
    let grants = offer
        .grants
        .iter()
        .filter(|grant| !grant.starts_with("exec:"))
        .count();
    (
        format!("{grants} {}", if grants == 1 { "grant" } else { "grants" }),
        Token::TextMuted,
    )
}

/// The list, its marked row's detail, and the field and answers under
/// them; or, in the list's place, everything the marked row asks for.
fn render_list(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ExtensionOfferDialog,
    lit: Lit,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let width = area
        .width
        .saturating_sub(4)
        .clamp(WIDTH.0, WIDTH.1)
        .min(area.width);
    let measure = usize::from(width.saturating_sub(dialog::shell_chrome_width()));
    let intro = text::fold(
        &format!(
            "Plugins this project uses bring them. Each runs a script inside the workspace, \
             {}.",
            if open.everywhere {
                "in every project".to_owned()
            } else {
                format!("in {} only", open.named)
            }
        ),
        measure,
    );
    // The subject, air, the intro, air; the list; the rule, the detail;
    // air, then the answers. The list and the detail share what is left,
    // the detail taking no more than half.
    let fixed = 1 + 1 + intro.len() as u16 + 1 + 1 + 1 + 1;
    let available = area.height.saturating_sub(2 + dialog::SHELL_ROWS + fixed);
    let detail_rows = (available / 2).clamp(DETAIL_ROWS.0, DETAIL_ROWS.1);
    let room = available
        .saturating_sub(detail_rows)
        .clamp(LIST_ROWS.0, LIST_ROWS.1);
    let shown = (open.offers.len() as u16).min(room);
    open.shown.set(usize::from(shown));
    open.detail.set(detail_rows);
    let fixed = fixed + detail_rows;
    let whole = matches!(open.step, OfferStep::Whole { .. });
    let hint = dialog::border_hint(
        &[uze_keys::Scope::Global, uze_keys::Scope::Choose],
        &list_hint(open, whole),
    );
    let title = match open.offers.as_slice() {
        [one] => format!("Enable {} in {}?", one.name, open.named),
        _ => format!("Enable extensions in {}?", open.named),
    };
    let shell = dialog::shell(frame, area, width, fixed + shown, &title, hint);
    let body = shell.body;
    let mut y = body.y;
    let line = |frame: &mut ratatui::Frame<'_>, y: u16, line: Line<'static>| {
        if y < body.bottom() {
            frame.render_widget(Paragraph::new(line), Rect::new(body.x, y, body.width, 1));
        }
    };
    let plugins: std::collections::BTreeSet<(&str, &str)> = open
        .offers
        .iter()
        .map(|offer| (offer.plugin.as_str(), offer.marketplace.as_str()))
        .collect();
    let subject = if whole {
        "all it asks for".to_owned()
    } else {
        let count = open.offers.len();
        let mut said = format!(
            "{count} {} from {} {}",
            if count == 1 {
                "extension"
            } else {
                "extensions"
            },
            plugins.len(),
            if plugins.len() == 1 {
                "plugin"
            } else {
                "plugins"
            }
        );
        if count > 1 {
            said.push_str(&format!(" · {} on", open.chosen_count()));
        }
        said
    };
    let mut subject_line = vec![Span::styled(subject.clone(), theme::fg(Token::TextMuted))];
    if !whole && open.offers.len() > 1 {
        // The pointer's way to what `a` does: a quiet word that brightens.
        const ALL: &str = "switch all";
        let x = body.x + (text::columns(&subject) + 3) as u16;
        subject_line.push(Span::styled(" · ", theme::fg(Token::TextDim)));
        subject_line.push(Span::styled(
            ALL,
            theme::fg(lit.tone(WorkspaceHit::ExtensionOfferAll, Token::TextDim)),
        ));
        hits.push((
            Rect::new(x, y, ALL.len() as u16, 1),
            WorkspaceHit::ExtensionOfferAll,
        ));
    }
    line(frame, y, Line::from(subject_line));
    y += 2;
    for folded in &intro {
        line(
            frame,
            y,
            Line::from(Span::styled(
                folded.clone(),
                theme::fg(Token::TextSecondary),
            )),
        );
        y += 1;
    }
    y += 1;
    let region = Rect::new(body.x, y, body.width, shown + 1 + detail_rows);
    if let OfferStep::Whole { first } = open.step {
        render_whole(frame, region, open, first, lit, hits);
    } else {
        render_rows(
            frame,
            Rect::new(body.x, y, body.width, shown),
            open,
            lit,
            hits,
        );
        render_detail(
            frame,
            Rect::new(body.x, y + shown, body.width, 1 + detail_rows),
            open,
            lit,
            hits,
        );
    }
    let answers_row = Rect::new(body.x, region.bottom() + 1, body.width, 1);
    if answers_row.y < body.bottom() {
        render_answers(frame, answers_row, open, lit, hits);
    }
    hits.push((shell.popup, WorkspaceHit::ExtensionOfferBody));
}

/// The keys the list answers to now, for its bottom border.
fn list_hint(open: &ExtensionOfferDialog, whole: bool) -> Vec<(Action, &'static str)> {
    let enable = if open.chosen_count() == 0 {
        "select one first"
    } else if open.on_allow {
        "enable"
    } else {
        "cancel"
    };
    if whole {
        return vec![
            (Action::SelectNext, "scroll"),
            (Action::Dismiss, "back"),
            (Action::Activate, enable),
        ];
    }
    let mut keys = vec![
        (Action::SelectNext, "move"),
        (Action::ToggleChoice, "switch"),
        (Action::NextValue, "where"),
    ];
    if open
        .offers
        .get(open.cursor)
        .is_some_and(|offer| asked_of(offer).len() > usize::from(open.detail.get()) - 1)
    {
        keys.push((Action::Expand, "see all"));
    }
    keys.push((Action::Activate, enable));
    keys
}

/// The rows from `first`, each with the cursor, its mark, its name, its
/// plugin and what is notable about it, and the scrollbar when they do
/// not all fit.
fn render_rows(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ExtensionOfferDialog,
    lit: Lit,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let visible = usize::from(area.height);
    let first = open.first.min(open.offers.len().saturating_sub(visible));
    let mut list = area;
    let bar = Scrollbar::measure(
        Rect::new(
            area.right().saturating_sub(Scrollbar::width()),
            area.y,
            Scrollbar::width(),
            area.height,
        ),
        visible,
        open.offers.len(),
    );
    if let Some(bar) = bar {
        list.width = list.width.saturating_sub(Scrollbar::width() + 1);
        for row in 0..area.height {
            hits.push((
                Rect::new(bar_x(area), area.y + row, Scrollbar::width(), 1),
                WorkspaceHit::ExtensionOfferBar(bar.first_at(area.y + row)),
            ));
        }
    }
    let cursor_width = usize::from(theme::width(Symbol::Prompt)) + 1;
    let name_width = 22;
    for (index, offer) in open.offers.iter().enumerate().skip(first).take(visible) {
        let row = Rect::new(list.x, list.y + (index - first) as u16, list.width, 1);
        let marked = index == open.cursor;
        let (tag, tag_tone) = tag_of(offer);
        let tag_width = text::columns(&tag);
        let attribution_width = usize::from(row.width)
            .saturating_sub(cursor_width + 2 + name_width + 2 + tag_width + 2);
        let mark_hit = WorkspaceHit::ExtensionOfferMark(index);
        let mark = crate::ui::widget::form::check(open.chosen[index], lit.0 == Some(mark_hit));
        let name = text::elide(&offer.name, name_width);
        let attribution = text::elide(
            &format!("{}@{}", offer.plugin, offer.marketplace),
            attribution_width,
        );
        let gap = usize::from(row.width).saturating_sub(
            cursor_width + 2 + name_width + 2 + text::columns(&attribution) + tag_width,
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                // The cursor every list in the product points with.
                Span::styled(
                    if marked {
                        format!("{} ", theme::glyph(Symbol::Prompt))
                    } else {
                        " ".repeat(cursor_width)
                    },
                    theme::fg_bold(Token::Accent),
                ),
                mark,
                Span::raw(" "),
                Span::styled(
                    format!("{name:<name_width$}  "),
                    if marked {
                        theme::fg_bold(Token::TextBright)
                    } else {
                        theme::fg(Token::TextPrimary)
                    },
                ),
                Span::styled(attribution, theme::fg(Token::TextMuted)),
                Span::raw(" ".repeat(gap)),
                Span::styled(tag, theme::fg(tag_tone)),
            ])),
            row,
        );
        // The mark is its own target and stands before the row's: a click
        // on it switches the row, one anywhere else on the row marks it.
        let mark_x = row.x + cursor_width as u16;
        hits.push((
            Rect::new(mark_x, row.y, 1, 1),
            WorkspaceHit::ExtensionOfferMark(index),
        ));
        hits.push((row, WorkspaceHit::ExtensionOfferRow(index)));
    }
    if let Some(bar) = bar {
        bar.render(frame, first);
    }
}

/// Where a list's scrollbar stands in `area`.
fn bar_x(area: Rect) -> u16 {
    area.right().saturating_sub(Scrollbar::width())
}

/// A hairline, then the marked row's heading and what it asks for, each a
/// name and its meaning in two columns, cut to the rows the detail keeps.
fn render_detail(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ExtensionOfferDialog,
    lit: Lit,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let content = crate::ui::widget::Rule::new(crate::ui::widget::Edge::Top).render(frame, area);
    let Some(offer) = open.offers.get(open.cursor) else {
        return;
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(offer.name.clone(), theme::fg_bold(Token::TextBright)),
        Span::styled(
            format!(" · {}", standing_of(offer, open.everywhere)),
            theme::fg(Token::TextMuted),
        ),
    ])];
    let asked = asked_of(offer);
    let room = usize::from(content.height).saturating_sub(1).max(1);
    let over = asked.len() > room;
    let shown = if over { room - 1 } else { room };
    let meaning_width = usize::from(content.width).saturating_sub(GRANT_COLUMN + 2);
    lines.extend(asked.iter().take(shown).map(|asked| {
        Line::from(vec![
            Span::styled(
                format!(
                    "{:<GRANT_COLUMN$}  ",
                    text::elide(&asked.name, GRANT_COLUMN)
                ),
                theme::fg(if asked.wider {
                    Token::StateWarning
                } else {
                    Token::TextCode
                }),
            ),
            Span::styled(
                text::elide(&asked.meaning, meaning_width),
                theme::fg(Token::TextSecondary),
            ),
        ])
    }));
    if over {
        let more = format!(
            "{} {} more · see all {}",
            theme::glyph(Symbol::MarkMore),
            asked.len() - shown,
            theme::glyph(Symbol::StepNext)
        );
        let row = Rect::new(
            content.x + GRANT_COLUMN as u16 + 2,
            content.y + lines.len() as u16,
            text::columns(&more) as u16,
            1,
        );
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(GRANT_COLUMN + 2)),
            Span::styled(
                more,
                theme::fg(lit.tone(WorkspaceHit::ExtensionOfferWhole, Token::TextMuted)),
            ),
        ]));
        hits.push((row, WorkspaceHit::ExtensionOfferWhole));
    }
    frame.render_widget(Paragraph::new(lines), content);
}

/// Everything the marked row asks for in the list's place: a heading that
/// goes back, then each thing it asks for with its meaning folded beside
/// it, scrolled by whole entries.
fn render_whole(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    open: &ExtensionOfferDialog,
    first: usize,
    lit: Lit,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let Some(offer) = open.offers.get(open.cursor) else {
        return;
    };
    let heading = Line::from(vec![
        Span::styled(
            format!("{} ", theme::glyph(Symbol::StepPrevious)),
            theme::fg(lit.tone(WorkspaceHit::ExtensionOfferWhole, Token::TextDim)),
        ),
        Span::styled(offer.name.clone(), theme::fg_bold(Token::TextBright)),
        Span::styled(
            format!(" · {}", offer.attribution()),
            theme::fg(Token::TextMuted),
        ),
    ]);
    frame.render_widget(Paragraph::new(heading), Rect { height: 1, ..area });
    hits.push((
        Rect { height: 1, ..area },
        WorkspaceHit::ExtensionOfferWhole,
    ));
    let mut list = Rect::new(
        area.x,
        area.y + 2,
        area.width,
        area.height.saturating_sub(2),
    );
    let asked = asked_of(offer);
    let meaning_width = usize::from(list.width)
        .saturating_sub(GRANT_COLUMN + 2 + usize::from(Scrollbar::width()) + 1);
    let folded: Vec<Vec<String>> = asked
        .iter()
        .map(|asked| text::fold(&asked.meaning, meaning_width.max(1)))
        .collect();
    let first = first.min(asked.len().saturating_sub(1));
    let bar = Scrollbar::measure(
        Rect::new(bar_x(list), list.y, Scrollbar::width(), list.height),
        usize::from(list.height),
        folded.iter().map(Vec::len).sum(),
    );
    if bar.is_some() {
        list.width = list.width.saturating_sub(Scrollbar::width() + 1);
    }
    let mut lines = Vec::new();
    for (asked, meaning) in asked.iter().zip(&folded).skip(first) {
        if lines.len() + meaning.len() > usize::from(list.height) {
            break;
        }
        for (index, part) in meaning.iter().enumerate() {
            let name = if index == 0 {
                format!(
                    "{:<GRANT_COLUMN$}  ",
                    text::elide(&asked.name, GRANT_COLUMN)
                )
            } else {
                " ".repeat(GRANT_COLUMN + 2)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    name,
                    theme::fg(if asked.wider {
                        Token::StateWarning
                    } else {
                        Token::TextCode
                    }),
                ),
                Span::styled(part.clone(), theme::fg(Token::TextSecondary)),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), list);
    if let Some(bar) = bar {
        let row = folded.iter().take(first).map(Vec::len).sum();
        bar.render(frame, row);
    }
}

/// `where ‹ this project ›` against the start, the answers against the
/// end.
fn render_answers(
    frame: &mut ratatui::Frame<'_>,
    row: Rect,
    open: &ExtensionOfferDialog,
    lit: Lit,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let place = if open.everywhere {
        "every project"
    } else {
        "this project"
    };
    let arrows = lit.tone(WorkspaceHit::ExtensionOfferWhere, Token::TextDim);
    let field = Line::from(vec![
        Span::styled("where  ", theme::fg(Token::TextMuted)),
        Span::styled(
            format!("{} ", theme::glyph(Symbol::StepPrevious)),
            theme::fg(arrows),
        ),
        Span::styled(place, theme::fg_bold(Token::TextBright)),
        Span::styled(
            format!(" {}", theme::glyph(Symbol::StepNext)),
            theme::fg(arrows),
        ),
    ]);
    let field_width = field.width() as u16;
    frame.render_widget(Paragraph::new(field), row);
    hits.push((
        Rect::new(row.x, row.y, field_width.min(row.width), 1),
        WorkspaceHit::ExtensionOfferWhere,
    ));
    let count = open.chosen_count();
    let confirm = match count {
        0 => "Enable".to_owned(),
        n if open.everywhere => format!("Enable {n} everywhere"),
        n => format!("Enable {n}"),
    };
    let buttons = button_row(
        frame,
        row,
        &[
            (
                Button::new("Cancel", Token::TextSecondary).strong(!open.on_allow),
                WorkspaceHit::ExtensionOfferAnswer(false),
            ),
            (
                Button::new(
                    confirm,
                    if count == 0 {
                        Token::TextFaint
                    } else {
                        Tone::Caution.token()
                    },
                )
                .strong(open.on_allow && count > 0),
                WorkspaceHit::ExtensionOfferAnswer(true),
            ),
        ],
        crate::ui::widget::Align::Right,
    );
    hits.extend(buttons);
}
