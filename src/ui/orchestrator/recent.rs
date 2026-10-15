//! The sidebar's recent mode: every agent no pane carries, across every
//! project UZE has recorded, one row each, and what each waits on.
//!
//! A row is an agent, never a checkout. The checkout an agent left behind
//! is a fact on its row — its size, how far it is ahead, whether it holds
//! changes — and a checkout no agent holds is not listed at all: UZE's
//! own slots are collected on their own, and anybody else's are theirs.
//!
//! What is listed comes from the machine's records at once; the facts
//! that need Git arrive from a read per project, drawn from its last
//! answer meanwhile.

use super::*;
use crate::ui::extension_view::NavigatorScroll;
use std::time::SystemTime;
use uze_application::{CheckoutOwner, CheckoutView, CheckoutsView, JoinedWork, PreservedWork};

/// The mode's state. It outlives being open: leaving recent and coming
/// back finds the row that was selected, and the last read of every
/// project, rather than an empty list about to fill.
#[derive(Default)]
pub(super) struct Recent {
    pub(super) open: bool,
    /// Whether a click elsewhere handed the keyboard back while recent
    /// stays drawn: it is opened and closed by its own control, so using
    /// the rest of the workspace never costs a click to leave it first.
    pub(super) keys_elsewhere: bool,
    /// The selected row, by [`RecentRow::key`]: an index would land on a
    /// different agent once the list under it changed.
    pub(super) selected: Option<String>,
    /// Whether the selected row is asking to be discarded or forgotten.
    pub(super) asking: bool,
    /// Whether the keyboard is on going ahead rather than on the way out,
    /// while a question is asked. It starts on the way out: taking work
    /// away is never what a habitual enter does.
    pub(super) on_confirm: bool,
    /// Each project's checkouts, by its primary checkout once a read has
    /// said which that is, and by the directory asked about until then.
    pub(super) reads: BTreeMap<PathBuf, ProjectRead>,
    /// Where the list is scrolled to, which the wheel and the scrollbar
    /// move and a selection the keys move pulls along — the code surface
    /// navigator's own rule.
    pub(super) scroll: NavigatorScroll,
    /// The scrollbar the last frame drew on the sidebar's divider, when
    /// the rows did not fit.
    pub(super) bar: Option<crate::ui::widget::Scrollbar>,
    /// A press on that scrollbar's handle, until the first movement says
    /// whether it scrolls the list or moves the divider.
    pub(super) grab: Option<EdgeDrag>,
    /// The words the rows are narrowed by, matched against each row's
    /// name, branch, project and state.
    pub(super) filter: String,
    /// Whether the search is taking what is typed.
    pub(super) filtering: bool,
    /// Whether agents nobody named are listed too.
    pub(super) show_all: bool,
}

/// What a frame of recent settled on: where the list ended up scrolled,
/// and the scrollbar it drew.
#[derive(Debug, Default)]
pub(super) struct RecentFrame {
    pub(super) scroll: NavigatorScroll,
    pub(super) bar: Option<crate::ui::widget::Scrollbar>,
}

/// One project's read of its checkouts.
pub(super) struct ProjectRead {
    /// The read whose answer is awaited: one to any earlier is dropped.
    pub(super) asked: u64,
    /// `Some(None)` outside a repository; drawn while the next read is
    /// out.
    pub(super) answer: Option<Option<CheckoutsView>>,
}

impl Recent {
    /// Whether recent is open and the keyboard is its.
    pub(super) fn has_keys(&self) -> bool {
        self.open && !self.keys_elsewhere
    }

    fn view(&self, project: &Path) -> Option<&CheckoutsView> {
        self.reads.get(project)?.answer.as_ref()?.as_ref()
    }

    /// Whether a read already answered for the repository `directory` lies
    /// in, so a space rooted below a project's root does not read the
    /// same repository twice.
    pub(super) fn covers(&self, directory: &Path) -> bool {
        self.reads.iter().any(|(primary, read)| {
            matches!(read.answer, Some(Some(_))) && directory.starts_with(primary)
        })
    }
}

/// Why a row waits on the operator, or what state it is in when it does
/// not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Reason {
    Conflict { files: usize },
    ChecksFailed,
    Uncommitted,
    JoinReady,
    Ready,
    Delivering,
    WithReviewer,
    Shelved,
    NoChanges,
    ProjectGone,
}

impl Reason {
    /// Whether only the operator moves it on: these are listed first.
    pub(super) fn waits_on_you(self) -> bool {
        matches!(
            self,
            Self::Conflict { .. }
                | Self::ChecksFailed
                | Self::Uncommitted
                | Self::JoinReady
                | Self::Ready
        )
    }

    pub(super) fn words(self) -> String {
        match self {
            Self::Conflict { files: 0 } => "conflict".to_owned(),
            Self::Conflict { files: 1 } => "conflict in 1 file".to_owned(),
            Self::Conflict { files } => format!("conflict in {files} files"),
            Self::ChecksFailed => "checks failed".to_owned(),
            Self::Uncommitted => "uncommitted changes".to_owned(),
            Self::JoinReady => "subagent ready to join".to_owned(),
            Self::Ready => "ready to deliver".to_owned(),
            Self::Delivering => "delivering".to_owned(),
            Self::WithReviewer => "with reviewer".to_owned(),
            Self::Shelved => "shelved".to_owned(),
            Self::NoChanges => "no changes yet".to_owned(),
            Self::ProjectGone => "project not found".to_owned(),
        }
    }

    /// The hue the reason is said in on the selected row.
    pub(super) fn hue(self) -> Token {
        match self {
            Self::Conflict { .. } | Self::JoinReady => Token::StateWarning,
            Self::ChecksFailed => Token::StateDanger,
            Self::Uncommitted => Token::StateInfo,
            Self::Ready => Token::StateSuccess,
            Self::Delivering
            | Self::WithReviewer
            | Self::Shelved
            | Self::NoChanges
            | Self::ProjectGone => Token::TextSecondary,
        }
    }

    fn of(state: &WorkStateView, dirty: bool) -> Self {
        match state {
            WorkStateView::Conflicted { files } => Self::Conflict { files: files.len() },
            WorkStateView::GateFailed => Self::ChecksFailed,
            WorkStateView::Uncommitted => Self::Uncommitted,
            _ if dirty => Self::Uncommitted,
            WorkStateView::Ready => Self::Ready,
            WorkStateView::Integrating => Self::Delivering,
            WorkStateView::Published => Self::WithReviewer,
            WorkStateView::Shelved => Self::Shelved,
            WorkStateView::Running | WorkStateView::Integrated | WorkStateView::Closed => {
                Self::NoChanges
            }
        }
    }
}

/// A subagent's checkout its agent left unjoined, and what joining it
/// asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Join {
    pub(super) parent_id: String,
    pub(super) parent: String,
    pub(super) topic: String,
}

/// One row: an agent no pane carries, with what its checkout says.
#[derive(Clone, Debug)]
pub(super) struct RecentRow {
    /// The task's id, or a subagent checkout's path when no task names it.
    pub(super) key: String,
    pub(super) name: String,
    /// The repository's primary checkout.
    pub(super) project: PathBuf,
    pub(super) branch: Option<String>,
    pub(super) reason: Reason,
    pub(super) ahead: Option<usize>,
    pub(super) bytes: Option<u64>,
    pub(super) changed: Option<SystemTime>,
    pub(super) task: Option<PreservedWork>,
    pub(super) join: Option<Join>,
    /// Whether somebody named it. A subagent's row is its topic, which is
    /// a name by construction.
    pub(super) named: bool,
}

impl RecentRow {
    fn of_task(task: &PreservedWork, checkout: Option<&CheckoutView>, reason: Reason) -> Self {
        let active = SystemTime::UNIX_EPOCH + Duration::from_secs(task.last_active_unix);
        Self {
            key: task.id.clone(),
            name: if task.label.is_empty() {
                task.branch.clone()
            } else {
                task.label.clone()
            },
            project: task.project.clone(),
            branch: Some(task.branch.clone()),
            reason,
            ahead: checkout.map(|checkout| checkout.ahead),
            bytes: checkout.map(|checkout| checkout.bytes),
            changed: checkout
                .and_then(|checkout| checkout.last_changed)
                .or(Some(active)),
            task: Some(task.clone()),
            join: checkout.and_then(join_of),
            named: task.named,
        }
    }

    fn of_subagent(project: &Path, checkout: &CheckoutView, join: Join) -> Self {
        Self {
            key: checkout.path.display().to_string(),
            name: join.topic.clone(),
            project: project.to_path_buf(),
            branch: checkout.branch.clone(),
            reason: Reason::JoinReady,
            ahead: Some(checkout.ahead),
            bytes: Some(checkout.bytes),
            changed: checkout.last_changed,
            task: None,
            join: Some(join),
            named: true,
        }
    }

    /// Whether `needle`, already lowercased, is in anything the row says:
    /// its name, its branch, its project or its state.
    fn matches(&self, needle: &str) -> bool {
        [
            Some(self.name.clone()),
            self.branch.clone(),
            Some(project_name(&self.project)),
            Some(self.reason.words()),
        ]
        .into_iter()
        .flatten()
        .any(|said| said.to_lowercase().contains(needle))
    }

    /// What enter does to this row, by the word the foot names it with.
    pub(super) fn primary(&self) -> Option<&'static str> {
        match self.reason {
            Reason::ProjectGone => None,
            Reason::JoinReady => Some("join"),
            _ if self.task.is_some() => Some("resume"),
            _ => None,
        }
    }

    pub(super) fn delivers(&self) -> bool {
        self.reason == Reason::Ready && self.task.is_some()
    }

    /// What `d` does to this row: forgetting where the project is gone,
    /// discarding wherever there is a task to discard.
    pub(super) fn taking_away(&self) -> Option<&'static str> {
        match (&self.task, self.reason) {
            (None, _) => None,
            (Some(_), Reason::ProjectGone) => Some("forget"),
            (Some(_), _) => Some("discard"),
        }
    }

    /// The keys this row offers and the word each goes by here: enter's
    /// move, then delivering and taking away where they apply.
    pub(super) fn offers(&self) -> Vec<(Action, &'static str)> {
        let mut offers = Vec::new();
        if let Some(word) = self.primary() {
            offers.push((Action::Activate, word));
        }
        if self.delivers() {
            offers.push((Action::DeliverTask, "deliver"));
        }
        if let Some(word) = self.taking_away() {
            offers.push((Action::DiscardTask, word));
        }
        offers
    }

    /// The question asked before taking the row away.
    pub(super) fn question(&self) -> &'static str {
        if self.reason == Reason::ProjectGone {
            "forget this record? nothing on disk changes"
        } else {
            "discard branch, commits and conversation?"
        }
    }
}

/// The join a subagent's checkout offers: into its agent, once that agent
/// has ended unfinished. A running agent joins its own.
fn join_of(checkout: &CheckoutView) -> Option<Join> {
    match &checkout.owner {
        CheckoutOwner::Subagent {
            parent,
            parent_id,
            topic: Some(topic),
            joinable: true,
        } => Some(Join {
            parent_id: parent_id.clone(),
            parent: parent.clone(),
            topic: topic.clone(),
        }),
        _ => None,
    }
}

/// Whether `checkout` is the one `task` left behind: the store names the
/// task in it, or the task's record names the directory.
fn left_behind_by(task: &PreservedWork, checkout: &CheckoutView) -> bool {
    checkout.task.as_deref() == Some(task.id.as_str())
        || task.checkout.as_deref() == Some(checkout.path.as_path())
}

/// Why `task` is listed, given the checkout it left behind. A delivery
/// this client asked for and is still waiting on is drawn as delivering
/// at once, the way the tab strip's button is, rather than once the
/// record says so.
fn reason_of(
    model: &WorkspaceModel,
    task: &PreservedWork,
    checkout: Option<&CheckoutView>,
) -> Reason {
    if model.remembered.delivery_pending.contains(&task.id) {
        Reason::Delivering
    } else if !task.project_present {
        Reason::ProjectGone
    } else if checkout.and_then(join_of).is_some() {
        Reason::JoinReady
    } else {
        Reason::of(&task.state, checkout.is_some_and(|checkout| checkout.dirty))
    }
}

/// The checkout `task` left behind, as its project's last read saw it.
fn checkout_of<'a>(recent: &'a Recent, task: &PreservedWork) -> Option<&'a CheckoutView> {
    recent.view(&task.project).and_then(|view| {
        view.checkouts
            .iter()
            .find(|checkout| left_behind_by(task, checkout))
    })
}

/// The subagent checkouts waiting to be joined that no kept task names,
/// each with its project and its join: rows of their own.
fn unnamed_joins<'a>(
    recent: &'a Recent,
    kept: &'a [&PreservedWork],
) -> impl Iterator<Item = (&'a Path, &'a CheckoutView, Join)> {
    recent
        .reads
        .values()
        .filter_map(|read| read.answer.as_ref()?.as_ref())
        .flat_map(|view| {
            view.checkouts
                .iter()
                .map(move |checkout| (view.primary.as_path(), checkout))
        })
        .filter(|(_, checkout)| !kept.iter().any(|task| left_behind_by(task, checkout)))
        .filter_map(|(primary, checkout)| Some((primary, checkout, join_of(checkout)?)))
}

/// Every row in the order it is drawn and walked, narrowed to what the
/// search and the named-only default let through.
pub(super) fn recent_rows(model: &WorkspaceModel) -> Vec<RecentRow> {
    let recent = &model.recent;
    let needle = recent.filter.trim().to_lowercase();
    every_recent_row(model)
        .into_iter()
        .filter(|row| recent.show_all || row.named)
        .filter(|row| needle.is_empty() || row.matches(&needle))
        .collect()
}

/// Every row before any narrowing: what waits on the operator first, then
/// the rest, each most recently changed first.
pub(super) fn every_recent_row(model: &WorkspaceModel) -> Vec<RecentRow> {
    let recent = &model.recent;
    let kept = model.preserved_tasks();
    let mut rows: Vec<RecentRow> = kept
        .iter()
        .map(|task| {
            let checkout = checkout_of(recent, task);
            RecentRow::of_task(task, checkout, reason_of(model, task, checkout))
        })
        .collect();
    rows.extend(
        unnamed_joins(recent, &kept)
            .map(|(primary, checkout, join)| RecentRow::of_subagent(primary, checkout, join)),
    );
    rows.sort_by(|left, right| {
        right
            .reason
            .waits_on_you()
            .cmp(&left.reason.waits_on_you())
            .then_with(|| right.changed.cmp(&left.changed))
            .then_with(|| left.name.cmp(&right.name))
    });
    rows
}

/// The selected row's place in `rows`, or the first when the selection is
/// gone.
pub(super) fn selected_index(recent: &Recent, rows: &[RecentRow]) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    Some(
        recent
            .selected
            .as_ref()
            .and_then(|key| rows.iter().position(|row| &row.key == key))
            .unwrap_or(0),
    )
}

/// What the selected row offers beyond enter, for the index: the keys
/// that act here, only where they would.
pub(super) fn recent_offered(model: &WorkspaceModel) -> Vec<Action> {
    let rows = recent_rows(model);
    selected_index(&model.recent, &rows)
        .and_then(|index| rows.get(index))
        .map(|row| {
            row.offers()
                .into_iter()
                .map(|(action, _)| action)
                .filter(|action| *action != Action::Activate)
                .collect()
        })
        .unwrap_or_default()
}

/// The project directories recent reads checkouts from: every project
/// holding listed work, and every open space's, where a subagent its
/// agent left unjoined may be waiting.
pub(super) fn projects_to_read(model: &WorkspaceModel) -> Vec<PathBuf> {
    let mut projects: Vec<PathBuf> = Vec::new();
    let present = model
        .preserved_tasks()
        .into_iter()
        .filter(|task| task.project_present)
        .map(|task| task.project.clone());
    let spaces = model.session.iter().flat_map(|session| {
        session
            .workspace
            .spaces
            .iter()
            .map(|space| uze_application::slot_key(&space.root))
            .filter(|root| !model.recent.reads.contains_key(root) && !model.recent.covers(root))
    });
    for project in present.chain(spaces) {
        if !projects.contains(&project) {
            projects.push(project);
        }
    }
    projects
}

/// Whether a space rooted at `project` is open: resuming into one that is
/// not opens it.
pub(super) fn space_open_on(model: &WorkspaceModel, project: &Path) -> bool {
    model.session.as_ref().is_some_and(|session| {
        session
            .workspace
            .spaces
            .iter()
            .any(|space| uze_application::slot_key(&space.root) == project)
    })
}

/// A project's name as a row says it: the directory's own name.
pub(super) fn project_name(project: &Path) -> String {
    project.file_name().map_or_else(
        || project.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// What reading the checkouts of `project` answered; `None` outside a
/// repository.
pub(super) struct CheckoutsResolution {
    pub(super) project: PathBuf,
    /// Which read this answers (see `Remembered::checkouts_asked`): only
    /// the project's latest is kept.
    pub(super) asked: u64,
    pub(super) view: Option<CheckoutsView>,
}

pub(super) struct JoinResolution {
    pub(super) project: PathBuf,
    pub(super) join: Join,
    pub(super) answer: std::result::Result<JoinedWork, String>,
}

/// What a join came to, as the toast says it.
pub(super) fn describe_join(resolution: &JoinResolution) -> (ToastKind, String, String) {
    let Join { topic, parent, .. } = &resolution.join;
    match &resolution.answer {
        Ok(JoinedWork::Joined { commits }) => (
            ToastKind::Done,
            "joined".to_owned(),
            format!(
                "{topic} into {parent} · {commits} commit{}",
                if *commits == 1 { "" } else { "s" }
            ),
        ),
        Ok(JoinedWork::Conflicted { checkout, paths }) => (
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
        Err(reason) => (
            ToastKind::Failed,
            "not joined".to_owned(),
            format!("{topic}: {reason}"),
        ),
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

/// `4m`, `3h`, `2d`, `5w`: how long ago a row last changed.
pub(super) fn age_to_say(changed: Option<SystemTime>) -> String {
    let Some(seconds) = changed
        .and_then(|changed| changed.elapsed().ok())
        .map(|elapsed| elapsed.as_secs())
    else {
        return String::new();
    };
    match seconds {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        86400..604800 => format!("{}d", seconds / 86400),
        _ => format!("{}w", seconds / 604800),
    }
}
