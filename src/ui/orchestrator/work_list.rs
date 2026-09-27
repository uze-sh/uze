//! One project's list in the work modal: its kept tasks and its checkouts
//! as one row each, grouped by what the row asks of the operator, and the
//! actions the row in front offers.
//!
//! A task and the checkout it left behind are one row, not two: the
//! operator knows the work by its branch, and which record said what about
//! it is UZE's business.

use super::checkouts::{age_to_say, bytes_to_say, cleaned_up, join_of};
use super::work::{Project, Section, WorkOverlay, WorkQuestion, list_row};
use super::*;
use crate::ui::widget::{Button, RowState, stat::Stat, text};
use uze_application::{CheckoutOwner, CheckoutView, PreservedWork};

/// What a row asks of the operator. The groups the list is drawn in, the
/// cards above it and the mark on each row all read this one answer, so
/// the cards are the legend the rows are read by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Standing {
    /// UZE's work, kept, that nobody is working on.
    NeedsYou,
    /// Something is working in it.
    InProgress,
    /// One of UZE's slots, clean and ready for the next agent.
    Free,
    /// One of the operator's, which clean-up would take.
    Removable,
    /// The operator's own, or a harness's: its facts, and nothing asked.
    Settled,
}

impl Standing {
    fn of(task: Option<&PreservedWork>, checkout: Option<&CheckoutView>) -> Self {
        if checkout.is_some_and(|checkout| checkout.in_use) {
            return Self::InProgress;
        }
        if let Some(task) = task {
            return if task.state == WorkStateView::Integrating {
                Self::InProgress
            } else {
                Self::NeedsYou
            };
        }
        let Some(checkout) = checkout else {
            return Self::Settled;
        };
        let holds_work = checkout.dirty || !checkout.in_target;
        match &checkout.owner {
            CheckoutOwner::Agent { live: true, .. } => Self::InProgress,
            CheckoutOwner::Subagent { joinable: true, .. } => Self::NeedsYou,
            // Its agent is running, and joins its own subagents.
            CheckoutOwner::Subagent { topic: Some(_), .. } => Self::InProgress,
            CheckoutOwner::Agent { .. } | CheckoutOwner::Subagent { .. } if holds_work => {
                Self::NeedsYou
            }
            CheckoutOwner::Agent { .. } | CheckoutOwner::Subagent { .. } => Self::Free,
            _ if cleaned_up(checkout) => Self::Removable,
            _ => Self::Settled,
        }
    }

    /// The group it is listed under, by its place in the list and its
    /// heading.
    fn group(self) -> (usize, &'static str) {
        match self {
            Self::NeedsYou => (0, "NEEDS YOU"),
            Self::InProgress => (1, "IN PROGRESS"),
            Self::Free => (2, "READY FOR THE NEXT AGENT"),
            Self::Removable | Self::Settled => (3, "OTHERS"),
        }
    }

    fn hue(self) -> Token {
        match self {
            Self::NeedsYou => Token::StateWarning,
            Self::InProgress => Token::Accent,
            Self::Free => Token::StateSuccess,
            Self::Removable => Token::StateDanger,
            Self::Settled => Token::TextDim,
        }
    }

    fn mark(self) -> Span<'static> {
        Span::styled(
            format!("{} ", theme::glyph(Symbol::MarkStanding)),
            theme::fg(self.hue()),
        )
    }
}

/// One row of a project's list.
#[derive(Clone, Debug)]
pub(super) struct WorkRow {
    pub(super) task: Option<PreservedWork>,
    pub(super) checkout: Option<CheckoutView>,
    /// A task whose checkout is gone: its branch is all that holds the
    /// work.
    pub(super) branch_only: bool,
    pub(super) standing: Standing,
}

impl WorkRow {
    fn new(task: Option<PreservedWork>, checkout: Option<CheckoutView>, branch_only: bool) -> Self {
        let standing = Standing::of(task.as_ref(), checkout.as_ref());
        Self {
            task,
            checkout,
            branch_only,
            standing,
        }
    }

    /// What the row is called: its branch, which is what the operator
    /// knows the work by.
    pub(super) fn title(&self) -> &str {
        self.task
            .as_ref()
            .map(|task| task.branch.as_str())
            .or_else(|| self.checkout.as_ref()?.branch.as_deref())
            .unwrap_or("detached")
    }

    /// The directory the row's work sits in, if it still has one.
    pub(super) fn directory(&self) -> Option<&Path> {
        match (&self.checkout, &self.task) {
            (Some(checkout), _) => Some(&checkout.path),
            (None, Some(task)) if !self.branch_only => task.checkout.as_deref(),
            _ => None,
        }
    }

    /// What taking this row away means: a task's work is discarded, a
    /// checkout nobody's task holds is removed.
    pub(super) fn taking_away(&self) -> WorkQuestion {
        if self.task.is_some() {
            WorkQuestion::Discard
        } else {
            WorkQuestion::Remove
        }
    }

    /// Why removing this row's checkout is refused, as the last read saw it.
    pub(super) fn removal_refusal(&self) -> Option<&str> {
        self.checkout.as_ref()?.removal_refusal.as_deref()
    }

    fn in_use(&self) -> bool {
        self.checkout
            .as_ref()
            .is_some_and(|checkout| checkout.in_use)
    }

    fn last_changed(&self) -> Option<std::time::SystemTime> {
        self.checkout.as_ref()?.last_changed
    }
}

/// The kept tasks of `project`.
fn tasks_of(model: &WorkspaceModel, project: &Project) -> Vec<PreservedWork> {
    model
        .preserved_tasks()
        .into_iter()
        .filter(|work| project.owns(work))
        .collect()
}

/// Every row of `project` in the order it is drawn and selected in: by
/// group, then what somebody is working in, then the most recently
/// changed.
pub(super) fn rows_of(
    model: &WorkspaceModel,
    overlay: &WorkOverlay,
    project: &Project,
) -> Vec<WorkRow> {
    let mut tasks: Vec<Option<PreservedWork>> =
        tasks_of(model, project).into_iter().map(Some).collect();
    let answer = overlay.answer(&project.key);
    let mut rows = Vec::new();
    if let Some(Some(view)) = answer {
        for checkout in &view.checkouts {
            let task = tasks
                .iter_mut()
                .find(|task| {
                    task.as_ref().is_some_and(|task| {
                        checkout.task.as_deref() == Some(task.id.as_str())
                            || task.checkout.as_deref() == Some(checkout.path.as_path())
                    })
                })
                .and_then(Option::take);
            rows.push(WorkRow::new(task, Some(checkout.clone()), false));
        }
    }
    // A read that answered and did not list a task's checkout says it is
    // gone; before the read, the record is all there is to go by.
    let read = matches!(answer, Some(Some(_)));
    for task in tasks.into_iter().flatten() {
        let branch_only = task.checkout.is_none() || read;
        rows.push(WorkRow::new(Some(task), None, branch_only));
    }
    rows.sort_by(|left, right| {
        left.standing
            .group()
            .0
            .cmp(&right.standing.group().0)
            .then_with(|| right.in_use().cmp(&left.in_use()))
            .then_with(|| right.last_changed().cmp(&left.last_changed()))
            .then_with(|| left.title().cmp(right.title()))
    });
    rows
}

/// How many rows need the operator.
pub(super) fn needing_you(rows: &[WorkRow]) -> usize {
    rows.iter()
        .filter(|row| row.standing == Standing::NeedsYou)
        .count()
}

/// Whether `label` is what `branch` already says: an agent labelled from
/// its branch reads as the branch said twice.
fn said_by_branch(label: &str, branch: &str) -> bool {
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
    words(branch).ends_with(&words(label))
}

/// What the row says beside its branch, when that says something the
/// branch does not.
fn secondary_of(row: &WorkRow) -> Option<String> {
    let title = row.title();
    let unsaid = |label: &str| (!said_by_branch(label, title)).then(|| label.to_owned());
    let mut parts: Vec<String> = Vec::new();
    if let Some(task) = &row.task {
        parts.extend(unsaid(&task.label));
    }
    match row.checkout.as_ref().map(|checkout| &checkout.owner) {
        Some(CheckoutOwner::Agent {
            holder: Some(holder),
            ..
        }) if row.task.is_none() => parts.extend(unsaid(holder)),
        Some(CheckoutOwner::Subagent { parent, topic, .. }) => {
            parts.extend(topic.as_deref().and_then(unsaid));
            parts.push(format!("of {parent}"));
        }
        Some(CheckoutOwner::Harness { harness }) => parts.push(format!("left to {harness}")),
        _ => {}
    }
    if row.branch_only {
        parts.push("branch only".to_owned());
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// What is happening in the row's work, in a word or two.
fn happening_of(row: &WorkRow) -> &'static str {
    let dirty = row.checkout.as_ref().is_some_and(|checkout| checkout.dirty);
    if row.in_use() {
        return "in use";
    }
    match row.task.as_ref().map(|task| &task.state) {
        Some(WorkStateView::Conflicted { .. }) => "conflict",
        Some(WorkStateView::GateFailed) => "checks failed",
        Some(WorkStateView::Integrating) => "delivering",
        Some(WorkStateView::Uncommitted) => "uncommitted",
        Some(_) if dirty => "uncommitted",
        Some(WorkStateView::Running) => "was running",
        Some(WorkStateView::Ready) => "ready",
        Some(WorkStateView::Published) => "published",
        Some(_) => "parked",
        None if dirty => "uncommitted",
        None if matches!(
            row.checkout.as_ref().map(|checkout| &checkout.owner),
            Some(CheckoutOwner::Subagent { joinable: true, .. })
        ) =>
        {
            "parked"
        }
        None => "",
    }
}

/// The facts about one row as fixed columns — what is happening in it,
/// where its work stands, how long since it changed, and its size — so a
/// column of rows reads down each fact instead of across a sentence.
fn facts_of(row: &WorkRow, target: &str) -> String {
    let happening = happening_of(row);
    let (work, age, size) = match &row.checkout {
        Some(checkout) => (
            match (checkout.in_target, checkout.ahead) {
                (true, _) => format!("in {target}"),
                (false, 0) => format!("not in {target}"),
                (false, ahead) => format!("{ahead} ahead"),
            },
            age_to_say(checkout.last_changed).unwrap_or_default(),
            bytes_to_say(checkout.bytes),
        ),
        None => Default::default(),
    };
    format!("{happening:>13}  {work:>11}  {age:>4}  {size:>8}")
}

/// The figures above the list, each led by the mark its rows wear.
fn cards(rows: &[WorkRow], total_bytes: u64) -> Vec<Stat> {
    let mut cards: Vec<Stat> = [
        (Standing::NeedsYou, "Needs you"),
        (Standing::InProgress, "In progress"),
        (Standing::Free, "Free"),
        (Standing::Removable, "Can remove"),
    ]
    .into_iter()
    .map(|(standing, label)| Stat {
        label: label.to_owned(),
        value: rows
            .iter()
            .filter(|row| row.standing == standing)
            .count()
            .to_string(),
        hue: Token::TextBright,
        mark: Some((theme::glyph(Symbol::MarkStanding), standing.hue())),
    })
    .collect();
    cards.push(Stat {
        label: "On disk".to_owned(),
        value: bytes_to_say(total_bytes),
        hue: Token::StateWarning,
        mark: None,
    });
    cards
}

/// The question a change asks before it is made.
pub(super) fn question_to_say(
    question: WorkQuestion,
    row: Option<&WorkRow>,
    rows: &[WorkRow],
    target: &str,
) -> String {
    let title = row.map_or("this", WorkRow::title);
    match question {
        WorkQuestion::Discard if row.is_some_and(|row| row.directory().is_none()) => {
            format!("discard {title}? its branch is deleted, and the work on it is lost")
        }
        WorkQuestion::Discard => format!(
            "discard {title}? its checkout and its branch are deleted, and the work in them \
             is lost"
        ),
        WorkQuestion::Remove => format!(
            "remove {title}? its branch is kept, and {} is freed",
            row.and_then(|row| row.checkout.as_ref()).map_or_else(
                || "its directory".to_owned(),
                |checkout| { bytes_to_say(checkout.bytes) }
            )
        ),
        WorkQuestion::Adopt => format!(
            "adopt {title} as UZE's slot? a clean one becomes free for the next agent at once"
        ),
        WorkQuestion::Join => match row.and_then(|row| row.checkout.as_ref()).and_then(join_of) {
            Some(super::checkouts::CheckoutChange::Join { parent, topic, .. }) => format!(
                "join {topic} into {parent}? its commits are replayed onto {parent}'s branch"
            ),
            _ => "join this subagent into its agent?".to_owned(),
        },
        WorkQuestion::CleanUp => {
            let candidates: Vec<&CheckoutView> = rows
                .iter()
                .filter_map(|row| row.checkout.as_ref())
                .filter(|checkout| cleaned_up(checkout))
                .collect();
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

/// The actions the row in front offers, each enabled only where it would
/// do something now, and clean-up for the whole project.
fn buttons_for(
    model: &WorkspaceModel,
    row: Option<&WorkRow>,
    rows: &[WorkRow],
) -> Vec<(Button, Action)> {
    let mut buttons = Vec::new();
    if let Some(row) = row {
        if let Some(task) = &row.task {
            let delivering = task.state == WorkStateView::Integrating
                || model.remembered.delivery_pending.contains(&task.id);
            buttons.push((
                Button::new(Action::ResumeTask.label(), Token::Accent),
                Action::ResumeTask,
            ));
            buttons.push((
                Button::new(Action::DeliverTask.label(), Token::Accent).enabled(!delivering),
                Action::DeliverTask,
            ));
            buttons.push((
                Button::new(Action::FinishTask.label(), Token::TextSecondary),
                Action::FinishTask,
            ));
        }
        if row.task.is_none() && row.directory().is_some() {
            buttons.push((Button::new("Open space", Token::Accent), Action::Activate));
        }
        if let Some(checkout) = &row.checkout {
            if join_of(checkout).is_some() {
                buttons.push((
                    Button::new(Action::JoinCheckout.label(), Token::Accent),
                    Action::JoinCheckout,
                ));
            }
            if checkout.adoptable && row.task.is_none() {
                buttons.push((
                    Button::new(Action::AdoptCheckout.label(), Token::Accent),
                    Action::AdoptCheckout,
                ));
            }
        }
        match row.taking_away() {
            WorkQuestion::Discard => buttons.push((
                Button::new(Action::DiscardTask.label(), Token::StateDanger),
                Action::DiscardTask,
            )),
            _ if row.checkout.is_some() => buttons.push((
                Button::new("Remove", Token::StateDanger).enabled(row.removal_refusal().is_none()),
                Action::DiscardTask,
            )),
            _ => {}
        }
    }
    buttons.push((
        Button::new(Action::CleanUpCheckouts.label(), Token::StateDanger).enabled(
            rows.iter()
                .filter_map(|row| row.checkout.as_ref())
                .any(cleaned_up),
        ),
        Action::CleanUpCheckouts,
    ));
    buttons
}

/// The content column for `project`, laid out for a list `width` columns
/// wide.
pub(super) fn project_section(
    model: &WorkspaceModel,
    overlay: &WorkOverlay,
    project: Option<&Project>,
    width: u16,
) -> Section {
    let Some(project) = project else {
        let mut section = Section::new("work".to_owned(), String::new());
        section.say("nothing kept, and no space open");
        return section;
    };
    let rows = rows_of(model, overlay, project);
    let read = overlay.reads.get(&project.key);
    let answer = overlay.answer(&project.key);
    let view = overlay.view(&project.key);
    let target = view.map_or("the target", |view| view.target.as_str());
    let mut section = Section::new(project.name(), project.primary.display().to_string());
    if let Some(view) = view {
        section.cards = cards(&rows, view.total_bytes);
    }

    let mut heading = None;
    for (index, row) in rows.iter().enumerate() {
        let (_, group) = row.standing.group();
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
        let selected = index == overlay.selected;
        let state = RowState::of(
            selected,
            model.hovered == Some(WorkspaceHit::WorkRow(index)),
        );
        let facts = facts_of(row, target);
        let room = usize::from(width)
            .saturating_sub(text::columns(&facts) + 6)
            .max(8);
        let title = text::elide(row.title(), room);
        let mut lead = vec![
            Span::raw(" "),
            row.standing.mark(),
            Span::styled(
                title.clone(),
                theme::fg(if selected {
                    Token::TextBright
                } else {
                    Token::TextPrimary
                }),
            ),
        ];
        if let Some(secondary) = secondary_of(row) {
            let left = room.saturating_sub(text::columns(&title) + 2);
            if left > 3 {
                lead.push(Span::styled(
                    format!("  {}", text::elide(&secondary, left)),
                    theme::fg(Token::TextMuted),
                ));
            }
        }
        let first = section.lines.len();
        section
            .lines
            .push((list_row(lead, Some(facts), width, state), Some(index)));
        if selected {
            let place = row.directory().map_or_else(
                || "no checkout — the branch keeps the work".to_owned(),
                |path| path.display().to_string(),
            );
            let mut details = vec![text::elide_head(
                &place,
                usize::from(width).saturating_sub(4),
            )];
            // Said only where removing is the operator's to do: UZE
            // recycles its own slots, and a harness keeps its own, so on
            // those rows the refusal is the same sentence under every one.
            if let Some(checkout) = &row.checkout
                && let Some(reason) = &checkout.removal_refusal
                && matches!(
                    checkout.owner,
                    CheckoutOwner::Operator | CheckoutOwner::Unreadable
                )
            {
                details.push(format!("kept: {reason}"));
            }
            for detail in details {
                section.lines.push((
                    list_row(
                        vec![
                            Span::raw("   "),
                            Span::styled(detail, theme::fg(Token::TextMuted)),
                        ],
                        None,
                        width,
                        state,
                    ),
                    Some(index),
                ));
            }
            section.focus = Some((first, section.lines.len() - 1));
        }
    }

    match (read, answer) {
        (Some(_), None) => {
            if !rows.is_empty() {
                section.lines.push((Line::from(""), None));
            }
            section.say("reading…");
        }
        (_, Some(None)) => section.say("not a Git repository, so it has no checkouts"),
        (_, Some(Some(_))) if rows.is_empty() => {
            section.say("nothing here besides the project's own checkout");
        }
        _ => {}
    }

    let row = rows.get(overlay.selected);
    match overlay.asking {
        Some(question) => section.ask(question_to_say(question, row, &rows, target)),
        None => section.offer(buttons_for(model, row, &rows)),
    }
    section
}
