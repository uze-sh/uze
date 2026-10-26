//! The one contract the host speaks to every extension through.
//!
//! The declared engine implemented it first (`extension-view-engine`), and
//! the built-ins move onto it one at a time (`generic-extension-host`). It
//! carries what the orchestrator did on their behalf when each was wired
//! by hand (A1): which jobs may run together, what a job answers when its
//! thread dies, what happens to an answer whose surface has closed, state
//! that outlives a surface, and the gestures a board answers (H4).

use std::any::Any;
use std::path::PathBuf;

use crate::Host;
use crate::view::{
    Command, Layout, ScrollDirection, ScrollTarget, Section, Size, Span, View, ViewHit,
};

/// Where a surface is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceSlot {
    /// The full frame.
    View,
    /// A collapsible block of the sidebar.
    Section,
    /// A panel docked along the right of the agent area, beside the pane.
    Drawer,
}

/// One surface an extension offers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceInfo {
    /// The extension's own name for it, handed back in every call about it.
    pub id: String,
    pub slot: SurfaceSlot,
    pub title: String,
}

/// What the host does once an extension has answered a gesture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Stay,
    Close,
    /// Say this at the far end of the footer.
    Notice(Span),
    /// Show `target` on the code surface: where a drawing or a document
    /// ends and the checkout begins. `project` is what the path was
    /// written relative to.
    OpenPath {
        project: PathBuf,
        target: PathBuf,
    },
    /// Put this on the clipboard, and say so.
    Copy(String),
    /// Open this extension's full-frame `surface` on the checkout in
    /// front: a summary in the sidebar sending the viewer to what it
    /// counts.
    Show(String),
    /// Say this went wrong, wherever the viewer is now: a write that
    /// failed must not pass unsaid, even once its surface has closed.
    Failed {
        title: String,
        detail: String,
    },
}

/// How a full-frame surface is entered: by one of the doors it offers, or
/// sent to a path another surface pointed at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Entry {
    /// The extension's own name for a way in, which a person chose before
    /// pressing anything: reviewing or navigating.
    Door(String),
    /// `target`, inside the checkout the surface opens on.
    Path { target: PathBuf, directory: bool },
}

/// The checkout a full-frame surface opens on, as the host found it in
/// front of the viewer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkout {
    pub root: PathBuf,
    /// The checkout as a reader is shown it, for a title: which checkout
    /// is being looked at is known from the first frame, never read.
    pub display: String,
    /// The branch the checkout delivers to, once the host has evaluated
    /// it.
    pub target: Option<String>,
}

/// Whether an extension's jobs may run at the same time.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Lanes {
    /// One at a time, in the order asked: a save ahead of the read after it.
    #[default]
    Serial,
    /// Any number at once.
    Parallel,
}

/// A value only the extension that produced it knows the type of.
pub struct Opaque(pub Box<dyn Any + Send>);

impl Opaque {
    pub fn new<T: Any + Send>(value: T) -> Self {
        Self(Box::new(value))
    }

    pub fn downcast<T: Any>(self) -> Option<T> {
        self.0.downcast::<T>().ok().map(|boxed| *boxed)
    }
}

impl std::fmt::Debug for Opaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Opaque")
    }
}

/// The work of a [`Job`]: given the host, produce an answer.
pub type JobRun = Box<dyn FnOnce(&dyn Host) -> Opaque + Send>;

/// What a [`Job`] answers if its thread panics.
pub type JobFallback = Box<dyn FnOnce() -> Opaque + Send>;

/// Work that needs the [`Host`], run by the host off the drawing thread.
pub struct Job {
    pub run: JobRun,
    /// What the job answers if its thread panics, so a failed read is drawn
    /// as one instead of leaving the surface waiting forever.
    pub fallback: JobFallback,
}

/// Everything the host asks of an extension.
pub trait Extension: Send {
    /// Who the extension is, as the host attributes it.
    fn identity(&self) -> String;

    fn surfaces(&self) -> Vec<SurfaceInfo>;

    /// The view of `surface` (full-frame, or the drawer's column), or `None`
    /// when it is neither.
    fn view(&self, surface: &str, space: Size) -> Option<View>;

    /// The sidebar section of `surface`, or `None` when it is not one.
    fn section(&self, surface: &str, collapsed: bool) -> Option<Section>;

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome;

    fn hit(&mut self, surface: &str, hit: ViewHit, space: Size) -> Outcome;

    fn scroll(
        &mut self,
        surface: &str,
        target: ScrollTarget,
        direction: ScrollDirection,
        space: Size,
    );

    /// `surface` is opening full-frame on `checkout`, before any place is
    /// handed back to it.
    fn open(&mut self, _surface: &str, _checkout: &Checkout) {}

    /// `entry` was used on `surface`: before it opens, it says how it
    /// opens; while it stands, a door pressed again may close it or show
    /// the other half.
    fn enter(&mut self, _surface: &str, _entry: Entry) -> Outcome {
        Outcome::Stay
    }

    /// Whether a letter pressed on `surface` is text rather than a
    /// shortcut: everything behind it is sealed while it is.
    fn typing(&self, _surface: &str) -> bool {
        false
    }

    /// Text pasted while `surface` is typing.
    fn paste(&mut self, _surface: &str, _text: &str, _space: Size) {}

    /// The checkout in front of the viewer, told on every pass of the
    /// host's loop: what a section about the work in front reads from.
    /// `None` when no agent's checkout is in front, or when the operator
    /// switched the extension off.
    fn focus(&mut self, _checkout: Option<&Checkout>) {}

    /// A press on `surface` landed on nothing it drew: a list it has open
    /// shuts, as a click outside every menu here does.
    fn missed(&mut self, _surface: &str, _space: Size) {}

    /// `surface` was put away; what it held for the viewer may go. Called
    /// after its place was taken.
    fn close(&mut self, _surface: &str) {}

    /// How `surface` is laid out now, which decides the room it is given:
    /// a board takes the frame, a list the column beside its navigator.
    fn layout(&self, _surface: &str) -> Layout {
        Layout::Sidebar
    }

    /// The pointer moved over `surface` without pressing, onto `hit`.
    /// Answers whether the frame has to be drawn again.
    fn hover(&mut self, _surface: &str, _hit: Option<ViewHit>) -> bool {
        false
    }

    /// What `surface` drew was taken hold of and moved by so many cells.
    fn drag(&mut self, _surface: &str, _columns: i32, _rows: i32, _space: Size) {}

    /// A sideways wheel over `surface`, by so many columns.
    fn pan(&mut self, _surface: &str, _columns: i32, _space: Size) {}

    /// The content's scrollbar was pressed or held at the row that shows
    /// line `first` at the top.
    fn scroll_to(&mut self, _surface: &str, _first: usize, _space: Size) {}

    /// The text of `lines` of what `surface` shows, for copying a
    /// selection: asked rather than read off the frame, because a
    /// selection reaches lines scrolled out of what was drawn.
    fn text(&self, _surface: &str, _lines: std::ops::Range<usize>) -> Vec<String> {
        Vec::new()
    }

    /// Work to hand the host now. Empty when there is none.
    fn jobs(&mut self) -> Vec<Job> {
        Vec::new()
    }

    fn lanes(&self) -> Lanes {
        Lanes::Serial
    }

    /// A job's answer, whether or not its surface is still open: one that
    /// closed drops a read that finished, and still says a write that
    /// failed.
    fn absorb(&mut self, _answer: Opaque) -> Outcome {
        Outcome::Stay
    }

    /// Where the viewer was on `surface`, handed back on reopening.
    fn place(&self, _surface: &str) -> Option<Opaque> {
        None
    }

    fn resume(&mut self, _surface: &str, _place: Opaque) {}

    /// State that outlives every surface: a cache, a measure. Kept by the
    /// host while the client runs, apart from any place.
    fn memory(&self) -> Option<Opaque> {
        None
    }

    fn restore_memory(&mut self, _memory: Opaque) {}
}
