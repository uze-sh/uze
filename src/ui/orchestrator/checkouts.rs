//! The work modal's checkouts section: every worktree of a space's
//! project, grouped by who it belongs to, and the few explicit things the
//! operator may do to one.
//!
//! What is listed is read on a thread (`spawn_checkouts`) because it walks
//! every checkout to measure it, and every change runs on one too
//! (`spawn_checkout_change`); this module only draws the last answer and
//! says what a change came to.

use super::work::{Section, list_row};
use super::*;
use crate::ui::widget::{Button, RowState, stat::Stat, text};
use uze_application::{CheckoutOwner, CheckoutView, CheckoutsView, CleanUp, JoinedWork};

/// The checkouts section's own state.
pub(super) struct CheckoutsOverlay {
    /// The directory the section was asked about — the space's root. An
    /// answer about any other directory is not this section's.
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
    Join,
    CleanUp,
}

/// A change to the checkouts, as it travels to the thread that makes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CheckoutChange {
    Adopt {
        path: PathBuf,
        name: String,
    },
    Remove {
        path: PathBuf,
        name: String,
    },
    /// A parked agent's subagent, joined into that agent.
    Join {
        parent_id: String,
        parent: String,
        topic: String,
    },
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
    Joined {
        topic: String,
        parent: String,
        answer: std::result::Result<JoinedWork, String>,
    },
    CleanedUp(CleanUp),
}

/// The owner groups, in the order the section lists them.
fn group_of(owner: &CheckoutOwner) -> (usize, &'static str) {
    match owner {
        CheckoutOwner::Agent { .. } => (0, "AGENT SLOTS"),
        CheckoutOwner::Subagent { .. } => (1, "SUBAGENTS"),
        CheckoutOwner::Harness { .. } => (2, "HARNESS ISOLATION"),
        CheckoutOwner::Operator => (3, "YOURS"),
        CheckoutOwner::Unreadable => (4, "UNREADABLE RECORD"),
    }
}

/// Every checkout in the order it is drawn and selected in: by group, then
/// what somebody is working in, then the most recently changed — the order
/// an operator deciding what to keep reads them in.
pub(super) fn listed(view: &CheckoutsView) -> Vec<&CheckoutView> {
    let mut listed: Vec<&CheckoutView> = view.checkouts.iter().collect();
    listed.sort_by(|left, right| {
        group_of(&left.owner)
            .0
            .cmp(&group_of(&right.owner).0)
            .then_with(|| right.in_use.cmp(&left.in_use))
            .then_with(|| right.last_changed.cmp(&left.last_changed))
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

/// How many checkouts the last answer listed.
pub(super) fn checkout_count(model: &WorkspaceModel, overlay: &CheckoutsOverlay) -> Option<usize> {
    answer_for(model, overlay)
        .and_then(|answer| answer.view.as_ref())
        .map(|view| view.checkouts.len())
}

/// What the sidebar says under the section's name.
pub(super) fn sidebar_caption(
    model: &WorkspaceModel,
    overlay: Option<&CheckoutsOverlay>,
) -> String {
    let Some(overlay) = overlay else {
        return "no space open".to_owned();
    };
    match answer_for(model, overlay) {
        None => "reading…".to_owned(),
        Some(CheckoutsResolution { view: None, .. }) => "no repository".to_owned(),
        Some(CheckoutsResolution {
            view: Some(view), ..
        }) => format!("{} on disk", bytes_to_say(view.total_bytes)),
    }
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

/// Who the checkout is for, when that says something its branch does not:
/// an agent labelled from its branch reads as the branch said twice.
fn owner_to_say(owner: &CheckoutOwner, branch: Option<&str>) -> Option<String> {
    let said_by_branch = |label: &str| {
        let words = |text: &str| {
            text.chars()
                .map(|character| {
                    if character.is_alphanumeric() {
                        character.to_ascii_lowercase()
                    } else {
                        ' '
                    }
                })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        branch.is_some_and(|branch| words(branch).ends_with(&words(label)))
    };
    match owner {
        CheckoutOwner::Agent { holder } => holder.clone().filter(|label| !said_by_branch(label)),
        CheckoutOwner::Subagent {
            parent,
            topic,
            joinable,
            ..
        } => Some(
            [
                topic.clone(),
                Some(format!("of {parent}")),
                joinable.then(|| "parked".to_owned()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        ),
        CheckoutOwner::Harness { harness } => Some(format!("left to {harness}")),
        CheckoutOwner::Operator | CheckoutOwner::Unreadable => None,
    }
}

/// The facts about one checkout as fixed columns — what is happening in it,
/// where its work stands, how long since it changed, and its size — so a
/// column of rows reads down each fact instead of across a sentence.
fn facts_to_say(checkout: &CheckoutView, target: &str) -> String {
    let happening = if checkout.in_use {
        "in use"
    } else if checkout.dirty {
        "uncommitted"
    } else {
        ""
    };
    let work = match (checkout.in_target, checkout.ahead) {
        (true, _) => format!("in {target}"),
        (false, 0) => format!("not in {target}"),
        (false, ahead) => format!("{ahead} ahead"),
    };
    let age = age_to_say(checkout.last_changed).unwrap_or_default();
    format!(
        "{happening:>11}  {work:>11}  {age:>4}  {:>8}",
        bytes_to_say(checkout.bytes)
    )
}

/// Where a checkout stands, one answer per checkout: the summary cards
/// count these, and each row carries the same mark as the card that counts
/// it, so the cards are the legend the rows are read by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Standing {
    InUse,
    HoldsWork,
    /// One of UZE's slots, ready for the next agent.
    Free,
    /// One of the operator's, which clean-up takes.
    Removable,
    /// Nothing to do: a harness's, or one somebody else keeps.
    Settled,
}

impl Standing {
    fn of(checkout: &CheckoutView) -> Self {
        if checkout.in_use {
            Self::InUse
        } else if checkout.dirty || !checkout.in_target {
            Self::HoldsWork
        } else if matches!(
            checkout.owner,
            CheckoutOwner::Agent { .. } | CheckoutOwner::Subagent { .. }
        ) {
            Self::Free
        } else if checkout.owner == CheckoutOwner::Operator && checkout.removal_refusal.is_none() {
            Self::Removable
        } else {
            Self::Settled
        }
    }

    fn mark(self) -> (Symbol, Token) {
        match self {
            Self::InUse => (Symbol::MarkToggleOn, Token::Accent),
            Self::HoldsWork => (Symbol::MarkDot, Token::StateWarning),
            Self::Free => (Symbol::MarkToggleOff, Token::StateSuccess),
            Self::Removable => (Symbol::MarkCross, Token::StateDanger),
            Self::Settled => (Symbol::MarkDot, Token::TextDim),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::InUse => "In use",
            Self::HoldsWork => "Holds work",
            Self::Free => "Free",
            Self::Removable => "Can remove",
            Self::Settled => "Settled",
        }
    }
}

fn mark_of(checkout: &CheckoutView) -> Span<'static> {
    let (symbol, hue) = Standing::of(checkout).mark();
    Span::styled(format!("{} ", theme::glyph(symbol)), theme::fg(hue))
}

/// What the checkout's row is called: its branch, which is what the
/// operator knows it by. The path is the selected row's detail.
fn title_of(checkout: &CheckoutView) -> &str {
    checkout.branch.as_deref().unwrap_or("detached")
}

/// Checkouts clean-up would remove as the last read saw them: the
/// operator's own, clean, unused and in the target.
fn clean_up_candidates(view: &CheckoutsView) -> Vec<&CheckoutView> {
    view.checkouts
        .iter()
        .filter(|checkout| {
            checkout.owner == CheckoutOwner::Operator
                && checkout.removal_refusal.is_none()
                && checkout.in_target
                && !checkout.in_use
                && !checkout.dirty
        })
        .collect()
}

/// Whether a clean-up would find anything, as the last read saw it: one
/// that would not is said at once rather than asked about.
pub(super) fn clean_up_would_remove(model: &WorkspaceModel, overlay: &CheckoutsOverlay) -> bool {
    answer_for(model, overlay)
        .and_then(|answer| answer.view.as_ref())
        .is_some_and(|view| !clean_up_candidates(view).is_empty())
}

/// The join a subagent's checkout offers: into its agent, once that agent
/// is parked. A running agent joins its own.
pub(super) fn join_of(checkout: &CheckoutView) -> Option<CheckoutChange> {
    match &checkout.owner {
        CheckoutOwner::Subagent {
            parent,
            parent_id,
            topic: Some(topic),
            joinable: true,
        } => Some(CheckoutChange::Join {
            parent_id: parent_id.clone(),
            parent: parent.clone(),
            topic: topic.clone(),
        }),
        _ => None,
    }
}

/// Why a join is not offered for `checkout`, said when it is asked for.
pub(super) fn join_refusal(checkout: &CheckoutView) -> String {
    match &checkout.owner {
        CheckoutOwner::Subagent { topic: None, .. } => {
            "no subagent holds it any more; remove it instead".to_owned()
        }
        CheckoutOwner::Subagent { parent, .. } => {
            format!("{parent} is still running, and joins its own subagents")
        }
        _ => "only a subagent's checkout joins into its agent".to_owned(),
    }
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
        CheckoutOutcome::Joined {
            topic,
            parent,
            answer: Ok(JoinedWork::Joined { commits }),
        } => (
            ToastKind::Done,
            "joined".to_owned(),
            format!(
                "{topic} into {parent} · {commits} commit{}",
                if *commits == 1 { "" } else { "s" }
            ),
        ),
        CheckoutOutcome::Joined {
            topic,
            answer: Ok(JoinedWork::Conflicted { checkout, paths }),
            ..
        } => (
            ToastKind::Warned,
            "join paused".to_owned(),
            format!(
                "{topic} conflicts on {}; resolve it in {}, continue the rebase, and join again",
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                checkout.display()
            ),
        ),
        CheckoutOutcome::Joined {
            topic,
            answer: Err(reason),
            ..
        } => (
            ToastKind::Failed,
            "not joined".to_owned(),
            format!("{topic}: {reason}"),
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
    view: Option<&CheckoutsView>,
) -> String {
    let title = selected.map_or("this checkout", title_of);
    let target = view.map_or("the target", |view| view.target.as_str());
    match question {
        CheckoutQuestion::Adopt => format!(
            "adopt {title} as UZE's slot? a clean one becomes free for the next agent at once"
        ),
        CheckoutQuestion::Remove => format!(
            "remove {title}? its branch is kept, and {} is freed",
            selected.map_or_else(
                || "its directory".to_owned(),
                |checkout| bytes_to_say(checkout.bytes)
            )
        ),
        CheckoutQuestion::Join => match selected.and_then(join_of) {
            Some(CheckoutChange::Join { parent, topic, .. }) => format!(
                "join {topic} into {parent}? its commits are replayed onto {parent}'s branch"
            ),
            _ => "join this subagent into its agent?".to_owned(),
        },
        CheckoutQuestion::CleanUp => {
            let candidates = view.map(clean_up_candidates).unwrap_or_default();
            let bytes: u64 = candidates.iter().map(|checkout| checkout.bytes).sum();
            let one = candidates.len() == 1;
            format!(
                "clean up? removes {} checkout{} of yours that {} clean, unused and in \
                 {target}, freeing {}",
                candidates.len(),
                if one { "" } else { "s" },
                if one { "is" } else { "are" },
                bytes_to_say(bytes)
            )
        }
    }
}

/// The figures above the list.
fn cards(view: &CheckoutsView) -> Vec<Stat> {
    let mut cards: Vec<Stat> = [
        Standing::InUse,
        Standing::HoldsWork,
        Standing::Free,
        Standing::Removable,
    ]
    .into_iter()
    .map(|standing| {
        let (symbol, hue) = standing.mark();
        Stat {
            label: standing.label().to_owned(),
            value: view
                .checkouts
                .iter()
                .filter(|checkout| Standing::of(checkout) == standing)
                .count()
                .to_string(),
            hue: Token::TextBright,
            mark: Some((theme::glyph(symbol).to_string(), hue)),
        }
    })
    .collect();
    cards.push(Stat {
        label: "On disk".to_owned(),
        value: bytes_to_say(view.total_bytes),
        hue: Token::StateWarning,
        mark: None,
    });
    cards
}

/// The checkouts section, laid out for a list `width` columns wide.
pub(super) fn checkouts_section(
    model: &WorkspaceModel,
    overlay: Option<&CheckoutsOverlay>,
    width: u16,
) -> Section {
    let answer = overlay.and_then(|overlay| answer_for(model, overlay));
    let view = answer.and_then(|answer| answer.view.as_ref());
    let rows = view.map(listed).unwrap_or_default();
    let target = view.map_or("the target", |view| view.target.as_str());
    let project = view
        .map(|view| view.primary.as_path())
        .or(overlay.map(|overlay| overlay.project.as_path()))
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned());

    let mut section = Section::new(
        uze_keys::Scope::Checkouts,
        "Checkouts",
        "this project's worktrees",
    );
    section.trailer = project.map(|name| Span::styled(name, theme::fg(Token::TextSecondary)));
    match (overlay, answer, view) {
        (None, ..) => section.say("open a space to see its project's checkouts"),
        (Some(_), None, _) => section.say("reading every checkout…"),
        (Some(_), Some(_), None) => section.say("not a Git repository, so it has no checkouts"),
        (Some(_), Some(_), Some(view)) => {
            section.cards = cards(view);
            if rows.is_empty() {
                section.say("no checkout besides the project's own");
            }
        }
    }

    let selected_index = overlay.map(|overlay| overlay.selected);
    let mut heading = None;
    for (index, checkout) in rows.iter().enumerate() {
        let (_, group) = group_of(&checkout.owner);
        if heading != Some(group) {
            if heading.is_some() {
                section.lines.push((Line::from(""), None));
            }
            heading = Some(group);
            section.lines.push((
                Line::from(Span::styled(group, theme::fg(Token::TextDim))),
                None,
            ));
        }
        let selected = Some(index) == selected_index;
        let state = RowState::of(
            selected,
            model.hovered == Some(WorkspaceHit::WorkRow(index)),
        );
        let facts = facts_to_say(checkout, target);
        let room = usize::from(width)
            .saturating_sub(text::columns(&facts) + 6)
            .max(8);
        let title = text::elide(title_of(checkout), room);
        let mut lead = vec![
            Span::raw(" "),
            mark_of(checkout),
            Span::styled(
                title.clone(),
                theme::fg(if selected {
                    Token::TextBright
                } else {
                    Token::TextPrimary
                }),
            ),
        ];
        if let Some(owner) = owner_to_say(&checkout.owner, checkout.branch.as_deref()) {
            let left = room.saturating_sub(text::columns(&title) + 2);
            if left > 3 {
                lead.push(Span::styled(
                    format!("  {}", text::elide(&owner, left)),
                    theme::fg(Token::TextMuted),
                ));
            }
        }
        let first = section.lines.len();
        section
            .lines
            .push((list_row(lead, Some(facts), width, state), Some(index)));
        if selected {
            let mut details = vec![Span::styled(
                text::elide_head(
                    &checkout.path.display().to_string(),
                    usize::from(width).saturating_sub(4),
                ),
                theme::fg(Token::TextMuted),
            )];
            // Said only where removing is the operator's to do: UZE recycles
            // its own slots, and a harness keeps its own, so on those rows
            // the refusal is the same sentence under every one.
            if let Some(reason) = &checkout.removal_refusal
                && matches!(
                    checkout.owner,
                    CheckoutOwner::Operator | CheckoutOwner::Unreadable
                )
            {
                details.push(Span::styled(
                    format!("kept: {reason}"),
                    theme::fg(Token::TextMuted),
                ));
            }
            for detail in details {
                section.lines.push((
                    list_row(vec![Span::raw("   "), detail], None, width, state),
                    Some(index),
                ));
            }
            section.focus = Some((first, section.lines.len() - 1));
        }
    }

    let selected = overlay.and_then(|overlay| selected_checkout(model, overlay));
    match overlay.and_then(|overlay| overlay.asking) {
        Some(question) => section.ask(
            question_to_say(question, selected, view),
            Action::ConfirmCheckoutChange,
        ),
        None => {
            section.offer(vec![
                (
                    Button::new("Open space", Token::Accent).enabled(selected.is_some()),
                    Action::Activate,
                ),
                (
                    Button::new(Action::AdoptCheckout.label(), Token::Accent)
                        .enabled(selected.is_some_and(|checkout| checkout.adoptable)),
                    Action::AdoptCheckout,
                ),
                (
                    Button::new(Action::JoinCheckout.label(), Token::Accent)
                        .enabled(selected.and_then(join_of).is_some()),
                    Action::JoinCheckout,
                ),
                (
                    Button::new(Action::RemoveCheckout.label(), Token::StateDanger).enabled(
                        selected.is_some_and(|checkout| checkout.removal_refusal.is_none()),
                    ),
                    Action::RemoveCheckout,
                ),
                (
                    Button::new(Action::CleanUpCheckouts.label(), Token::StateDanger)
                        .enabled(view.is_some()),
                    Action::CleanUpCheckouts,
                ),
            ]);
        }
    }
    section
}
