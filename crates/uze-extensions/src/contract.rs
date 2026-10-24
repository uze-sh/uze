//! The one contract the host speaks to every extension through.
//!
//! Each built-in extension is still wired into the orchestrator by hand,
//! field by field (about twenty places for `spec`). This is the shape that
//! replaces that wiring: the declared engine implements it first
//! (`extension-view-engine`), and `code`, `architect` and `spec` move onto it
//! in a later change without the contract changing — which is why it already
//! carries what the first review found the orchestrator does on their
//! behalf today (A1): which jobs may run together, what a job answers when
//! its thread dies, what happens to an answer whose surface has closed, and
//! state that outlives a surface.

use std::any::Any;

use crate::Host;
use crate::view::{Command, ScrollDirection, ScrollTarget, Section, Size, Span, View, ViewHit};

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

    fn scroll(&mut self, surface: &str, target: ScrollTarget, direction: ScrollDirection);

    /// Work to hand the host now. Empty when there is none.
    fn jobs(&mut self) -> Vec<Job> {
        Vec::new()
    }

    fn lanes(&self) -> Lanes {
        Lanes::Serial
    }

    /// A job's answer, while its surface is still open.
    fn absorb(&mut self, _answer: Opaque) {}

    /// A job's answer that arrived after its surface closed. A write that
    /// failed is still worth saying; a read that finished is not.
    fn orphaned(&mut self, _answer: Opaque) -> Option<Outcome> {
        None
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
