//! The code surface as the host hosts every extension: one view, entered
//! by the door a person chose, open on one checkout at a time, whose four
//! reads are jobs; and the sidebar's timeline of the checkout in front,
//! read again on its own cadence.
//!
//! The jobs run in parallel, and the order the files half needs is kept
//! here rather than by the host: everything that reads or writes a file
//! waits for the one before it to answer, so a save lands ahead of the
//! read that re-colours it, while listings and a second colouring pass go
//! alongside (see [`CodeExtension::file_jobs`]).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{
    CATALOG, CodeHeld, CodeOutcome, CodePlace, CodeView, ContentMode, DiffAnswer, DiffRequest,
    FileAnswer, FileRequest, Measure, RefreshedChanges, Timeline, fulfill, handle_command,
    handle_hover, handle_mouse, handle_scroll, measure, scroll_to, text, timeline,
    timeline_section, unanswered, view,
};
use crate::contract::{
    Checkout, Entry, Extension, Job, Lanes, Opaque, Outcome, SurfaceInfo, SurfaceSlot,
};
use crate::view::{Command, Layout, ScrollDirection, ScrollTarget, Section, Size, View, ViewHit};

/// The full-frame surface: the checkout's changes, files and map.
pub const SURFACE: &str = "code";
/// The door that reviews: the changes, on their diff.
pub const CHANGES: &str = "changes";
/// The door that navigates: the tree, on a file's contents.
pub const FILES: &str = "files";
/// The sidebar's section: the recent history of the checkout in front.
pub const TIMELINE: &str = "timeline";

/// How far back the timeline reads: the most its section can be dragged
/// open to.
pub const TIMELINE_COMMITS: usize = 30;
/// How often the timeline is read again while the checkout in front stays
/// put. Telling a delivered commit from one still ahead compares patches
/// across the branch and its target, and history does not move at the
/// pace a working tree does.
const TIMELINE_REFRESH: Duration = Duration::from_secs(3);

/// How long a checkout's measurement is taken to still describe it.
///
/// Long enough that opening the surface, closing it and opening it again
/// is free, which is how it is used, and short enough that a map is never
/// a picture of a morning ago.
const MEASURE_FRESH: Duration = Duration::from_secs(30);

/// What one of the files half's requests answered, for the checkout it
/// was asked about.
pub struct FileRead {
    pub root: PathBuf,
    pub answer: FileAnswer,
}

/// The selection's diff, read alone because only the selection moved.
pub struct DiffRead {
    pub root: PathBuf,
    pub answer: DiffAnswer,
}

/// The changes half, read again on its cadence.
pub struct ChangesRead {
    pub root: PathBuf,
    pub refreshed: RefreshedChanges,
}

/// The checkout measured for the map; `None` outside a repository.
pub struct MeasureRead {
    pub root: PathBuf,
    pub measure: Option<Measure>,
}

/// The history of the checkout in front, for the checkout it was asked
/// about; `None` where it has none to read.
pub struct TimelineRead {
    pub cwd: PathBuf,
    pub timeline: Option<Timeline>,
}

/// The history last read, and of which checkout.
struct History {
    cwd: PathBuf,
    timeline: Option<Timeline>,
    at: Instant,
}

/// How the next opening goes: the mode a door means, a path another
/// surface sent the viewer to, which wins over where they last were, or a
/// return to the tab the surface was put away for.
enum Arrival {
    Door(ContentMode),
    Sent { target: PathBuf, directory: bool },
    Return,
}

/// What the `resume` following an opening is handed, as the opening
/// decided: the checkout's place, what was held for the tab, or nothing
/// worth taking, since a path that was sent wins.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Awaiting {
    #[default]
    Place,
    Held,
    Nothing,
}

#[derive(Default)]
pub struct CodeExtension {
    open: Option<CodeView>,
    arrival: Option<Arrival>,
    awaiting: Awaiting,
    /// A read or write of a file is out: the next waits for it.
    file_pending: bool,
    diff_pending: bool,
    changes_pending: bool,
    /// The checkout the open surface last asked to have measured. A
    /// `git grep` over every file is not a read to repeat, and outside a
    /// repository the answer is nothing, which must not be asked again.
    measure_asked: Option<PathBuf>,
    /// Kept whether or not a surface is open to show it: the next to open
    /// on that checkout is the reason it was worth measuring.
    measures: BTreeMap<PathBuf, (Instant, Measure)>,
    focus: Option<Checkout>,
    /// Drawn until a read of the checkout now in front replaces it.
    history: Option<History>,
    reading_history: Option<PathBuf>,
}

impl CodeExtension {
    fn view_mut(&mut self, surface: &str) -> Option<&mut CodeView> {
        (surface == SURFACE).then_some(self.open.as_mut()).flatten()
    }

    fn view_ref(&self, surface: &str) -> Option<&CodeView> {
        (surface == SURFACE).then_some(self.open.as_ref()).flatten()
    }

    /// The open surface, for the host's tests to look into.
    pub fn code_view(&self) -> Option<&CodeView> {
        self.open.as_ref()
    }

    pub fn code_view_mut(&mut self) -> Option<&mut CodeView> {
        self.open.as_mut()
    }

    /// The files half's requests that may go now. A listing, the read of
    /// the checkout's `files.exclude` and a second colouring pass answer
    /// about nothing else in the queue and go at once, in bulk: opening
    /// back where the viewer was asks for every directory that was open.
    /// Anything else waits for the read or write before it, and the queue
    /// is not looked past, which is what keeps a save ahead of the read
    /// after it.
    fn file_jobs(&mut self, jobs: &mut Vec<Job>) {
        let Some(view) = self.open.as_mut() else {
            return;
        };
        let root = view.root().to_path_buf();
        loop {
            let alongside = match view.peek_request() {
                Some(
                    FileRequest::List(_) | FileRequest::Exclusions(_) | FileRequest::Colour(_),
                ) => true,
                Some(_) if !self.file_pending => false,
                _ => return,
            };
            let Some(request) = view.take_request() else {
                return;
            };
            self.file_pending |= !alongside;
            jobs.push(file_job(root.clone(), request));
        }
    }

    /// The selection's diff alone, the moment it moved.
    fn diff_job(&mut self, jobs: &mut Vec<Job>) {
        if self.diff_pending {
            return;
        }
        let Some((root, request)) = self
            .open
            .as_ref()
            .and_then(|view| Some((view.root().to_path_buf(), view.diff_request()?)))
        else {
            return;
        };
        self.diff_pending = true;
        jobs.push(diff_job(root, request));
    }

    /// The changes half again, on its own cadence. A moved selection is
    /// the one-file read's, unless there is no list yet to find it in: the
    /// first read is a whole one.
    fn changes_job(&mut self, jobs: &mut Vec<Job>) {
        if self.changes_pending {
            return;
        }
        let Some(view) = self.open.as_ref() else {
            return;
        };
        let first = view.diff_pending() && view.diff_request().is_none();
        if !first && !view.refresh_due() {
            return;
        }
        self.changes_pending = true;
        jobs.push(changes_job(view.root().to_path_buf(), view.placement()));
    }

    /// The checkout's measurement, once per surface that shows the map
    /// and has none recent enough.
    fn measure_job(&mut self, jobs: &mut Vec<Job>) {
        let Some(view) = self.open.as_ref().filter(|view| view.wants_measure()) else {
            return;
        };
        let root = view.root().to_path_buf();
        if self.measure_asked.as_deref() == Some(root.as_path()) {
            return;
        }
        // A measurement this recent describes the same checkout: a map is
        // where the lines are, which does not change between two looks.
        if self
            .measures
            .get(&root)
            .is_some_and(|(taken, _)| taken.elapsed() < MEASURE_FRESH)
        {
            return;
        }
        self.measure_asked = Some(root.clone());
        jobs.push(measure_job(root));
    }

    /// The history of the checkout in front, again once it has stood for
    /// [`TIMELINE_REFRESH`] or the viewer moved to another, one read at a
    /// time.
    fn history_job(&mut self, jobs: &mut Vec<Job>) {
        if self.reading_history.is_some() {
            return;
        }
        let Some(focus) = &self.focus else {
            return;
        };
        let fresh = self.history.as_ref().is_some_and(|history| {
            history.cwd == focus.root && history.at.elapsed() < TIMELINE_REFRESH
        });
        if fresh {
            return;
        }
        self.reading_history = Some(focus.root.clone());
        jobs.push(history_job(focus));
    }

    fn absorb_history(&mut self, read: TimelineRead) {
        if self.reading_history.as_ref() == Some(&read.cwd) {
            self.reading_history = None;
        }
        if self.focus.as_ref().map(|focus| &focus.root) == Some(&read.cwd) {
            self.history = Some(History {
                cwd: read.cwd,
                timeline: read.timeline,
                at: Instant::now(),
            });
        }
    }

    fn timeline(&self) -> Option<(&PathBuf, &Timeline)> {
        let history = self.history.as_ref()?;
        Some((&history.cwd, history.timeline.as_ref()?))
    }

    fn absorb_file(&mut self, read: FileRead) -> Outcome {
        // A listing never took the chain, so it does not release it.
        if !matches!(
            read.answer,
            FileAnswer::Listed { .. } | FileAnswer::Excluded(_) | FileAnswer::Coloured { .. }
        ) {
            self.file_pending = false;
        }
        match self.open.as_mut().filter(|view| view.root() == read.root) {
            Some(view) => {
                view.absorb(read.answer);
                Outcome::Stay
            }
            // A surface that moved on has nowhere to say a write failed,
            // and the reader believes it landed.
            None => unheard_failure(read.answer),
        }
    }

    fn absorb_measure(&mut self, read: MeasureRead) {
        let Some(measure) = read.measure else {
            if let Some(view) = self.open.as_mut().filter(|view| view.root() == read.root) {
                view.absorb_unmeasurable();
            }
            return;
        };
        self.measures
            .insert(read.root.clone(), (Instant::now(), measure.clone()));
        if let Some(view) = self.open.as_mut().filter(|view| view.root() == read.root) {
            view.absorb_measure(measure);
        }
    }
}

fn file_job(root: PathBuf, request: FileRequest) -> Job {
    let silence = unanswered(&request, "reading it failed");
    let fallback_root = root.clone();
    Job {
        run: Box::new(move |host| {
            Opaque::new(FileRead {
                answer: fulfill(host, request),
                root,
            })
        }),
        fallback: Box::new(move || {
            Opaque::new(FileRead {
                root: fallback_root,
                answer: silence,
            })
        }),
    }
}

fn diff_job(root: PathBuf, request: DiffRequest) -> Job {
    let silence = DiffAnswer::failed(&request, "reading the diff failed".to_owned());
    let fallback_root = root.clone();
    Job {
        run: Box::new(move |host| {
            Opaque::new(DiffRead {
                answer: CodeView::read_diff(host, &root, request),
                root,
            })
        }),
        fallback: Box::new(move || {
            Opaque::new(DiffRead {
                root: fallback_root,
                answer: silence,
            })
        }),
    }
}

fn changes_job(root: PathBuf, placement: super::ViewPlacement) -> Job {
    let silence = RefreshedChanges::failed(placement.clone(), "reading the changes".to_owned());
    let fallback_root = root.clone();
    Job {
        run: Box::new(move |host| {
            Opaque::new(ChangesRead {
                refreshed: CodeView::refresh(host, root.clone(), placement),
                root,
            })
        }),
        fallback: Box::new(move || {
            Opaque::new(ChangesRead {
                root: fallback_root,
                refreshed: silence,
            })
        }),
    }
}

fn history_job(focus: &Checkout) -> Job {
    let cwd = focus.root.clone();
    let target = focus.target.clone();
    let fallback_cwd = cwd.clone();
    Job {
        run: Box::new(move |host| {
            Opaque::new(TimelineRead {
                timeline: timeline(host, &cwd, TIMELINE_COMMITS, target.as_deref()),
                cwd,
            })
        }),
        fallback: Box::new(move || {
            Opaque::new(TimelineRead {
                cwd: fallback_cwd,
                timeline: None,
            })
        }),
    }
}

fn measure_job(root: PathBuf) -> Job {
    let fallback_root = root.clone();
    Job {
        run: Box::new(move |host| {
            Opaque::new(MeasureRead {
                measure: measure(host, &root).ok(),
                root,
            })
        }),
        fallback: Box::new(move || {
            Opaque::new(MeasureRead {
                root: fallback_root,
                measure: None,
            })
        }),
    }
}

/// A write that failed after its surface moved on, said anyway.
fn unheard_failure(answer: FileAnswer) -> Outcome {
    let (title, path, message) = match answer {
        FileAnswer::Saved {
            path,
            outcome: Err(message),
        } => ("save failed", path, message),
        FileAnswer::Deleted {
            path,
            outcome: Err(message),
        } => ("delete failed", path, message),
        FileAnswer::Renamed {
            from,
            outcome: Err(message),
            ..
        } => ("rename failed", from, message),
        _ => return Outcome::Stay,
    };
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    Outcome::Failed {
        title: title.to_owned(),
        detail: format!("{name}: {message}"),
    }
}

fn outcome(outcome: CodeOutcome) -> Outcome {
    match outcome {
        CodeOutcome::Stay => Outcome::Stay,
        CodeOutcome::Close => Outcome::Close,
        CodeOutcome::Copy(text) => Outcome::Copy(text),
    }
}

/// The mode a door opens on.
fn door_mode(door: &str) -> Option<ContentMode> {
    match door {
        CHANGES => Some(ContentMode::Diff),
        FILES => Some(ContentMode::Contents),
        _ => None,
    }
}

/// Which half a mode belongs to. A document read as its preview is still
/// the files half: its door is the one already open, and shuts rather than
/// turning the preview back into source first.
fn half(mode: ContentMode) -> ContentMode {
    match mode {
        ContentMode::Preview => ContentMode::Contents,
        other => other,
    }
}

impl Extension for CodeExtension {
    fn identity(&self) -> String {
        CATALOG.id.to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        vec![
            SurfaceInfo {
                opens: None,
                id: SURFACE.to_owned(),
                slot: SurfaceSlot::View,
                title: CATALOG.name.to_owned(),
            },
            SurfaceInfo {
                opens: None,
                id: TIMELINE.to_owned(),
                slot: SurfaceSlot::Section,
                title: TIMELINE.to_owned(),
            },
        ]
    }

    fn view(&self, surface: &str, space: Size) -> Option<View> {
        self.view_ref(surface).map(|state| view(state, space))
    }

    /// The history of the checkout in front. How far it is scrolled is
    /// the host's, which laid its rows out.
    fn section(&self, surface: &str, collapsed: bool) -> Option<Section> {
        if surface != TIMELINE {
            return None;
        }
        self.timeline()
            .map(|(_, timeline)| timeline_section(timeline, collapsed, 0))
    }

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome {
        self.view_mut(surface).map_or(Outcome::Stay, |state| {
            outcome(handle_command(state, command, space))
        })
    }

    /// On the timeline, a commit asks for its account; the fold of the
    /// header is the host's.
    fn hit(&mut self, surface: &str, hit: ViewHit, space: Size) -> Outcome {
        if surface == TIMELINE {
            let ViewHit::SelectItem(index) = hit else {
                return Outcome::Stay;
            };
            let Some((cwd, commit)) = self
                .timeline()
                .and_then(|(cwd, timeline)| Some((cwd, timeline.commits.get(index)?)))
            else {
                return Outcome::Stay;
            };
            return Outcome::OpenCommit {
                project: cwd.clone(),
                hash: commit.hash.clone(),
            };
        }
        self.view_mut(surface).map_or(Outcome::Stay, |state| {
            outcome(handle_mouse(state, Some(hit), space))
        })
    }

    /// The content only: the list is the host's to scroll, since only it
    /// knows how many rows fit. The host does not ask for a step down
    /// once the last line is drawn.
    fn scroll(
        &mut self,
        surface: &str,
        target: ScrollTarget,
        direction: ScrollDirection,
        _space: Size,
    ) {
        if target == ScrollTarget::Content
            && let Some(state) = self.view_mut(surface)
        {
            handle_scroll(state, direction, false);
        }
    }

    /// Pressed while it stands, the door already open shuts it and the
    /// other shows that half with the selection; pressed while it is put
    /// away, it says which half opens.
    fn enter(&mut self, surface: &str, entry: Entry) -> Outcome {
        if surface != SURFACE {
            return Outcome::Stay;
        }
        let arrival = match entry {
            Entry::Door(door) => {
                let Some(wanted) = door_mode(&door) else {
                    return Outcome::Stay;
                };
                if let Some(view) = self.open.as_mut() {
                    if half(view.showing()) == half(wanted) {
                        return Outcome::Close;
                    }
                    view.show(wanted);
                    return Outcome::Stay;
                }
                Arrival::Door(wanted)
            }
            Entry::Path { target, directory } => Arrival::Sent { target, directory },
            Entry::Return => Arrival::Return,
        };
        self.arrival = Some(arrival);
        Outcome::Stay
    }

    fn focus(&mut self, checkout: Option<&Checkout>) {
        self.focus = checkout.cloned();
        if self.focus.is_none() {
            self.history = None;
        }
    }

    fn open(&mut self, surface: &str, checkout: &Checkout) {
        if surface != SURFACE {
            return;
        }
        let (mode, sent, awaiting) = match self.arrival.take() {
            Some(Arrival::Door(mode)) => (mode, None, Awaiting::Place),
            Some(Arrival::Sent { target, directory }) => (
                ContentMode::Contents,
                Some(CodePlace::at(&checkout.root, &target, directory)),
                Awaiting::Nothing,
            ),
            Some(Arrival::Return) => (ContentMode::Diff, None, Awaiting::Held),
            None => (ContentMode::Diff, None, Awaiting::Place),
        };
        let mut view = CodeView::opening(checkout.root.clone(), checkout.display.clone(), mode);
        self.awaiting = awaiting;
        if let Some(place) = sent {
            view = view.resuming(place);
        }
        if let Some((_, measure)) = self.measures.get(&checkout.root) {
            view.absorb_measure(measure.clone());
        }
        self.measure_asked = None;
        self.open = Some(view);
    }

    fn close(&mut self, surface: &str) {
        if surface == SURFACE {
            self.open = None;
        }
    }

    /// The map is a picture of the whole checkout and takes the frame;
    /// everything else is read in the column beside the tree.
    fn layout(&self, surface: &str) -> Layout {
        match self.view_ref(surface).map(CodeView::showing) {
            Some(ContentMode::Map) => Layout::Board,
            _ => Layout::Sidebar,
        }
    }

    fn missed(&mut self, surface: &str, space: Size) {
        if let Some(state) = self.view_mut(surface) {
            handle_mouse(state, None, space);
        }
    }

    fn hover(&mut self, surface: &str, hit: Option<ViewHit>) -> bool {
        self.view_mut(surface)
            .is_some_and(|state| handle_hover(state, hit))
    }

    fn scroll_to(&mut self, surface: &str, first: usize, _space: Size) {
        if let Some(state) = self.view_mut(surface) {
            scroll_to(state, first);
        }
    }

    fn text(&self, surface: &str, lines: std::ops::Range<usize>) -> Vec<String> {
        self.view_ref(surface)
            .map(|state| text(state, lines))
            .unwrap_or_default()
    }

    fn typing(&self, surface: &str) -> bool {
        self.view_ref(surface).is_some_and(CodeView::typing)
    }

    fn paste(&mut self, surface: &str, text: &str, space: Size) {
        if let Some(state) = self.view_mut(surface) {
            state.paste(text, space);
        }
    }

    fn jobs(&mut self) -> Vec<Job> {
        let mut jobs = Vec::new();
        self.diff_job(&mut jobs);
        self.changes_job(&mut jobs);
        self.file_jobs(&mut jobs);
        self.measure_job(&mut jobs);
        self.history_job(&mut jobs);
        jobs
    }

    fn lanes(&self) -> Lanes {
        Lanes::Parallel
    }

    fn absorb(&mut self, answer: Opaque) -> Outcome {
        let answer = match answer.0.downcast::<FileRead>() {
            Ok(read) => return self.absorb_file(*read),
            Err(other) => other,
        };
        let answer = match answer.downcast::<DiffRead>() {
            Ok(read) => {
                self.diff_pending = false;
                if let Some(view) = self.open.as_mut().filter(|view| view.root() == read.root) {
                    view.absorb_diff(read.answer);
                }
                return Outcome::Stay;
            }
            Err(other) => other,
        };
        let answer = match answer.downcast::<ChangesRead>() {
            Ok(read) => {
                self.changes_pending = false;
                if let Some(view) = self.open.as_mut().filter(|view| view.root() == read.root) {
                    view.absorb_changes(read.refreshed);
                }
                return Outcome::Stay;
            }
            Err(other) => other,
        };
        let answer = match answer.downcast::<MeasureRead>() {
            Ok(read) => {
                self.absorb_measure(*read);
                return Outcome::Stay;
            }
            Err(other) => other,
        };
        if let Ok(read) = answer.downcast::<TimelineRead>() {
            self.absorb_history(*read);
        }
        Outcome::Stay
    }

    fn place(&self, surface: &str) -> Option<Opaque> {
        self.view_ref(surface).map(CodeView::place).map(Opaque::new)
    }

    fn hold(&self, surface: &str) -> Option<Opaque> {
        self.view_ref(surface).map(CodeView::held).map(Opaque::new)
    }

    /// Where the viewer last was, or after a return what stood, unless
    /// another surface sent them somewhere.
    fn resume(&mut self, surface: &str, place: Opaque) {
        let awaiting = std::mem::take(&mut self.awaiting);
        if surface != SURFACE {
            return;
        }
        let Some(view) = self.open.take() else {
            return;
        };
        self.open = Some(match awaiting {
            Awaiting::Place => match place.downcast::<CodePlace>() {
                Some(place) => view.resuming(place),
                None => view,
            },
            Awaiting::Held => match place.downcast::<CodeHeld>() {
                Some(held) => view.returning(held),
                None => view,
            },
            Awaiting::Nothing => view,
        });
    }
}
