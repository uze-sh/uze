//! The checkouts view: every worktree of a space's project, grouped by who
//! it belongs to, and the few explicit things the operator may do to one.
//!
//! What is listed is read on a thread (`spawn_checkouts`) because it walks
//! every checkout to measure it, and every change runs on one too
//! (`spawn_checkout_change`); this module only draws the last answer and
//! says what a change came to.

use super::*;
use crate::ui::widget::{Align, Button, POPUP_H_PAD, Surface, button_row, hint, row, text};
use uze_application::{CheckoutOwner, CheckoutView, CheckoutsView, CleanUp};

/// A row carries a path, a branch and the facts about it, which is more
/// than the preserved list's short rows, so this dialog reads wider.
const CHECKOUTS_MAX_WIDTH: u16 = 100;
const CHECKOUTS_MIN_WIDTH: u16 = 40;

const SCOPES: &[uze_keys::Scope] = &[uze_keys::Scope::Global, uze_keys::Scope::Checkouts];

/// Open state of the checkouts view.
pub(super) struct CheckoutsOverlay {
    /// The directory the view was asked about — the space's root. An
    /// answer about any other directory is not this view's.
    pub(super) project: PathBuf,
    /// Index into [`listed`].
    pub(super) selected: usize,
    /// A change was asked for and waits for its confirmation.
    pub(super) asking: Option<CheckoutQuestion>,
}

impl CheckoutsOverlay {
    pub(super) fn over(project: PathBuf) -> Self {
        Self {
            project,
            selected: 0,
            asking: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CheckoutQuestion {
    Adopt,
    Remove,
    CleanUp,
}

/// A change to the checkouts, as it travels to the thread that makes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CheckoutChange {
    Adopt { path: PathBuf, name: String },
    Remove { path: PathBuf, name: String },
    CleanUp,
}

/// What reading the checkouts of `project` answered; `None` outside a
/// repository.
pub(super) struct CheckoutsResolution {
    pub(super) project: PathBuf,
    /// Which read this answers (see `Remembered::checkouts_asked`).
    pub(super) asked: u64,
    pub(super) view: Option<CheckoutsView>,
}

pub(super) struct CheckoutChangeResolution {
    pub(super) project: PathBuf,
    pub(super) outcome: CheckoutOutcome,
}

pub(super) enum CheckoutOutcome {
    Adopted {
        name: String,
        answer: std::result::Result<uze_application::AdoptedCheckout, String>,
    },
    Removed {
        name: String,
        answer: std::result::Result<uze_application::RemovedCheckout, String>,
    },
    CleanedUp(CleanUp),
}

/// The owner groups, in the order the view lists them.
fn group_of(owner: &CheckoutOwner) -> (usize, &'static str) {
    match owner {
        CheckoutOwner::Agent { .. } => (0, "AGENT SLOTS"),
        CheckoutOwner::Subagent { .. } => (1, "SUBAGENTS"),
        CheckoutOwner::Harness { .. } => (2, "HARNESS ISOLATION"),
        CheckoutOwner::Operator => (3, "YOURS"),
        CheckoutOwner::Unreadable => (4, "UNREADABLE RECORD"),
    }
}

/// Every checkout in the order it is drawn and selected in: by group, and
/// by name within one.
pub(super) fn listed(view: &CheckoutsView) -> Vec<&CheckoutView> {
    let mut listed: Vec<&CheckoutView> = view.checkouts.iter().collect();
    listed.sort_by(|left, right| {
        group_of(&left.owner)
            .0
            .cmp(&group_of(&right.owner).0)
            .then_with(|| left.name.cmp(&right.name))
    });
    listed
}

/// The last answer for `overlay`'s project, if one has arrived.
pub(super) fn answer_for<'a>(
    model: &'a WorkspaceModel,
    overlay: &CheckoutsOverlay,
) -> Option<&'a CheckoutsResolution> {
    model
        .remembered
        .checkouts
        .as_ref()
        .filter(|answer| answer.project == overlay.project)
}

/// The checkout the selection is on.
pub(super) fn selected_checkout<'a>(
    model: &'a WorkspaceModel,
    overlay: &CheckoutsOverlay,
) -> Option<&'a CheckoutView> {
    let view = answer_for(model, overlay)?.view.as_ref()?;
    listed(view).get(overlay.selected).copied()
}

/// `812 KB`, `1.4 GB`: the unit that keeps the number short.
pub(super) fn bytes_to_say(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// `4m`, `3h`, `2d`: how long ago the checkout last changed.
fn age_to_say(changed: Option<std::time::SystemTime>) -> Option<String> {
    let seconds = changed?.elapsed().ok()?.as_secs();
    Some(match seconds {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    })
}

fn owner_to_say(owner: &CheckoutOwner) -> Option<String> {
    match owner {
        CheckoutOwner::Agent { holder } => holder.clone(),
        CheckoutOwner::Subagent { parent } => Some(format!("of {parent}")),
        CheckoutOwner::Harness { harness } => Some(format!("left to {harness}")),
        CheckoutOwner::Operator | CheckoutOwner::Unreadable => None,
    }
}

/// The facts about one checkout, in the order a decision reads them.
fn facts_to_say(checkout: &CheckoutView, target: &str) -> String {
    let mut facts = Vec::new();
    if checkout.in_use {
        facts.push("in use".to_owned());
    }
    if checkout.dirty {
        facts.push("uncommitted".to_owned());
    }
    facts.push(if checkout.in_target {
        format!("in {target}")
    } else {
        format!("not in {target}")
    });
    facts.extend(age_to_say(checkout.last_changed));
    facts.join(" · ")
}

/// What a change came to, as the toast says it: the kind, a title and a
/// detail — both, since a toast worth raising says what it is about.
pub(super) fn describe_change(outcome: &CheckoutOutcome) -> (ToastKind, String, String) {
    match outcome {
        CheckoutOutcome::Adopted {
            name,
            answer: Ok(adopted),
        } => (
            ToastKind::Done,
            "adopted".to_owned(),
            if adopted.free {
                format!("{name} is UZE's now, and free for the next agent")
            } else {
                format!("{name} is UZE's now, parked until its work is dealt with")
            },
        ),
        CheckoutOutcome::Adopted {
            name,
            answer: Err(reason),
        } => (
            ToastKind::Failed,
            "not adopted".to_owned(),
            format!("{name}: {reason}"),
        ),
        CheckoutOutcome::Removed {
            name,
            answer: Ok(removed),
        } => (
            ToastKind::Done,
            "removed".to_owned(),
            format!(
                "{name}, branch kept · freed {}",
                bytes_to_say(removed.bytes)
            ),
        ),
        CheckoutOutcome::Removed {
            name,
            answer: Err(reason),
        } => (
            ToastKind::Failed,
            "not removed".to_owned(),
            format!("{name}: {reason}"),
        ),
        CheckoutOutcome::CleanedUp(clean_up) => describe_clean_up(clean_up),
    }
}

/// How many kept checkouts are named with their reason before the rest
/// are counted: a toast is a line, not a report.
const KEPT_NAMED: usize = 2;

fn describe_clean_up(clean_up: &CleanUp) -> (ToastKind, String, String) {
    let mut parts = Vec::new();
    if !clean_up.removed.is_empty() {
        parts.push(format!(
            "removed {} · freed {}",
            clean_up.removed.len(),
            bytes_to_say(clean_up.freed())
        ));
    }
    for kept in clean_up.kept.iter().take(KEPT_NAMED) {
        parts.push(format!("kept {}: {}", kept.name, kept.reason));
    }
    if clean_up.kept.len() > KEPT_NAMED {
        parts.push(format!("{} more kept", clean_up.kept.len() - KEPT_NAMED));
    }
    if !clean_up.left_to_harness.is_empty() {
        parts.push(format!(
            "{} left to their harness",
            clean_up.left_to_harness.len()
        ));
    }
    if clean_up.removed.is_empty() {
        let detail = if parts.is_empty() {
            "no checkout of yours is clean, unused and in the target".to_owned()
        } else {
            parts.join(" · ")
        };
        (ToastKind::Told, "nothing to clean up".to_owned(), detail)
    } else {
        (ToastKind::Done, "cleaned up".to_owned(), parts.join(" · "))
    }
}

/// The question a change asks before it is made.
fn question_to_say(
    question: CheckoutQuestion,
    selected: Option<&CheckoutView>,
    target: &str,
) -> String {
    let name = selected.map_or("this checkout", |checkout| checkout.name.as_str());
    match question {
        CheckoutQuestion::Adopt => format!(
            "adopt {name} as UZE's slot? a clean one is free at once, and the next agent resets it"
        ),
        CheckoutQuestion::Remove => format!("remove {name}? its branch is kept"),
        CheckoutQuestion::CleanUp => {
            format!("remove every checkout of yours that is clean, unused and in {target}?")
        }
    }
}

/// The checkouts view, centred over the workspace like the preserved list.
pub(super) fn render_checkouts(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overlay: &CheckoutsOverlay,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let answer = answer_for(model, overlay);
    let view = answer.and_then(|answer| answer.view.as_ref());
    let rows = view.map(listed).unwrap_or_default();
    let target = view.map_or("the target", |view| view.target.as_str());

    let project = view
        .map(|view| view.primary.as_path())
        .unwrap_or(&overlay.project)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = Line::from(vec![
        Span::styled("CHECKOUTS", theme::fg(Token::TextMuted)),
        Span::styled(format!("  {project}"), theme::fg(Token::TextSecondary)),
    ]);

    // The body: group headings and rows, with the row each line selects.
    let mut body: Vec<(Line<'static>, Option<usize>)> = Vec::new();
    let mut selected_line = None;
    match (answer, view) {
        (None, _) => body.push((
            Line::from(Span::styled(
                "reading every checkout…",
                theme::fg(Token::TextSecondary),
            )),
            None,
        )),
        (Some(_), None) => body.push((
            Line::from(Span::styled(
                "not a Git repository, so it has no checkouts",
                theme::fg(Token::TextSecondary),
            )),
            None,
        )),
        (Some(_), Some(_)) if rows.is_empty() => body.push((
            Line::from(Span::styled(
                "no checkout besides the project's own",
                theme::fg(Token::TextSecondary),
            )),
            None,
        )),
        _ => {}
    }
    let mut heading = None;
    for (index, checkout) in rows.iter().enumerate() {
        let (_, group) = group_of(&checkout.owner);
        if heading != Some(group) {
            if heading.is_some() {
                body.push((Line::from(""), None));
            }
            heading = Some(group);
            body.push((
                Line::from(Span::styled(group, theme::fg(Token::TextDim))),
                None,
            ));
        }
        let selected = index == overlay.selected;
        let mut spans = vec![
            Span::styled(
                if selected {
                    format!("{} ", theme::glyph(Symbol::ChevronCollapsed))
                } else {
                    "  ".to_owned()
                },
                theme::fg(Token::Accent),
            ),
            Span::styled(
                checkout.name.clone(),
                theme::fg(if selected {
                    Token::TextBright
                } else {
                    Token::TextPrimary
                }),
            ),
        ];
        if let Some(owner) = owner_to_say(&checkout.owner) {
            spans.push(Span::styled(
                format!("  {owner}"),
                theme::fg(Token::TextMuted),
            ));
        }
        spans.push(Span::styled(
            format!("  {}", checkout.branch.as_deref().unwrap_or("detached")),
            theme::fg(Token::TextSecondary),
        ));
        spans.push(Span::styled(
            format!("  {}", facts_to_say(checkout, target)),
            theme::fg(Token::TextMuted),
        ));
        spans.push(Span::styled(
            format!("  {}", bytes_to_say(checkout.bytes)),
            theme::fg(Token::TextSecondary),
        ));
        if selected {
            selected_line = Some(body.len());
        }
        body.push((Line::from(spans), Some(index)));
        if selected && let Some(reason) = &checkout.removal_refusal {
            body.push((
                Line::from(Span::styled(
                    format!("    cannot remove: {reason}"),
                    theme::fg(Token::TextMuted),
                )),
                Some(index),
            ));
        }
    }

    let selected = rows.get(overlay.selected).copied();
    let total = view.map(|view| {
        Line::from(Span::styled(
            format!(
                "{} checkouts · {} on disk",
                view.checkouts.len(),
                bytes_to_say(view.total_bytes)
            ),
            theme::fg(Token::TextSecondary),
        ))
    });
    let (prompt, buttons) = match overlay.asking {
        Some(question) => {
            let mut line = Line::from(Span::styled(
                format!("{}  ", question_to_say(question, selected, target)),
                theme::fg(Token::StateWarning),
            ));
            line.spans.extend(
                hint::line(SCOPES, &[Action::ConfirmCheckoutChange, Action::Dismiss]).spans,
            );
            let buttons = vec![
                (
                    Button::new(Action::ConfirmCheckoutChange.label(), Token::StateDanger)
                        .strong(true),
                    WorkspaceHit::CheckoutAction(Action::ConfirmCheckoutChange),
                ),
                (
                    Button::new("Cancel", Token::TextSecondary),
                    WorkspaceHit::CheckoutAction(Action::Dismiss),
                ),
            ];
            (line, buttons)
        }
        None => {
            let mut offered = vec![Action::Activate];
            let mut buttons = vec![(
                Button::new("Open space", Token::Accent),
                WorkspaceHit::CheckoutAction(Action::Activate),
            )];
            if selected.is_some_and(|checkout| checkout.adoptable) {
                offered.push(Action::AdoptCheckout);
                buttons.push((
                    Button::new(Action::AdoptCheckout.label(), Token::Accent),
                    WorkspaceHit::CheckoutAction(Action::AdoptCheckout),
                ));
            }
            offered.extend([
                Action::RemoveCheckout,
                Action::CleanUpCheckouts,
                Action::Dismiss,
            ]);
            buttons.extend([
                (
                    Button::new(Action::RemoveCheckout.label(), Token::StateDanger),
                    WorkspaceHit::CheckoutAction(Action::RemoveCheckout),
                ),
                (
                    Button::new(Action::CleanUpCheckouts.label(), Token::StateDanger),
                    WorkspaceHit::CheckoutAction(Action::CleanUpCheckouts),
                ),
            ]);
            if selected.is_none() {
                buttons.retain(|(_, hit)| {
                    *hit == WorkspaceHit::CheckoutAction(Action::CleanUpCheckouts)
                });
            }
            (hint::line(SCOPES, &offered), buttons)
        }
    };

    let mut foot: Vec<Line<'static>> = Vec::new();
    foot.extend(total);
    foot.push(prompt);

    let content = std::iter::once(&title)
        .chain(body.iter().map(|(line, _)| line))
        .chain(foot.iter())
        .map(Line::width)
        .max()
        .unwrap_or(0) as u16;
    let width = (content + 2 + 2 * POPUP_H_PAD)
        .clamp(CHECKOUTS_MIN_WIDTH, CHECKOUTS_MAX_WIDTH)
        .min(area.width)
        .max(1);
    let text_width = width.saturating_sub(2 + 2 * POPUP_H_PAD);
    // Title, a blank, the body, a blank, the foot, a blank and the buttons.
    let chrome = 1 + 1 + 1 + foot.len() as u16 + 1 + 1;
    let height = (body.len() as u16 + chrome + 2).min(area.height).max(1);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let inner = Surface::floating()
        .padding(Padding::new(POPUP_H_PAD, POPUP_H_PAD, 0, 0))
        .render(frame, popup);
    let [title_area, _, body_area, _, foot_area, _, buttons_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(foot.len() as u16),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let mut title = title;
    text::clip(&mut title, text_width as usize);
    frame.render_widget(Paragraph::new(title), title_area);

    // The selection is kept on screen: the view scrolls with it, and never
    // further than the list goes.
    let visible = usize::from(body_area.height);
    let offset = selected_line
        .map_or(0, |line| (line + 2).saturating_sub(visible))
        .min(body.len().saturating_sub(visible));
    let mut mine: Vec<(Rect, WorkspaceHit)> = Vec::new();
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(visible);
    for (position, (line, index)) in body.iter().enumerate().skip(offset).take(visible) {
        let mut line = line.clone();
        text::clip(&mut line, text_width as usize);
        if Some(position) == selected_line {
            row::pad_to(
                &mut line.spans,
                text_width,
                theme::color(Token::SurfaceSelected),
            );
        }
        if let Some(index) = index {
            let y = body_area.y + (position - offset) as u16;
            mine.push((
                Rect::new(body_area.x, y, body_area.width, 1),
                WorkspaceHit::CheckoutRow(*index),
            ));
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), body_area);

    for line in &mut foot {
        text::clip(line, text_width as usize);
    }
    frame.render_widget(Paragraph::new(foot), foot_area);
    mine.extend(button_row(frame, buttons_area, &buttons, Align::Left));
    // Last of this dialog's own, so a click on its rows and buttons finds
    // them first and a click elsewhere on it finds nothing to act on.
    mine.push((popup, WorkspaceHit::CheckoutsBody));
    // Prepended: what is underneath must not answer a click meant here.
    hits.splice(0..0, mine);
}
