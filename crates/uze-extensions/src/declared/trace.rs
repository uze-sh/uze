//! Why a surface looks as it does: the inspector's account of a build.
//!
//! The trace mirrors the compiled view, one entry per node as it was
//! written, so an author reads it against their own file. A node an `each`
//! repeats is still one entry, counting how often it was reached and drawn
//! and keeping each distinct reason it was not. A template's body is traced
//! once, apart, the way the view declares it, however many times it is used.

use std::collections::BTreeMap;

use uze_contract::compiled::Span;
use uze_contract::{Item, NodeName};

use super::RecordedAction;

/// The most distinct reasons one entry keeps. Reasons repeat per item, and
/// the first few already say what the matter is.
const REASONS_MAX: usize = 8;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    /// What the surface was bound to.
    pub data: DataState,
    /// The view's top-level nodes.
    pub items: Vec<TraceNode>,
    /// Each template's body, by name.
    pub templates: BTreeMap<String, Vec<TraceNode>>,
    /// Whether the node cap stopped the build.
    pub folded: bool,
    /// The actions pressed on this surface, oldest first.
    pub recorded: Vec<RecordedAction>,
}

/// Whether there was data to bind.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum DataState {
    #[default]
    Bound,
    /// The source has answered nothing yet.
    Waiting,
    /// The source failed, saying why.
    Failed(String),
    /// The host would not ask the source, saying why.
    Withheld(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceNode {
    pub entry: TraceEntry,
    pub span: Span,
    /// How many times the build considered it: once per item of an
    /// enclosing `each`, and never when its parent was not drawn.
    pub reached: u32,
    pub drawn: u32,
    /// Why it was not drawn, or, for a `List` or `Table` repeating its
    /// rows, why it has none.
    pub absent: Vec<Absence>,
    pub children: Vec<TraceNode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TraceEntry {
    Node(NodeName),
    /// `use "name"`.
    Use(String),
}

/// Why a node was not drawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Absence {
    /// `when` named a field that is missing (`missing`), or one that is
    /// there but not present: `null`, `false`, `""`, `0` or `[]`.
    When { field: String, missing: bool },
    /// `unless` named a field that is present.
    Unless { field: String },
    /// `show` asked for a UI value the host does not hold.
    Show {
        id: String,
        value: String,
        actual: Option<String>,
    },
    /// None of `show-any`'s pairs held.
    ShowAny,
    /// `each` named a field that is missing.
    EachMissing { field: String },
    /// `each` named an empty list.
    EachEmpty { field: String },
    /// `each` named something that is not a list.
    EachNotList { field: String },
    /// A property bound to exactly one reference whose field is missing.
    Missing { field: String },
    /// The view had reached the node cap.
    CapReached,
    /// `use` named a template the view does not declare.
    UnknownTemplate,
    /// A node that only means something inside another (a `Mark` outside
    /// `Marks`, a `Segment` outside `Progress`, a `Tab` outside `Tabs`).
    Misplaced,
}

impl TraceNode {
    /// One entry per item, with nothing reached yet.
    pub(super) fn skeleton(items: &[Item]) -> Vec<TraceNode> {
        items.iter().map(Self::of).collect()
    }

    fn of(item: &Item) -> TraceNode {
        let (entry, span, children) = match item {
            Item::Node(node) => (
                TraceEntry::Node(node.name),
                node.span,
                Self::skeleton(&node.children),
            ),
            Item::Use { template, span } => (TraceEntry::Use(template.clone()), *span, Vec::new()),
        };
        TraceNode {
            entry,
            span,
            reached: 0,
            drawn: 0,
            absent: Vec::new(),
            children,
        }
    }

    pub(super) fn absent(&mut self, reason: Absence) {
        if self.absent.len() < REASONS_MAX && !self.absent.contains(&reason) {
            self.absent.push(reason);
        }
    }
}
