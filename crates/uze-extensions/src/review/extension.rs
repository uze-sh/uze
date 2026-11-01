//! The review as the host hosts it: one view, opened on the checkout in
//! front, its reads handed over as jobs, and a door on the tab strip that
//! says what the branch adds and what waits on the operator, kept current
//! whether or not the view is open.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::change::{self, Change};
use super::git::{self, Base, FileDiff, Listing, Totals};
use super::record::{CommentStatus, ReviewRecord};
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

/// How often the open review reads its list and record again.
const LIST_EVERY: Duration = Duration::from_millis(1500);
/// How often the door reads its counts and the record behind them.
const DOOR_EVERY: Duration = Duration::from_millis(2000);

/// What the door last learned about the checkout in front.
#[derive(Default)]
struct DoorState {
    root: Option<PathBuf>,
    totals: Option<Totals>,
    record: ReviewRecord,
    base: Base,
    at: Option<Instant>,
    reading: bool,
}

#[derive(Default)]
pub struct ReviewExtension {
    open: Option<ReviewView>,
    focus: Option<Checkout>,
    door: DoorState,
    /// Each open agent, by the checkout it works in.
    agents: Vec<(String, PathBuf)>,
    sending: Option<(PathBuf, Sending)>,
    /// The agent a send was typed into, until its turn finishes.
    working: Option<String>,
    listing: bool,
    listed_at: Option<Instant>,
    /// Changes made on a review that closed before they were written, by
    /// checkout: written all the same, and handed back if it opens first.
    waiting: Vec<(PathBuf, Vec<Change>)>,
    /// The changes a write is out for, put back if it fails.
    in_flight: Option<(PathBuf, Vec<Change>)>,
    /// A write that failed is tried again from here, not on every pass.
    retry_at: Option<Instant>,
    reading_diff: bool,
    writing: bool,
    /// What the next open is for: a return puts back what was held.
    held: Option<Held>,
    /// Places kept per checkout.
    places: std::collections::BTreeMap<PathBuf, Place>,
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
    root: PathBuf,
    base: Base,
    totals: Option<Totals>,
    record: Option<ReviewRecord>,
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

    fn agent_of(&self, root: &Path) -> Option<String> {
        self.agents
            .iter()
            .find(|(_, checkout)| checkout == root)
            .map(|(id, _)| id.clone())
    }

    fn act(&mut self, asked: Asked) -> Outcome {
        match asked {
            Asked::Send => self.send(),
            Asked::OpenInCode => self.open_in_code(),
            asked => outcome(asked),
        }
    }

    /// Asks the bridge to tell the agent its drafts wait for it. The
    /// drafts become a send only once the bridge says the line was typed.
    fn send(&mut self) -> Outcome {
        let agent = self
            .open
            .as_ref()
            .and_then(|view| self.agent_of(&view.root));
        let Some(view) = self.open.as_mut() else {
            return Outcome::Stay;
        };
        let ids: Vec<u64> = view.drafts().iter().map(|comment| comment.id).collect();
        if ids.is_empty() {
            view.notice = Some(Span::new("no drafts to send", Role::Muted));
            return Outcome::Stay;
        }
        let Some(agent) = agent else {
            view.notice = Some(Span::new(
                "no agent UZE launched works in this checkout",
                Role::Warning,
            ));
            return Outcome::Stay;
        };
        if self.sending.is_some() {
            view.notice = Some(Span::new("a send is waiting for your answer", Role::Muted));
            return Outcome::Stay;
        }
        let line = change::send_line(ids.len(), view.record.next_send.max(1));
        self.sending = Some((
            view.root.clone(),
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
        }))
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
        let due = view.relist || self.listed_at.is_none_or(|at| at.elapsed() >= LIST_EVERY);
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
        let fallback_root = root.clone();
        jobs.push(Job {
            run: Box::new(move |host| {
                let base = match filter {
                    super::Filter::All => git::base(host, &root, target.as_deref(), &known),
                    super::Filter::Uncommitted => Base::default(),
                };
                let listing = git::listing(host, &root, base.commit.as_deref());
                Opaque::new(ListRead {
                    branch: crate::shared::checkout::branch_of(host, &root),
                    record: host.review_record(&root),
                    root,
                    filter,
                    base,
                    listing,
                })
            }),
            fallback: Box::new(move || {
                Opaque::new(ListRead {
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
        let Some(path) = view.restore.take() else {
            return;
        };
        let root = view.root.clone();
        let fallback = (root.clone(), path.clone());
        jobs.push(Job {
            run: Box::new(move |host| {
                let outcome = host.restore_to_head(&root, std::slice::from_ref(&path));
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
            && self.door.at.is_some_and(|at| at.elapsed() < DOOR_EVERY);
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
        jobs.push(Job {
            run: Box::new(move |host| {
                let base = git::base(host, &root, focus.target.as_deref(), &known);
                Opaque::new(DoorRead {
                    totals: git::totals(host, &root, base.commit.as_deref()),
                    record: host.review_record(&root).ok(),
                    root,
                    base,
                })
            }),
            fallback: Box::new(move || {
                Opaque::new(DoorRead {
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

    fn record_for_door(&self) -> &ReviewRecord {
        match (&self.open, &self.focus) {
            (Some(view), Some(focus)) if view.root == focus.root => &view.record,
            _ => &self.door.record,
        }
    }
}

impl Extension for ReviewExtension {
    fn identity(&self) -> String {
        CATALOG.id.to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        vec![SurfaceInfo {
            id: SURFACE.to_owned(),
            slot: SurfaceSlot::View,
            title: CATALOG.name.to_owned(),
            opens: None,
        }]
    }

    fn view(&self, surface: &str, space: Size) -> Option<View> {
        (surface == SURFACE)
            .then(|| self.open.as_ref().map(|view| super::view(view, space)))
            .flatten()
    }

    fn section(&self, _surface: &str, _collapsed: bool) -> Option<Section> {
        None
    }

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome {
        let Some(view) = self.view_mut(surface) else {
            return Outcome::Stay;
        };
        let asked = handle_command(view, command, space);
        self.act(asked)
    }

    fn hit(&mut self, surface: &str, hit: ViewHit, space: Size) -> Outcome {
        let Some(view) = self.view_mut(surface) else {
            return Outcome::Stay;
        };
        let asked = handle_hit(view, hit, space);
        self.act(asked)
    }

    fn scroll(
        &mut self,
        surface: &str,
        target: ScrollTarget,
        direction: ScrollDirection,
        _space: Size,
    ) {
        if let Some(view) = self.view_mut(surface) {
            // The list is the host's to scroll: the wheel never moves
            // the selection.
            if target == ScrollTarget::Content {
                handle_scroll(view, direction, false);
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
        if self.door.root.as_ref() == Some(&checkout.root) {
            view.absorb_record(Ok(self.door.record.clone()));
        }
        self.listed_at = None;
        self.open = Some(view);
    }

    fn close(&mut self, surface: &str) {
        if surface != SURFACE {
            return;
        }
        if let Some(view) = self.open.take() {
            self.places.insert(
                view.root.clone(),
                Place {
                    selected: view.selected.clone(),
                    filter: view.filter,
                },
            );
            // What was changed and not yet written is still written: the
            // operator's work never waits on the surface staying open.
            if !view.unwritten.is_empty() {
                self.door.record = view.record.clone();
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
    }

    fn agent_events(&mut self, happened: &[AgentHappened], _now: Instant) {
        let finished = happened.iter().any(|event| {
            Some(&event.agent) == self.working.as_ref()
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
        let Some(view) = self.open.as_mut().filter(|view| view.root == root) else {
            return;
        };
        match written {
            AgentWritten::Typed => {
                view.change(Change::Sent {
                    ids: sending.ids,
                    agent: sending.agent.clone(),
                });
                self.working = Some(sending.agent);
            }
            AgentWritten::Refused(reason) => {
                view.notice = Some(Span::new(format!("not sent: {reason}"), Role::Warning));
            }
            AgentWritten::Declined => {
                view.notice = Some(Span::new("not sent", Role::Muted));
            }
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
                if let Some(view) = self.open.as_mut().filter(|view| view.root == read.root) {
                    view.absorb_listing(read.filter, read.base, read.branch, read.listing);
                    view.absorb_record(read.record);
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
                if self.focus.as_ref().map(|focus| &focus.root) == Some(&read.root) {
                    self.door.root = Some(read.root);
                    self.door.base = read.base;
                    self.door.totals = read.totals;
                    if let Some(record) = read.record {
                        self.door.record = record;
                    }
                    self.door.at = Some(Instant::now());
                }
            }
            return Outcome::Stay;
        }
        if answer.0.is::<Written>() {
            let Some(written) = answer.downcast::<Written>() else {
                return Outcome::Stay;
            };
            self.writing = false;
            let in_flight = self.in_flight.take();
            match &written.record {
                Ok(_) => self.retry_at = None,
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
            match self.open.as_mut().filter(|view| view.root == written.root) {
                Some(view) => view.absorb_record(written.record),
                None => {
                    if let Err(error) = written.record {
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

    fn door_badge(&self) -> Option<DoorBadge> {
        let record = self.record_for_door();
        let drafts = record
            .comments
            .iter()
            .filter(|comment| comment.status == CommentStatus::Draft)
            .count();
        let answered = record
            .comments
            .iter()
            .filter(|comment| comment.status == CommentStatus::Answered)
            .count();
        let mut spans = Vec::new();
        let totals = self.door.totals.filter(|totals| totals.files > 0);
        match totals {
            Some(totals) => {
                spans.push(Span::new(
                    format!("+{}", grouped(totals.additions)),
                    Role::Success,
                ));
                spans.push(Span::new(" ", Role::Default));
                spans.push(Span::new(
                    format!("-{}", grouped(totals.deletions)),
                    Role::Danger,
                ));
            }
            None => spans.push(Span::new("review", Role::Muted)),
        }
        let quiet = totals.is_none() && drafts == 0 && answered == 0 && self.working.is_none();
        if self.working.is_some() {
            spans.push(Span::new(" ◐", Role::Info));
        } else if drafts > 0 {
            spans.push(Span::new(format!(" ◆{drafts}"), Role::Warning));
        } else if answered > 0 {
            spans.push(Span::new(format!(" ◆{answered}"), Role::Success));
        }
        Some(DoorBadge { spans, quiet })
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
    #[test]
    fn a_count_sets_its_thousands_apart() {
        assert_eq!(super::grouped(672), "672");
        assert_eq!(super::grouped(14_714), "14,714");
        assert_eq!(super::grouped(1_000_000), "1,000,000");
    }
}
