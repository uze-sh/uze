//! The review as the host hosts it: one view, opened on the checkout in
//! front, its reads handed over as jobs, and a door on the tab strip that
//! says what the branch adds and what waits on the operator, kept current
//! whether or not the view is open.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::change::{self, Change};
use super::git::{self, Base, FileDiff, Listing, Totals};
use super::record::ReviewRecord;
use super::surface::{Focus, Sending};
use super::{Asked, CATALOG, ReviewView, handle_command, handle_hit, handle_scroll, outcome};
use crate::contract::{
    AgentAsk, AgentHappened, AgentRequest, AgentWritten, Checkout, DoorBadge, Entry, Extension,
    Job, Lanes, Opaque, Outcome, SurfaceInfo, SurfaceSlot,
};
use crate::view::{
    Command, Role, ScrollDirection, ScrollTarget, Section, Size, Span, View, ViewHit,
};

/// The full-frame surface.
pub const SURFACE: &str = "review";
/// The door that opens it.
pub const DOOR: &str = "review";
/// The action a send is asked under, which the bridge's answer names.
const SEND_ACTION: &str = "review-send";
/// The bar's item: what waits in the review of the checkout in front.
pub const ITEM: &str = "review-item";
/// The drawer the item opens: the comments, and their send.
pub const COMMENTS: &str = "review-comments";

/// How often the open review reads its list and record again.
const LIST_EVERY: Duration = Duration::from_millis(1500);
/// How often the door reads its counts and the record behind them.
const DOOR_EVERY: Duration = Duration::from_millis(2000);
/// How many times its own duration a periodic read waits before the next:
/// Git gets at most a fifth of the clock, however large the checkout. On a
/// checkout whose `status` takes seconds, reading back to back would be a
/// core spent on Git for as long as the workspace is open.
const PACE: u32 = 5;

/// When a read that took `took` is next due, every `every` at the least.
fn paced(every: Duration, took: Duration) -> Duration {
    every.max(took * PACE)
}

/// What the door last learned about the checkout in front.
#[derive(Default)]
struct DoorState {
    root: Option<PathBuf>,
    totals: Option<Totals>,
    base: Base,
    at: Option<Instant>,
    reading: bool,
    /// How long the last read of it took.
    took: Duration,
}

#[derive(Default)]
pub struct ReviewExtension {
    open: Option<ReviewView>,
    focus: Option<Checkout>,
    door: DoorState,
    /// Each checkout's review as last read or written, kept per checkout
    /// so the one in front never shows another's, and an agent come back
    /// to shows its comments before its read lands.
    records: std::collections::HashMap<PathBuf, ReviewRecord>,
    /// Each open agent, by the checkout it works in.
    agents: Vec<(String, PathBuf)>,
    sending: Option<(PathBuf, Sending)>,
    /// What the drawer last had to say about a send (refused, declined),
    /// and the checkout it was said of: another checkout's drawer never
    /// shows it.
    drawer_notice: Option<(PathBuf, Span)>,
    /// A comment a press in the drawer asked the review to open on.
    open_on: Option<u64>,
    /// The agent a send was typed into, until its turn finishes or it is
    /// gone: the turn's end is when its answers are read, without waiting
    /// for the next pass.
    working: Option<String>,
    listing: bool,
    listed_at: Option<Instant>,
    /// How long the last list read took.
    list_took: Duration,
    /// Changes made on a review that closed before they were written, by
    /// checkout: written all the same, and handed back if it opens first.
    waiting: Vec<(PathBuf, Vec<Change>)>,
    /// The changes a write is out for, put back if it fails.
    in_flight: Option<(PathBuf, Vec<Change>)>,
    /// A write that failed is tried again from here, not on every pass.
    retry_at: Option<Instant>,
    /// How many writes have been answered.
    generation: u64,
    /// A failed write was said: the retries after it say nothing more
    /// until one succeeds.
    failure_told: bool,
    reading_diff: bool,
    writing: bool,
    /// What the next open is for: a return puts back what was held.
    held: Option<Held>,
    /// Places kept per checkout.
    places: std::collections::BTreeMap<PathBuf, Place>,
    /// The operator turned off marking a file viewed as it is left.
    marks_by_hand_only: bool,
    /// Each checkout's files unmarked by hand, kept past the review
    /// closing, as another agent coming to the front closes it too.
    unmarked: std::collections::BTreeMap<PathBuf, std::collections::BTreeMap<PathBuf, u64>>,
}

/// Where the viewer was on a checkout's review.
#[derive(Clone, Debug, Default)]
struct Place {
    selected: Option<PathBuf>,
    filter: super::Filter,
}

/// What a return puts back: the place, and what was being done there.
#[derive(Clone, Debug)]
struct Held {
    place: Place,
    scroll: usize,
    cursor: usize,
    mark: Option<(usize, usize)>,
    excerpt: String,
    composer: Option<super::compose::Composer>,
    focus: Focus,
}

struct ListRead {
    took: Duration,
    /// How many writes had been answered when it was asked: a record read
    /// before a write landed is older than what is on screen.
    generation: u64,
    root: PathBuf,
    filter: super::Filter,
    base: Base,
    branch: String,
    listing: Result<Listing, String>,
    record: Result<ReviewRecord, String>,
}

struct DiffRead {
    root: PathBuf,
    path: PathBuf,
    diff: Result<FileDiff, String>,
    theme: String,
}

struct DoorRead {
    took: Duration,
    /// How many writes had been answered when it was asked.
    generation: u64,
    root: PathBuf,
    base: Base,
    totals: Option<Totals>,
    /// The record as read, an error being a checkout that keeps none;
    /// `None` when nothing was read.
    record: Option<Result<ReviewRecord, String>>,
}

struct Written {
    root: PathBuf,
    record: Result<ReviewRecord, String>,
}

struct Restored {
    root: PathBuf,
    path: PathBuf,
    outcome: Result<(), String>,
}

impl ReviewExtension {
    pub fn review_view(&self) -> Option<&ReviewView> {
        self.open.as_ref()
    }

    pub fn review_view_mut(&mut self) -> Option<&mut ReviewView> {
        self.open.as_mut()
    }

    fn view_mut(&mut self, surface: &str) -> Option<&mut ReviewView> {
        (surface == SURFACE).then_some(self.open.as_mut()).flatten()
    }

    /// The one agent working in `root`. Several have no checkout of their
    /// own, and the review is kept for none of them.
    fn agent_of(&self, root: &Path) -> Option<String> {
        let mut here = self.agents.iter().filter(|(_, checkout)| checkout == root);
        match (here.next(), here.next()) {
            (Some((id, _)), None) => Some(id.clone()),
            _ => None,
        }
    }

    fn act(&mut self, asked: Asked) -> Outcome {
        match asked {
            Asked::Send => self.send(),
            Asked::OpenInCode => self.open_in_code(),
            Asked::OpenFile => self.open_file(),
            asked => outcome(asked),
        }
    }

    /// A press in the drawer: the send, or a comment, which the review
    /// opens on, in its file, with its lines marked.
    fn drawer_hit(&mut self, hit: ViewHit, _space: Size) -> Outcome {
        let ViewHit::Node { node, act } = hit else {
            return Outcome::Stay;
        };
        match act {
            crate::view::NodeAct::Press if node == super::drawer::SEND => self.send(),
            crate::view::NodeAct::Row(row) => {
                let Some(id) = self
                    .record_in_front()
                    .and_then(|(_, record)| super::drawer::comment_at(record, node, row))
                else {
                    return Outcome::Stay;
                };
                let in_front = self.focus.as_ref().map(|focus| &focus.root);
                match self
                    .open
                    .as_mut()
                    .filter(|view| Some(&view.root) == in_front)
                {
                    Some(view) => {
                        view.go_to_thread_id(id);
                        Outcome::Show(SURFACE.to_owned())
                    }
                    None => {
                        self.open_on = Some(id);
                        Outcome::Show(SURFACE.to_owned())
                    }
                }
            }
            _ => Outcome::Stay,
        }
    }

    /// The checkout in front and its review as it stands: the view's,
    /// open on it, else what the door last read.
    fn record_in_front(&self) -> Option<(&Path, &ReviewRecord)> {
        let root = &self.focus.as_ref()?.root;
        match &self.open {
            Some(view) if &view.root == root => Some((root, &view.record)),
            _ => self
                .records
                .get(root)
                .map(|record| (root.as_path(), record)),
        }
    }

    /// Says `span` of `root`'s send where it was asked: its review, when
    /// that is open, and its drawer.
    fn say(&mut self, root: &Path, span: Span) {
        if let Some(view) = self.open.as_mut().filter(|view| view.root == root) {
            view.notice = Some(span.clone());
        }
        self.drawer_notice = Some((root.to_path_buf(), span));
    }

    /// Asks the bridge to tell the agent of the checkout in front that its
    /// drafts wait for it, from the review or from the drawer, open or
    /// not. The drafts become a send only once the bridge says the line
    /// was typed.
    fn send(&mut self) -> Outcome {
        let Some((root, record)) = self.record_in_front() else {
            return Outcome::Stay;
        };
        let root = root.to_path_buf();
        let ids: Vec<u64> = record
            .comments
            .iter()
            .filter(|comment| comment.status == super::CommentStatus::Draft)
            .map(|comment| comment.id)
            .collect();
        let next_send = record.next_send.max(1);
        if ids.is_empty() {
            self.say(&root, Span::new("no drafts to send", Role::Muted));
            return Outcome::Stay;
        }
        let Some(agent) = self.agent_of(&root) else {
            self.say(
                &root,
                Span::new(
                    "no agent UZE launched works in this checkout",
                    Role::Warning,
                ),
            );
            return Outcome::Stay;
        };
        if self.sending.is_some() {
            self.say(
                &root,
                Span::new("a send is waiting for your answer", Role::Muted),
            );
            return Outcome::Stay;
        }
        self.drawer_notice = None;
        let line = change::send_line(ids.len(), next_send);
        let summary = change::send_summary(ids.len());
        self.sending = Some((
            root,
            Sending {
                ids,
                agent: agent.clone(),
            },
        ));
        Outcome::Agent(Box::new(AgentRequest {
            what: AgentAsk::Prompt { text: line },
            agent: Some(agent),
            action: SEND_ACTION.to_owned(),
            label: "Send the review".to_owned(),
            unattended: false,
            summary: Some(summary),
        }))
    }

    /// The selected file in the code surface, from its top.
    fn open_file(&self) -> Outcome {
        let Some(view) = self.open.as_ref() else {
            return Outcome::Stay;
        };
        match view.selected.clone() {
            Some(target) => Outcome::OpenPath {
                project: view.root.clone(),
                target,
                line: None,
            },
            None => Outcome::Stay,
        }
    }

    /// The marked line, or the cursor's, in the code surface: the first
    /// marked line that still exists in the new file.
    fn open_in_code(&mut self) -> Outcome {
        let Some(view) = self.open.as_ref() else {
            return Outcome::Stay;
        };
        let Some(path) = view.selected.clone() else {
            return Outcome::Stay;
        };
        let cells = view.cells();
        let from = view.marked().map_or(view.cursor, |(from, _)| from);
        let line = cells
            .iter()
            .skip(from)
            .find(|cell| cell.kind != super::diff::DiffLineKind::Removed)
            .or_else(|| {
                cells[..from.min(cells.len())]
                    .iter()
                    .rev()
                    .find(|cell| cell.kind != super::diff::DiffLineKind::Removed)
            })
            .map(|cell| cell.line_no);
        Outcome::OpenPath {
            project: view.root.clone(),
            target: path,
            line,
        }
    }

    fn list_job(&mut self, jobs: &mut Vec<Job>) {
        if self.listing {
            return;
        }
        let Some(view) = self.open.as_mut() else {
            return;
        };
        let every = paced(LIST_EVERY, self.list_took);
        let due = view.relist || self.listed_at.is_none_or(|at| at.elapsed() >= every);
        if !due {
            return;
        }
        view.relist = false;
        self.listing = true;
        self.listed_at = Some(Instant::now());
        let root = view.root.clone();
        let filter = view.filter;
        let known = view.base.clone();
        let target = self.focus.as_ref().and_then(|focus| focus.target.clone());
        let generation = self.generation;
        let fallback_root = root.clone();
        jobs.push(Job {
            run: Box::new(move |host| {
                let started = Instant::now();
                let base = match filter {
                    super::Filter::All => git::base(host, &root, target.as_deref(), &known),
                    super::Filter::Uncommitted => Base::default(),
                };
                let listing = git::listing(host, &root, base.commit.as_deref());
                let record = host.review_record(&root);
                Opaque::new(ListRead {
                    took: started.elapsed(),
                    record,
                    generation,
                    branch: crate::shared::checkout::branch_of(host, &root),
                    root,
                    filter,
                    base,
                    listing,
                })
            }),
            fallback: Box::new(move || {
                Opaque::new(ListRead {
                    took: Duration::ZERO,
                    generation,
                    root: fallback_root,
                    filter,
                    base: Base::default(),
                    branch: String::new(),
                    listing: Err("reading what changed failed".to_owned()),
                    record: Err("reading the review failed".to_owned()),
                })
            }),
        });
    }

    fn diff_job(&mut self, jobs: &mut Vec<Job>) {
        if self.reading_diff {
            return;
        }
        let Some(view) = self.open.as_mut().filter(|view| view.diff_pending) else {
            return;
        };
        let Some(file) = view.selected_file().cloned() else {
            return;
        };
        self.reading_diff = true;
        view.asking = view
            .diff_key(&file.path)
            .map(|key| (file.path.clone(), key));
        let root = view.root.clone();
        let base = match view.filter {
            super::Filter::All => view.base.commit.clone(),
            super::Filter::Uncommitted => None,
        };
        let fallback = (root.clone(), file.path.clone());
        jobs.push(Job {
            run: Box::new(move |host| {
                let diff = git::file_diff(host, &root, base.as_deref(), &file.path, file.status);
                Opaque::new(DiffRead {
                    theme: host.syntax_theme(),
                    root,
                    path: file.path,
                    diff,
                })
            }),
            fallback: Box::new(move || {
                Opaque::new(DiffRead {
                    root: fallback.0,
                    path: fallback.1,
                    diff: Err("reading the diff failed".to_owned()),
                    theme: String::new(),
                })
            }),
        });
    }

    fn write_job(&mut self, jobs: &mut Vec<Job>) {
        if self.writing || self.retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        let from_view = self
            .open
            .as_mut()
            .filter(|view| !view.unwritten.is_empty())
            .map(|view| (view.root.clone(), std::mem::take(&mut view.unwritten)));
        let Some((root, changes)) =
            from_view.or_else(|| (!self.waiting.is_empty()).then(|| self.waiting.remove(0)))
        else {
            return;
        };
        self.writing = true;
        self.in_flight = Some((root.clone(), changes.clone()));
        let fallback_root = root.clone();
        jobs.push(Job {
            run: Box::new(move |host| {
                let record = host.update_review_record(&root, &mut |record| {
                    for change in &changes {
                        change.apply(record);
                    }
                });
                Opaque::new(Written { root, record })
            }),
            fallback: Box::new(move || {
                Opaque::new(Written {
                    root: fallback_root,
                    record: Err("writing the review failed".to_owned()),
                })
            }),
        });
    }

    fn restore_job(&mut self, jobs: &mut Vec<Job>) {
        let Some(view) = self.open.as_mut() else {
            return;
        };
        let Some(paths) = view.restore.take() else {
            return;
        };
        let Some(path) = paths.first().cloned() else {
            return;
        };
        let root = view.root.clone();
        let fallback = (root.clone(), path.clone());
        jobs.push(Job {
            run: Box::new(move |host| {
                let outcome = host.restore_to_head(&root, &paths);
                Opaque::new(Restored {
                    root,
                    path,
                    outcome,
                })
            }),
            fallback: Box::new(move || {
                Opaque::new(Restored {
                    root: fallback.0,
                    path: fallback.1,
                    outcome: Err("discarding failed".to_owned()),
                })
            }),
        });
    }

    fn door_job(&mut self, jobs: &mut Vec<Job>) {
        if self.door.reading {
            return;
        }
        let Some(focus) = self.focus.clone() else {
            return;
        };
        let fresh = self.door.root.as_ref() == Some(&focus.root)
            && self
                .door
                .at
                .is_some_and(|at| at.elapsed() < paced(DOOR_EVERY, self.door.took));
        if fresh {
            return;
        }
        self.door.reading = true;
        let known = match self.door.root.as_ref() == Some(&focus.root) {
            true => self.door.base.clone(),
            false => Base::default(),
        };
        let root = focus.root.clone();
        let fallback_root = root.clone();
        let generation = self.generation;
        jobs.push(Job {
            run: Box::new(move |host| {
                let started = Instant::now();
                let base = git::base(host, &root, focus.target.as_deref(), &known);
                let totals = git::totals(host, &root, base.commit.as_deref());
                Opaque::new(DoorRead {
                    took: started.elapsed(),
                    totals,
                    record: Some(host.review_record(&root)),
                    generation,
                    root,
                    base,
                })
            }),
            fallback: Box::new(move || {
                Opaque::new(DoorRead {
                    took: Duration::ZERO,
                    generation,
                    root: fallback_root,
                    base: Base::default(),
                    totals: None,
                    record: None,
                })
            }),
        });
    }

    /// What a read of the door would have found for `root`: how the
    /// host's tests stand in for Git.
    pub fn counted(&mut self, root: PathBuf, additions: u32, deletions: u32, files: usize) {
        self.door.root = Some(root);
        self.door.totals = Some(Totals {
            additions,
            deletions,
            files,
        });
        self.door.at = Some(Instant::now());
    }

    /// A draft on `path`, as the door's read of the record at `root`, in
    /// front, would have found it: the client's tests' stand-in for a
    /// record on disk and a pass that focused it.
    pub fn drafted(&mut self, root: PathBuf, path: &str, text: &str) {
        self.focus = Some(Checkout {
            display: root.display().to_string(),
            root: root.clone(),
            target: None,
        });
        self.door.root = Some(root.clone());
        let record = self.records.entry(root).or_default();
        Change::Draft {
            id: record.free_id(),
            path: path.to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: text.to_owned(),
        }
        .apply(record);
    }

    /// The review open on `root` with `files` listed, each with its diff
    /// read from `diffs` (Git's text), the first selected and on show: the
    /// client's tests' stand-in for Git, as `counted` is for the door.
    #[doc(hidden)]
    pub fn showing(&mut self, root: PathBuf, diffs: &[(&str, &str)]) {
        let files: Vec<crate::shared::git_status::ChangedFile> = diffs
            .iter()
            .map(|(path, _)| crate::shared::git_status::ChangedFile {
                status: crate::shared::git_status::FileStatus::Modified,
                path: root.join(path),
                renamed_from: None,
            })
            .collect();
        let mut view = ReviewView::opening(root.clone(), root.display().to_string());
        view.absorb_listing(
            super::Filter::All,
            Base::default(),
            String::new(),
            Ok(Listing {
                files,
                uncommitted: Default::default(),
                partners: Default::default(),
                stamps: Default::default(),
                head: None,
            }),
        );
        if let Some((path, text)) = diffs.first() {
            let path = root.join(path);
            let diff = FileDiff {
                cells: super::diff::read(
                    text,
                    &path,
                    crate::shared::highlight::FALLBACK_SYNTAX_THEME,
                ),
                fingerprint: git::fingerprint(text),
                binary: false,
                picture: None,
                path: path.clone(),
            };
            view.absorb_diff(path, Ok(diff), String::new());
        }
        self.listing = true;
        self.listed_at = Some(Instant::now());
        self.open = Some(view);
    }

    /// Lets a failed write be tried again at once, as the clock would.
    #[cfg(test)]
    pub(crate) fn retry_now(&mut self) {
        self.retry_at = None;
    }

    /// The changes a write is out for on `root`, if one is.
    /// Keeps `record` as `root`'s, with what is still on its way to disk
    /// laid over it: a read or a write answered before a change landed
    /// never takes that change away from the bar or the drawer.
    fn know(&mut self, root: PathBuf, mut record: ReviewRecord) {
        let pending = self.in_flight_for(&root).into_iter().chain(
            self.waiting
                .iter()
                .filter(|(waiting, _)| *waiting == root)
                .flat_map(|(_, changes)| changes.clone()),
        );
        for change in pending {
            change.apply(&mut record);
        }
        self.records.insert(root, record);
    }

    fn in_flight_for(&self, root: &Path) -> Vec<Change> {
        self.in_flight
            .as_ref()
            .filter(|(written, _)| written == root)
            .map(|(_, changes)| changes.clone())
            .unwrap_or_default()
    }
}

impl Extension for ReviewExtension {
    fn identity(&self) -> String {
        CATALOG.id.to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        vec![
            SurfaceInfo {
                id: SURFACE.to_owned(),
                slot: SurfaceSlot::View,
                title: CATALOG.name.to_owned(),
                opens: None,
            },
            // What waits in the review rides the bar wherever the operator
            // is, so a draft is never left unsent for want of the review
            // being open; it opens the comments beside the pane.
            SurfaceInfo {
                id: ITEM.to_owned(),
                slot: SurfaceSlot::Bar { right: true },
                title: "Comments".to_owned(),
                opens: Some(COMMENTS.to_owned()),
            },
            SurfaceInfo {
                id: COMMENTS.to_owned(),
                slot: SurfaceSlot::Drawer(uze_contract::DrawerDecl {
                    side: uze_contract::DrawerSide::Right,
                    float: false,
                    fit: false,
                }),
                title: "Comments".to_owned(),
                opens: None,
            },
        ]
    }

    fn view(&self, surface: &str, space: Size) -> Option<View> {
        match surface {
            SURFACE => self.open.as_ref().map(|view| super::view(view, space)),
            ITEM => super::drawer::item(self.record_in_front()?.1),
            COMMENTS => {
                let (root, record) = self.record_in_front()?;
                Some(super::drawer::drawer(
                    record,
                    self.agent_of(root).is_some(),
                    self.drawer_notice
                        .as_ref()
                        .filter(|(said, _)| said == root)
                        .map(|(_, span)| span),
                ))
            }
            _ => None,
        }
    }

    fn section(&self, _surface: &str, _collapsed: bool) -> Option<Section> {
        None
    }

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome {
        let Some(view) = self.view_mut(surface) else {
            return Outcome::Stay;
        };
        view.note_seen(space);
        let asked = handle_command(view, command, space);
        view.note_seen(space);
        self.act(asked)
    }

    fn hit(&mut self, surface: &str, hit: ViewHit, space: Size) -> Outcome {
        if surface == COMMENTS {
            return self.drawer_hit(hit, space);
        }
        let Some(view) = self.view_mut(surface) else {
            return Outcome::Stay;
        };
        view.note_seen(space);
        let asked = handle_hit(view, hit, space);
        view.note_seen(space);
        self.act(asked)
    }

    fn hover(&mut self, surface: &str, hit: Option<ViewHit>) -> bool {
        self.view_mut(surface)
            .is_some_and(|view| super::hover(view, hit))
    }

    fn scroll(
        &mut self,
        surface: &str,
        target: ScrollTarget,
        direction: ScrollDirection,
        space: Size,
    ) {
        if let Some(view) = self.view_mut(surface) {
            // The list is the host's to scroll: the wheel never moves
            // the selection.
            if target == ScrollTarget::Content {
                view.note_seen(space);
                handle_scroll(view, direction, false);
                view.note_seen(space);
            }
        }
    }

    fn enter(&mut self, surface: &str, entry: Entry) -> Outcome {
        if surface != SURFACE {
            return Outcome::Stay;
        }
        match entry {
            // Pressed while it stands, its door shuts it.
            Entry::Door(_) if self.open.is_some() => Outcome::Close,
            Entry::Door(_) | Entry::Path { .. } => {
                self.held = None;
                Outcome::Stay
            }
            Entry::Return => Outcome::Stay,
        }
    }

    fn open(&mut self, surface: &str, checkout: &Checkout) {
        if surface != SURFACE {
            return;
        }
        let mut view = ReviewView::opening(checkout.root.clone(), checkout.display.clone());
        if let Some(place) = self.places.get(&checkout.root) {
            view.selected = place.selected.clone();
            view.filter = place.filter;
        }
        // Changes still waiting to be written are this view's again, so
        // every record it reads is shown with them laid over it.
        let (mine, others): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(root, _)| *root == checkout.root);
        self.waiting = others;
        view.unwritten = mine.into_iter().flat_map(|(_, changes)| changes).collect();
        if let Some(record) = self.records.get(&checkout.root) {
            let in_flight = self.in_flight_for(&checkout.root);
            view.absorb_record(Ok(record.clone()), &in_flight);
        }
        view.open_on = self.open_on.take();
        view.marks_by_hand_only = self.marks_by_hand_only;
        view.unmarked = self.unmarked.remove(&checkout.root).unwrap_or_default();
        self.listed_at = None;
        self.open = Some(view);
    }

    fn configure(&mut self, preferences: &serde_json::Value) {
        self.marks_by_hand_only = preferences
            .get(super::MARK_VIEWED_AUTOMATICALLY)
            .and_then(serde_json::Value::as_bool)
            .is_some_and(|automatic| !automatic);
        if let Some(view) = &mut self.open {
            view.marks_by_hand_only = self.marks_by_hand_only;
        }
    }

    fn close(&mut self, surface: &str) {
        if surface != SURFACE {
            return;
        }
        if let Some(mut view) = self.open.take() {
            view.leave(Instant::now());
            self.unmarked
                .insert(view.root.clone(), std::mem::take(&mut view.unmarked));
            self.places.insert(
                view.root.clone(),
                Place {
                    selected: view.selected.clone(),
                    filter: view.filter,
                },
            );
            // What was changed and not yet written is still written: the
            // operator's work never waits on the surface staying open.
            // The door says what the surface last showed; laid again over
            // it, a change waiting to be written is a repeat.
            self.records.insert(view.root.clone(), view.record.clone());
            if !view.unwritten.is_empty() {
                self.waiting.push((view.root.clone(), view.unwritten));
            }
        }
    }

    fn place(&self, surface: &str) -> Option<Opaque> {
        let view = self.open.as_ref().filter(|_| surface == SURFACE)?;
        Some(Opaque::new(Place {
            selected: view.selected.clone(),
            filter: view.filter,
        }))
    }

    fn hold(&self, surface: &str) -> Option<Opaque> {
        let view = self.open.as_ref().filter(|_| surface == SURFACE)?;
        Some(Opaque::new(Held {
            place: Place {
                selected: view.selected.clone(),
                filter: view.filter,
            },
            scroll: view.scroll,
            cursor: view.cursor,
            mark: view.mark,
            excerpt: view.excerpt.clone(),
            composer: view.composer.clone(),
            focus: view.focus,
        }))
    }

    fn resume(&mut self, surface: &str, place: Opaque) {
        let Some(view) = self.view_mut(surface) else {
            return;
        };
        let place = if place.0.is::<Held>() {
            let Some(held) = place.downcast::<Held>() else {
                return;
            };
            view.scroll = held.scroll;
            view.cursor = held.cursor;
            view.mark = held.mark;
            view.excerpt = held.excerpt;
            view.composer = held.composer;
            view.focus = held.focus;
            held.place
        } else {
            match place.downcast::<Place>() {
                Some(place) => place,
                None => return,
            }
        };
        view.selected = place.selected;
        view.filter = place.filter;
        view.diff_pending = view.selected.is_some();
    }

    fn typing(&self, surface: &str) -> bool {
        surface == SURFACE && self.open.as_ref().is_some_and(ReviewView::typing)
    }

    fn paste(&mut self, surface: &str, text: &str, _space: Size) {
        if let Some(composer) = self
            .view_mut(surface)
            .and_then(|view| view.composer.as_mut())
        {
            composer.insert(text);
        }
    }

    fn text(&self, surface: &str, lines: std::ops::Range<usize>) -> Vec<String> {
        match self.open.as_ref().filter(|_| surface == SURFACE) {
            Some(view) => super::text(view, lines),
            None => Vec::new(),
        }
    }

    fn selected(&mut self, surface: &str, lines: std::ops::Range<usize>, text: &str) {
        if let Some(view) = self.view_mut(surface) {
            super::selected(view, lines, text);
        }
    }

    fn focus(&mut self, checkout: Option<&Checkout>) {
        let moved =
            self.focus.as_ref().map(|focus| &focus.root) != checkout.map(|checkout| &checkout.root);
        self.focus = checkout.cloned();
        if moved {
            self.door.at = None;
        }
    }

    fn agents(&mut self, agents: &serde_json::Value) {
        let before = std::mem::take(&mut self.agents);
        self.agents = agents
            .as_array()
            .map(|agents| {
                agents
                    .iter()
                    .filter_map(|agent| {
                        Some((
                            agent.get("id")?.as_str()?.to_owned(),
                            PathBuf::from(agent.get("checkout")?.as_str()?),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // A review is kept for the agent working in a checkout: another
        // come to work there, or none, and what was kept is not theirs.
        let workers = |agents: &[(String, PathBuf)], root: &PathBuf| -> Vec<String> {
            agents
                .iter()
                .filter(|(_, checkout)| checkout == root)
                .map(|(id, _)| id.clone())
                .collect()
        };
        let moved: Vec<PathBuf> = before
            .iter()
            .map(|(_, root)| root.clone())
            .filter(|root| workers(&before, root) != workers(&self.agents, root))
            .collect();
        for root in moved {
            self.records.remove(&root);
        }
        // An agent gone takes the turn it was on with it.
        if let Some(agent) = &self.working
            && !self.agents.iter().any(|(id, _)| id == agent)
        {
            self.working = None;
        }
    }

    fn agent_events(&mut self, happened: &[AgentHappened], _now: Instant) {
        let finished = happened.iter().any(|event| {
            self.working.as_ref() == Some(&event.agent)
                && event.event == uze_contract::AgentEvent::TurnFinished
        });
        if finished {
            self.working = None;
            self.door.at = None;
            self.listed_at = None;
        }
    }

    fn agent_write(&mut self, action: &str, written: AgentWritten) {
        if action != SEND_ACTION {
            return;
        }
        let Some((root, sending)) = self.sending.take() else {
            return;
        };
        let open = self.open.as_mut().filter(|view| view.root == root);
        if written == AgentWritten::Typed {
            let sent = Change::Sent {
                ids: sending.ids,
                agent: sending.agent.clone(),
            };
            // Typed is typed: the drafts are sent whether or not the review
            // still stands to show it.
            // The door's record says it at once too, for the drawer and
            // the bar, open review or not.
            if let Some(record) = self.records.get_mut(&root) {
                sent.apply(record);
            }
            match open {
                Some(view) => view.change(sent),
                None => self.waiting.push((root.clone(), vec![sent])),
            }
            self.working = Some(sending.agent);
            return;
        }
        match written {
            AgentWritten::Typed => {}
            AgentWritten::Refused(reason) => {
                self.say(
                    &root,
                    Span::new(format!("not sent: {reason}"), Role::Warning),
                );
            }
            AgentWritten::Declined => self.say(&root, Span::new("not sent", Role::Muted)),
        }
    }

    fn jobs(&mut self) -> Vec<Job> {
        let mut jobs = Vec::new();
        self.door_job(&mut jobs);
        self.list_job(&mut jobs);
        self.diff_job(&mut jobs);
        self.write_job(&mut jobs);
        self.restore_job(&mut jobs);
        jobs
    }

    fn lanes(&self) -> Lanes {
        Lanes::Parallel
    }

    fn absorb(&mut self, answer: Opaque) -> Outcome {
        if answer.0.is::<ListRead>() {
            if let Some(read) = answer.downcast::<ListRead>() {
                self.listing = false;
                self.list_took = read.took;
                let in_flight = self.in_flight_for(&read.root);
                let current = read.generation == self.generation;
                if let Some(view) = self.open.as_mut().filter(|view| view.root == read.root) {
                    view.absorb_listing(read.filter, read.base, read.branch, read.listing);
                    if current {
                        view.absorb_record(read.record, &in_flight);
                    }
                }
            }
            return Outcome::Stay;
        }
        if answer.0.is::<DiffRead>() {
            if let Some(read) = answer.downcast::<DiffRead>() {
                self.reading_diff = false;
                if let Some(view) = self.open.as_mut().filter(|view| view.root == read.root) {
                    view.absorb_diff(read.path, read.diff, read.theme);
                }
            }
            return Outcome::Stay;
        }
        if answer.0.is::<DoorRead>() {
            if let Some(read) = answer.downcast::<DoorRead>() {
                self.door.reading = false;
                self.door.took = read.took;
                if self.focus.as_ref().map(|focus| &focus.root) == Some(&read.root) {
                    self.door.root = Some(read.root.clone());
                    self.door.base = read.base;
                    self.door.totals = read.totals;
                    self.door.at = Some(Instant::now());
                }
                match read.record {
                    // Older than the last write answered: not taken, and
                    // asked again on the next pass rather than a period on.
                    Some(_) if read.generation != self.generation => {
                        if self.door.root.as_ref() == Some(&read.root) {
                            self.door.at = None;
                        }
                    }
                    Some(Ok(record)) => self.know(read.root, record),
                    // A checkout never read that answers none keeps no
                    // review (a shell, a shared checkout); one read before
                    // keeps what it held through a read that failed.
                    Some(Err(_)) => {
                        self.records.entry(read.root).or_default();
                    }
                    None => {}
                }
            }
            return Outcome::Stay;
        }
        if answer.0.is::<Written>() {
            let Some(written) = answer.downcast::<Written>() else {
                return Outcome::Stay;
            };
            self.writing = false;
            self.generation += 1;
            let in_flight = self.in_flight.take();
            match &written.record {
                Ok(_) => {
                    self.retry_at = None;
                    self.failure_told = false;
                }
                // Put back, to be tried again on the list's cadence: a
                // failed write must not drop what the operator wrote.
                Err(_) => {
                    self.retry_at = Some(Instant::now() + LIST_EVERY);
                    if let Some((root, changes)) = in_flight {
                        match self.open.as_mut().filter(|view| view.root == root) {
                            Some(view) => {
                                let newer = std::mem::take(&mut view.unwritten);
                                view.unwritten = changes.into_iter().chain(newer).collect();
                            }
                            None => self.waiting.insert(0, (root, changes)),
                        }
                    }
                }
            }
            if let Ok(record) = &written.record {
                self.know(written.root.clone(), record.clone());
            }
            match self.open.as_mut().filter(|view| view.root == written.root) {
                Some(view) => view.absorb_record(written.record, &[]),
                None => {
                    if let Err(error) = written.record
                        && !std::mem::replace(&mut self.failure_told, true)
                    {
                        return Outcome::Failed {
                            title: "the review was not kept".to_owned(),
                            detail: error,
                        };
                    }
                }
            }
            return Outcome::Stay;
        }
        let Some(restored) = answer.downcast::<Restored>() else {
            return Outcome::Stay;
        };
        let name = restored
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match self.open.as_mut().filter(|view| view.root == restored.root) {
            Some(view) => {
                view.relist = true;
                view.notice = Some(match restored.outcome {
                    Ok(()) => Span::new(format!("{name} discarded"), Role::Muted),
                    Err(error) => Span::new(format!("{name}: {error}"), Role::Danger),
                });
            }
            None => {
                if let Err(error) = restored.outcome {
                    return Outcome::Failed {
                        title: "discard failed".to_owned(),
                        detail: format!("{name}: {error}"),
                    };
                }
            }
        }
        Outcome::Stay
    }

    /// Told whom a send goes to and when it was taken up.
    fn observes_agents(&self) -> bool {
        true
    }

    /// The branch's size against its target, and nothing more: what waits
    /// in the review is the review's to say, inside it. A checkout with
    /// nothing changed has no door: there is nothing to review.
    fn door_badge(&self) -> Option<DoorBadge> {
        let totals = self.door.totals.filter(|totals| totals.files > 0)?;
        Some(DoorBadge {
            spans: vec![
                Span::new(format!("+{}", grouped(totals.additions)), Role::Success),
                Span::new(" ", Role::Default),
                Span::new(format!("-{}", grouped(totals.deletions)), Role::Danger),
            ],
        })
    }
}

/// `n` with its thousands set apart, so a large branch's count is read at
/// a glance: `14,714`.
fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{Base, DoorRead, ReviewExtension, ReviewRecord};
    use crate::contract::{AgentWritten, Checkout, Extension, Opaque};

    fn checkout(root: &str) -> Checkout {
        Checkout {
            root: PathBuf::from(root),
            display: root.to_owned(),
            target: None,
        }
    }

    #[test]
    fn the_preference_reaches_the_review_open_now_and_every_one_opened_after() {
        let mut extension = ReviewExtension::default();
        extension.open(super::SURFACE, &checkout("/w"));
        assert!(
            !extension.review_view().unwrap().marks_by_hand_only,
            "on by default"
        );

        extension
            .configure(&serde_json::json!({ crate::review::MARK_VIEWED_AUTOMATICALLY: false }));
        assert!(extension.review_view().unwrap().marks_by_hand_only);
        extension.close(super::SURFACE);
        extension.open(super::SURFACE, &checkout("/w"));
        assert!(extension.review_view().unwrap().marks_by_hand_only);

        extension.configure(&serde_json::json!({}));
        assert!(
            !extension.review_view().unwrap().marks_by_hand_only,
            "unset is the default"
        );
    }

    /// What the door's read of `root` answers, asked at `generation`.
    fn door_read(generation: u64, root: &str, record: Result<ReviewRecord, String>) -> Opaque {
        Opaque::new(DoorRead {
            took: Duration::ZERO,
            generation,
            root: PathBuf::from(root),
            base: Base::default(),
            totals: None,
            record: Some(record),
        })
    }

    fn drafts(extension: &ReviewExtension) -> usize {
        extension
            .record_in_front()
            .map(|(_, record)| {
                record
                    .comments
                    .iter()
                    .filter(|comment| comment.status == crate::review::CommentStatus::Draft)
                    .count()
            })
            .unwrap_or(0)
    }

    /// A checkout that keeps no review (a shell, a checkout two agents
    /// share) never shows the one before's: its failed read is no review.
    #[test]
    fn a_checkout_whose_review_cannot_be_read_never_shows_anothers() {
        let mut extension = ReviewExtension::default();
        extension.drafted(PathBuf::from("/a"), "a.rs", "check");
        assert_eq!(drafts(&extension), 1);
        extension.focus(Some(&checkout("/b")));
        let read = door_read(0, "/b", Err("no agent works here".to_owned()));
        extension.absorb(read);
        assert_eq!(drafts(&extension), 0, "b's, not a's");
        assert!(
            extension
                .view(super::ITEM, crate::view::Size::default())
                .is_none()
        );
        extension.focus(Some(&checkout("/a")));
        assert_eq!(drafts(&extension), 1, "a's again at once, before its read");
    }

    /// The sent agent's turn ending is when its answers are read: the
    /// door and the list are asked again at once, not on their next pass.
    #[test]
    fn the_sent_agents_turn_ending_reads_the_review_again_at_once() {
        use crate::contract::AgentHappened;
        let mut extension = ReviewExtension::default();
        extension.drafted(PathBuf::from("/w"), "a.rs", "check");
        extension.counted(PathBuf::from("/w"), 1, 0, 1);
        extension.agents(&serde_json::json!([{ "id": "a1", "checkout": "/w" }]));
        let _ = extension.send();
        extension.agent_write(super::SEND_ACTION, AgentWritten::Typed);
        extension.listed_at = Some(std::time::Instant::now());
        let finished = |agent: &str| AgentHappened {
            agent: agent.to_owned(),
            event: uze_contract::AgentEvent::TurnFinished,
            harness: String::new(),
            checkout: PathBuf::from("/w"),
            detail: serde_json::Value::Null,
        };
        extension.agent_events(&[finished("other")], std::time::Instant::now());
        assert!(
            extension.door.at.is_some(),
            "another agent's turn is not news"
        );
        extension.agent_events(&[finished("a1")], std::time::Instant::now());
        assert!(extension.door.at.is_none() && extension.listed_at.is_none());
    }

    /// A read that fails keeps what the checkout held; one older than
    /// the last write is not taken and is asked again on the next pass.
    #[test]
    fn a_failed_read_keeps_the_comments_and_an_old_one_asks_again_at_once() {
        let mut extension = ReviewExtension::default();
        extension.drafted(PathBuf::from("/w"), "a.rs", "check");
        extension.counted(PathBuf::from("/w"), 1, 0, 1);
        extension.absorb(door_read(
            extension.generation,
            "/w",
            Err("the record could not be read".to_owned()),
        ));
        assert_eq!(drafts(&extension), 1, "kept through a failed read");
        extension.generation += 1;
        extension.absorb(door_read(0, "/w", Ok(ReviewRecord::default())));
        assert_eq!(drafts(&extension), 1, "an old read is not taken");
        assert!(
            extension.door.at.is_none(),
            "and the door asks again at once"
        );
    }

    /// Another agent come to work in a checkout is not offered the one
    /// before's drafts: what was kept there is let go until its own read.
    #[test]
    fn another_agent_in_the_checkout_is_never_offered_the_one_befores_drafts() {
        let mut extension = ReviewExtension::default();
        extension.drafted(PathBuf::from("/w"), "a.rs", "check");
        extension.agents(&serde_json::json!([{ "id": "x", "checkout": "/w" }]));
        assert_eq!(drafts(&extension), 1);
        extension.agents(&serde_json::json!([{ "id": "x", "checkout": "/w" }]));
        assert_eq!(drafts(&extension), 1, "the same agent keeps its review");
        extension.agents(&serde_json::json!([{ "id": "y", "checkout": "/w" }]));
        assert_eq!(drafts(&extension), 0, "x's drafts are not y's");
    }

    /// What the drawer says of one checkout's send stays that checkout's:
    /// declined in one, another's drawer says nothing of it.
    #[test]
    fn a_send_declined_in_one_checkout_is_not_said_in_anothers_drawer() {
        let mut extension = ReviewExtension::default();
        extension.drafted(PathBuf::from("/b"), "b.rs", "check");
        extension.drafted(PathBuf::from("/a"), "a.rs", "check");
        extension.agents(&serde_json::json!([{ "id": "a1", "checkout": "/a" }]));
        let _ = extension.send();
        extension.agent_write(super::SEND_ACTION, AgentWritten::Declined);
        let said = |extension: &ReviewExtension| {
            let view = extension.view(super::COMMENTS, crate::view::Size::default());
            format!("{view:?}").contains("not sent")
        };
        assert!(said(&extension), "said where it was declined");
        extension.focus(Some(&checkout("/b")));
        assert!(!said(&extension), "and nowhere else");
    }

    /// Sent from the drawer with the review put away, the drafts stay
    /// sent though a read taken before the write landed says otherwise:
    /// a second press must not type the same send again.
    #[test]
    fn a_read_from_before_the_send_landed_never_brings_the_drafts_back() {
        let mut extension = ReviewExtension::default();
        extension.drafted(PathBuf::from("/w"), "a.rs", "check");
        extension.agents(&serde_json::json!([{ "id": "a1", "checkout": "/w" }]));
        let before = extension.generation;
        let on_disk = extension.records[&PathBuf::from("/w")].clone();
        let _ = extension.send();
        extension.agent_write(super::SEND_ACTION, AgentWritten::Typed);
        assert_eq!(drafts(&extension), 0, "sent");
        extension.absorb(door_read(before, "/w", Ok(on_disk.clone())));
        assert_eq!(drafts(&extension), 0, "an old read is not news");
        extension.absorb(door_read(extension.generation, "/w", Ok(on_disk)));
        assert_eq!(
            drafts(&extension),
            0,
            "nor a new one the write has not reached"
        );
    }

    #[test]
    fn a_slow_read_waits_five_times_its_own_length() {
        let every = Duration::from_millis(1500);
        assert_eq!(super::paced(every, Duration::from_millis(100)), every);
        assert_eq!(
            super::paced(every, Duration::from_secs(2)),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn a_count_sets_its_thousands_apart() {
        assert_eq!(super::grouped(672), "672");
        assert_eq!(super::grouped(14_714), "14,714");
        assert_eq!(super::grouped(1_000_000), "1,000,000");
    }
}
