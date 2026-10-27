//! The one contract the host speaks to every extension through.
//!
//! The declared engine implemented it first (`extension-view-engine`), and
//! the built-ins move onto it one at a time (`extension-generic-host`). It
//! carries what the orchestrator did on their behalf when each was wired
//! by hand (A1): which jobs may run together, what a job answers when its
//! thread dies, what happens to an answer whose surface has closed, state
//! that outlives a surface, and the gestures a board answers (H4).

use std::any::Any;
use std::path::PathBuf;
use std::time::Instant;

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
    /// One row of the sidebar's header.
    Status,
    /// `rows` reserved along the top, or the `bottom`, of each agent's
    /// pane, taken before the pane's terminal is sized.
    Band { rows: u8, bottom: bool },
    /// A modal card over the frame, opened by a gesture.
    Overlay,
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
    /// Show the account of commit `hash` of the checkout at `project`,
    /// beside the row it was chosen on: where a history ends and what one
    /// commit did begins.
    OpenCommit {
        project: PathBuf,
        hash: String,
    },
    /// Put this on the clipboard, and say so.
    Copy(String),
    /// Say this in a toast, which the host attributes to the extension
    /// that said it: what an extension's answer told the viewer.
    Toast(String),
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
    /// Do this to an agent, through the host: shown first, refused rather
    /// than queued, limited (`extension-agent-bridge`).
    Agent(Box<AgentRequest>),
}

/// What an extension asks the host to do to an agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRequest {
    pub what: AgentAsk,
    /// The agent's id; `None` for the one the surface stands for.
    pub agent: Option<String>,
    /// The id of the extension's action that asks: what an allowance to
    /// prompt unattended is given to.
    pub action: String,
    /// The action's label, as the confirmation and the notice say it.
    pub label: String,
    /// Sent by an agent's event rather than a gesture.
    pub unattended: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentAsk {
    /// Type `text`, as resolved, after showing it.
    Prompt { text: String },
    /// Type the harness's own invocation of `skill` with `args`.
    Skill { skill: String, args: Vec<String> },
    /// Start the harness on a new conversation, always confirmed.
    Restart,
    /// Put the agent's pane in front.
    Focus,
}

/// Something an agent of the workspace did, as the host learned it.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentHappened {
    pub event: uze_contract::AgentEvent,
    pub agent: String,
    pub harness: String,
    pub checkout: PathBuf,
    /// What else the event says: the tool, a checkout-relative path, the
    /// call's id, its outcome, when.
    pub detail: serde_json::Value,
}

/// What an answer asked to tell the operator outside the frame, under
/// `notify`: a notification, a cue, or both, already let through the
/// extension's rate. The title is the extension's own; the host leads it
/// with the extension's attribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notify {
    pub notification: Option<uze_contract::Notification>,
    pub sound: Option<uze_contract::Sound>,
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

/// Something that happens to a checkout: what an extension asks the host
/// to watch, and what the host tells it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckoutEvent {
    pub checkout: PathBuf,
    /// For `paths.changed`, the paths watched, or the ones that changed.
    pub event: uze_contract::EventName,
}

/// A key or the pointer, handed as it came to a focused surface that reads
/// them: the resident tier's, the one exception to an extension knowing
/// nothing of the keyboard. The host keeps its escape chord and its pause
/// key, which never arrive here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RawInput {
    /// `key` spelled as the keymap spells it (`up`, `a`, `f5`, `space`),
    /// `mods` among `ctrl`, `alt` and `shift`, in that order.
    Key { key: String, mods: Vec<String> },
    /// A press `x` columns and `y` rows from the surface's top left.
    Pointer { x: u16, y: u16, button: String },
}

/// What the host saw on one pass of its loop.
#[derive(Clone, Copy, Debug)]
pub struct Pass<'a> {
    /// The host's clock, which every interval is counted on.
    pub now: Instant,
    /// The extension's surfaces in view: an open view in front, a section
    /// unfolded in the sidebar, an open drawer.
    pub visible: &'a [String],
    /// What happened to the checkouts it watches since the last pass.
    pub happened: &'a [CheckoutEvent],
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

    /// The operator's preferences for this extension changed, or were
    /// first read: what its surfaces read as `{pref.<name>}` from now on.
    fn configure(&mut self, _preferences: &serde_json::Value) {}

    /// What the workspace's agents did since the last pass, handed only to
    /// an extension holding `agent:observe`.
    fn agent_events(&mut self, _happened: &[AgentHappened], _now: Instant) {}

    /// The workspace's agents as they stand now, for an `agents` source,
    /// handed only to an extension holding `agent:observe`.
    fn agents(&mut self, _agents: &serde_json::Value) {}

    /// What the extension asks of the host on its own, outside any
    /// gesture: an unattended prompt its agent events sent. Taken once.
    fn requests(&mut self) -> Vec<Outcome> {
        Vec::new()
    }

    /// What its answers asked to tell the operator outside the frame since
    /// the last time the host asked, each within the extension's rate.
    /// Taken once.
    fn notifications(&mut self) -> Vec<Notify> {
        Vec::new()
    }

    /// `entry` was used on `surface`: before it opens, it says how it
    /// opens; while it stands, a door pressed again may close it or show
    /// the other half.
    fn enter(&mut self, _surface: &str, _entry: Entry) -> Outcome {
        Outcome::Stay
    }

    /// Whether `surface` applies where it would be drawn now: one that
    /// does not is neither offered nor drawn. Every surface applies unless
    /// its extension says otherwise.
    fn applies(&self, _surface: &str) -> bool {
        true
    }

    /// Whether a letter pressed on `surface` is text rather than a
    /// shortcut: everything behind it is sealed while it is.
    fn typing(&self, _surface: &str) -> bool {
        false
    }

    /// Text pasted while `surface` is typing.
    fn paste(&mut self, _surface: &str, _text: &str, _space: Size) {}

    /// Whether `surface`, while focused, reads keys and the pointer raw
    /// rather than as the host's commands.
    fn reads_raw_input(&self, _surface: &str) -> bool {
        false
    }

    /// A key or the pointer on a focused `surface` that reads them raw.
    fn raw_input(&mut self, _surface: &str, _input: RawInput) {}

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

    /// Whether what `surface` draws is taken hold of and moved: a press on
    /// it is then held until the first movement says whether it was a
    /// click or a drag. One that is not moved has its click at once.
    fn drags(&self, _surface: &str) -> bool {
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

    /// What may happen to a checkout that the extension would be asked
    /// again for: the host watches these and nothing else, so an event no
    /// extension declared costs nothing.
    fn watches(&self) -> Vec<CheckoutEvent> {
        Vec::new()
    }

    /// One pass of the host's loop, told before [`Extension::jobs`]: the
    /// clock, the surfaces in view, and what happened since.
    fn pass(&mut self, _pass: &Pass<'_>) {}

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
