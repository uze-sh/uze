//! A declared extension: one a plugin wrote in KDL, hosted through the
//! [`Extension`] contract.
//!
//! The engine holds what it was handed (the compiled extension and its
//! sources' data) and the UI state the host keeps for it, and binds the one
//! to the other into a [`View`] or a [`Section`] on demand. It parses
//! nothing: what reaches it was compiled by `uze-core` and read by the host.
//!
//! - `bind`: the binder, compiled view + data + UI state into a node tree.
//! - `value`: how a binding becomes a value (D23).
//! - `trace`: the inspector's account of a build.
//! - `time`: the instant an `Elapsed` counts from.
//!
//! What a source answers arrives one way: the engine asks the host, through
//! a [`Job`], to answer the source with its input about the checkout the
//! surface showing it is on, and takes the answer only while it is the
//! latest question asked of that source.

mod bind;
mod fixture_check;
mod time;
mod trace;
mod value;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use uze_contract::{Answer, Clean, CompiledExtension, CompiledView, Question, SurfaceDecl};

use crate::contract::{Checkout, Extension, Job, Opaque, Outcome, SurfaceInfo, SurfaceSlot};
use crate::view::{
    Command, Content, Layout, NodeAct, Role, ScrollDirection, ScrollTarget, Section, Size, Span,
    View, ViewHit,
};
pub use bind::NODE_CAP;
use bind::{Act, Built};
pub use fixture_check::{MissingField, TAB_COMBINATIONS_MAX, missing_fields};
pub use trace::{Absence, DataState, Trace, TraceEntry, TraceNode};

/// What a source answered: its data, or why there is none.
pub type SourceData = Result<serde_json::Value, String>;

/// A gesture that would have run an action, kept for the inspector: this
/// version runs nothing an extension ships.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedAction {
    pub surface: String,
    pub action: String,
    /// The `item` the action would have received, as the data holds it:
    /// whoever draws it cleans it first, as every other string.
    pub item: serde_json::Value,
}

pub struct DeclaredExtension {
    compiled: CompiledExtension,
    /// Bumped by [`DeclaredExtension::replace`], so a build of the old
    /// compile is never served for the new one.
    generation: u64,
    /// By source id.
    data: BTreeMap<String, SourceData>,
    /// Why a source has no data, when the host would not ask it: it needs
    /// consent or enabling, or its checkout is not one it may read. By
    /// source id, and said on the surface in place of the data.
    withheld: BTreeMap<String, String>,
    /// A digest of each source's data, taken once when it arrives rather
    /// than on every frame that asks whether it changed.
    digests: BTreeMap<String, u64>,
    /// The checkout in front, which a section and a drawer are about.
    in_front: Option<PathBuf>,
    /// The checkout each open view is about, by surface.
    opened: BTreeMap<String, PathBuf>,
    /// The latest question asked of each source, by source id.
    asked: BTreeMap<String, Asked>,
    /// How many questions were asked, numbering each.
    questions: u64,
    /// `ui.<id>` values, by surface then id.
    ui: BTreeMap<String, BTreeMap<String, String>>,
    /// The first row shown, by surface.
    scroll: BTreeMap<String, usize>,
    locale: String,
    recorded: Vec<RecordedAction>,
    /// The last build of each surface. Behind a cell because drawing asks
    /// through `&self`; the contract needs `Send`, never `Sync`.
    memo: RefCell<BTreeMap<String, Memo>>,
    /// How many builds have run, so the inspector can say what drawing
    /// costs and a test can tell a served build from a fresh one.
    builds: std::cell::Cell<u64>,
}

/// A question asked of a source: about which checkout, with which input,
/// and its number, which the answer carries back.
#[derive(Clone, Debug, PartialEq)]
struct Asked {
    checkout: PathBuf,
    input: serde_json::Value,
    number: u64,
}

/// The host's answer to the question numbered `number` of `source`.
struct SourceAnswer {
    source: String,
    number: u64,
    answer: Answer,
}

/// What a source's job answers when its thread stopped before answering.
const STOPPED: &str = "the question stopped before it was answered";

/// What a build depends on. The same key is the same tree, so a frame that
/// changed nothing costs a comparison.
#[derive(Clone, Debug, PartialEq)]
struct MemoKey {
    generation: u64,
    digest: Option<u64>,
    space: Size,
    ui: BTreeMap<String, String>,
}

struct Memo {
    key: MemoKey,
    built: Built,
}

/// Where the viewer was on a surface: its UI state and scroll.
struct Place {
    ui: BTreeMap<String, String>,
    scroll: usize,
}

/// What outlives every surface: the actions pressed so far.
struct Memory {
    recorded: Vec<RecordedAction>,
}

impl DeclaredExtension {
    pub fn new(compiled: CompiledExtension, locale: impl Into<String>) -> Self {
        Self {
            compiled,
            generation: 0,
            data: BTreeMap::new(),
            withheld: BTreeMap::new(),
            digests: BTreeMap::new(),
            in_front: None,
            opened: BTreeMap::new(),
            asked: BTreeMap::new(),
            questions: 0,
            ui: BTreeMap::new(),
            scroll: BTreeMap::new(),
            locale: locale.into(),
            recorded: Vec::new(),
            memo: RefCell::default(),
            builds: std::cell::Cell::new(0),
        }
    }

    pub fn compiled(&self) -> &CompiledExtension {
        &self.compiled
    }

    /// A newer compile of the same extension, such as after a linked
    /// extension was saved. UI state is kept where its ids still exist.
    /// Every source shown is asked again, since what answers it may have
    /// changed.
    pub fn replace(&mut self, compiled: CompiledExtension) {
        self.compiled = compiled;
        self.generation += 1;
        self.memo.get_mut().clear();
        self.asked.clear();
        let surfaces: Vec<String> = self.ui.keys().cloned().collect();
        for surface in surfaces {
            let ids = self
                .compiled
                .view_of(&surface)
                .map(bind::declared_ids)
                .unwrap_or_default();
            if let Some(ui) = self.ui.get_mut(&surface) {
                ui.retain(|id, _| ids.contains(id));
            }
        }
    }

    pub fn set_data(&mut self, source: impl Into<String>, data: SourceData) {
        let source = source.into();
        self.withheld.remove(&source);
        self.digests.insert(source.clone(), digest(&data));
        self.data.insert(source, data);
    }

    /// `source` has no data because the host would not ask it, and
    /// `reason` says why.
    fn withhold(&mut self, source: String, reason: String) {
        self.data.remove(&source);
        self.digests
            .insert(source.clone(), digest(&Err(format!("withheld: {reason}"))));
        self.withheld.insert(source, reason);
    }

    /// Why `source` has no data, when the host would not ask it.
    pub fn withheld(&self, source: &str) -> Option<&str> {
        self.withheld.get(source).map(String::as_str)
    }

    /// Each source a surface shows now, with the checkout it is about:
    /// an open view's first, then the sections' and drawers' on the
    /// checkout in front. A source shown on two checkouts at once is asked
    /// about the first, so the two never take turns asking.
    fn shown_sources(&self) -> Vec<(String, PathBuf)> {
        let surfaces = &self.compiled.manifest.surfaces;
        let opened = surfaces
            .iter()
            .filter_map(|surface| Some((surface, self.opened.get(&surface.id)?.clone())));
        let in_front = surfaces
            .iter()
            .filter(|surface| surface.slot != uze_contract::Slot::View)
            .filter_map(|surface| Some((surface, self.in_front.clone()?)));
        let mut shown: Vec<(String, PathBuf)> = Vec::new();
        for (surface, checkout) in opened.chain(in_front) {
            if !shown.iter().any(|(source, _)| *source == surface.source) {
                shown.push((surface.source.clone(), checkout));
            }
        }
        shown
    }

    /// The input `source` is asked with: what it declares, as written.
    fn input_of(&self, source: &str) -> serde_json::Value {
        let declared = self.compiled.manifest.source(source);
        serde_json::Value::Object(
            declared
                .map(|declared| {
                    declared
                        .input
                        .iter()
                        .map(|(name, text)| (name.clone(), serde_json::Value::String(text.clone())))
                        .collect()
                })
                .unwrap_or_default(),
        )
    }

    /// The job asking `source` about `checkout`, numbered so its answer is
    /// taken only while it is the latest.
    fn ask(&mut self, source: String, checkout: PathBuf, input: serde_json::Value) -> Job {
        self.questions += 1;
        let number = self.questions;
        self.asked.insert(
            source.clone(),
            Asked {
                checkout: checkout.clone(),
                input: input.clone(),
                number,
            },
        );
        let question = Question {
            identity: self.compiled.identity.clone(),
            declaration: source.clone(),
            input,
            checkout: Some(checkout),
        };
        let stopped = source.clone();
        Job {
            run: Box::new(move |host| {
                Opaque::new(SourceAnswer {
                    source,
                    number,
                    answer: host.answer(question),
                })
            }),
            fallback: Box::new(move || {
                Opaque::new(SourceAnswer {
                    source: stopped,
                    number,
                    answer: Answer::Failed {
                        reason: STOPPED.to_owned(),
                        trace: Vec::new(),
                    },
                })
            }),
        }
    }

    pub fn data(&self, source: &str) -> Option<&SourceData> {
        self.data.get(source)
    }

    pub fn ui(&self, surface: &str) -> Option<&BTreeMap<String, String>> {
        self.ui.get(surface)
    }

    /// Lets go of where the viewer was on `surface`, as on a surface never
    /// opened: what a host hands a drawer arriving in a space it was never
    /// opened in, since one engine answers for every space.
    pub fn forget(&mut self, surface: &str) {
        self.ui.remove(surface);
        self.scroll.remove(surface);
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn recorded(&self) -> &[RecordedAction] {
        &self.recorded
    }

    /// How many times a surface was bound since this extension was made.
    pub fn builds(&self) -> u64 {
        self.builds.get()
    }

    /// How `surface` came to look as it does at its last build: every
    /// compiled node, whether it was drawn and why not, and the actions
    /// pressed on it. `None` for a surface the extension does not declare.
    pub fn trace(&self, surface: &str) -> Option<Trace> {
        let space = self.last_space(surface);
        let (decl, _) = self.declaration(surface)?;
        let data = match (self.withheld.get(&decl.source), self.data.get(&decl.source)) {
            (Some(reason), _) => DataState::Withheld(Clean::label(reason).into_string()),
            (None, None) => DataState::Waiting,
            (None, Some(Err(reason))) => DataState::Failed(Clean::label(reason).into_string()),
            (None, Some(Ok(_))) => DataState::Bound,
        };
        let recorded = self
            .recorded
            .iter()
            .filter(|action| action.surface == surface)
            .cloned()
            .collect();
        self.with_built(surface, space, |built| Trace {
            data,
            items: built.items.clone(),
            templates: built.templates.clone(),
            folded: built.folded,
            recorded,
        })
    }

    fn declaration(&self, surface: &str) -> Option<(&SurfaceDecl, &CompiledView)> {
        let decl = self.compiled.manifest.surface(surface)?;
        Some((decl, self.compiled.views.get(&decl.view)?))
    }

    fn title(&self, decl: &SurfaceDecl) -> String {
        let title = decl.title.as_ref().unwrap_or(&self.compiled.manifest.name);
        Clean::label(title).into_string()
    }

    /// A section has no size of its own to bind against; the host lays it
    /// out in whatever the column has.
    fn space_for(&self, surface: &str, space: Size) -> Size {
        match self
            .compiled
            .manifest
            .surface(surface)
            .map(|decl| decl.slot)
        {
            Some(uze_contract::Slot::View | uze_contract::Slot::Drawer) => space,
            _ => Size::default(),
        }
    }

    /// The furthest `surface` scrolls: the tree takes no more rows than it
    /// has nodes, which bounds a scroll the host would clamp anyway.
    fn scroll_bound(&self, surface: &str) -> usize {
        let space = self.last_space(surface);
        self.with_built(surface, space, |built| built.count)
            .unwrap_or(0)
    }

    fn last_space(&self, surface: &str) -> Size {
        self.memo
            .borrow()
            .get(surface)
            .map(|memo| memo.key.space)
            .unwrap_or_default()
    }

    /// Runs `answer` on the build of `surface` for `space`, building it
    /// first unless the last build had the same inputs.
    fn with_built<R>(
        &self,
        surface: &str,
        space: Size,
        answer: impl FnOnce(&Built) -> R,
    ) -> Option<R> {
        let (decl, view) = self.declaration(surface)?;
        let held = self.ui.get(surface);
        let key = MemoKey {
            generation: self.generation,
            digest: self.digests.get(&decl.source).copied(),
            space,
            ui: held.cloned().unwrap_or_default(),
        };
        let mut memo = self.memo.borrow_mut();
        if memo.get(surface).is_none_or(|last| last.key != key) {
            let built = self.build(decl, view, held, space);
            self.builds.set(self.builds.get() + 1);
            memo.insert(surface.to_owned(), Memo { key, built });
        }
        memo.get(surface).map(|last| answer(&last.built))
    }

    fn build(
        &self,
        decl: &SurfaceDecl,
        view: &CompiledView,
        held: Option<&BTreeMap<String, String>>,
        space: Size,
    ) -> Built {
        if let Some(reason) = self.withheld.get(&decl.source) {
            return Built::notice(view, reason, Role::Warning);
        }
        let data = match self.data.get(&decl.source) {
            None => return Built::notice(view, "Waiting for data", Role::Muted),
            Some(Err(reason)) => return Built::notice(view, reason, Role::Danger),
            Some(Ok(data)) => data,
        };
        let ui = bind::effective_ui(view, held);
        let context = serde_json::json!({
            "size": { "width": space.width, "height": space.height },
            "slot": decl.slot.name(),
            "locale": self.locale,
        });
        let env = value::Env {
            ui: &ui,
            context: &context,
            lookups: &view.lookups,
            locale: &self.locale,
        };
        bind::build(view, data, env)
    }

    /// What a gesture on node `node` means at the last build.
    fn act(&self, surface: &str, space: Size, node: usize) -> Option<Act> {
        self.with_built(surface, space, |built| built.acts.get(&node).cloned())
            .flatten()
    }

    /// The first interactive node of a kind, in the order it is drawn.
    fn first_act(&self, surface: &str, space: Size, kind: fn(&Act) -> bool) -> Option<Act> {
        self.with_built(surface, space, |built| {
            built.acts.values().find(|act| kind(act)).cloned()
        })
        .flatten()
    }

    fn set_ui(&mut self, surface: &str, key: String, value: String) {
        self.ui
            .entry(surface.to_owned())
            .or_default()
            .insert(key, value);
    }

    fn perform(&mut self, surface: &str, act: Act, gesture: NodeAct) -> Outcome {
        match (act, gesture) {
            (Act::Tabs { key, values, .. }, NodeAct::Tab(index)) => {
                if let Some(value) = values.get(index) {
                    self.set_ui(surface, key, value.clone());
                }
                Outcome::Stay
            }
            (Act::Toggle { key, on }, NodeAct::Toggle) => {
                self.set_ui(surface, key, (!on).to_string());
                Outcome::Stay
            }
            (Act::Button { action, item }, NodeAct::Press) => self.record(surface, action, item),
            (Act::List { key, rows, .. }, NodeAct::Row(row)) if row < rows => {
                self.set_ui(surface, key, row.to_string());
                Outcome::Stay
            }
            _ => Outcome::Stay,
        }
    }

    fn record(&mut self, surface: &str, action: String, item: serde_json::Value) -> Outcome {
        let said = format!("{} recorded, not run", Clean::label(&action));
        self.recorded.push(RecordedAction {
            surface: surface.to_owned(),
            action,
            item,
        });
        Outcome::Notice(Span::new(said, Role::Muted))
    }

    /// The next (or previous) tab of the surface's first `Tabs`.
    fn cycle_tab(&mut self, surface: &str, space: Size, forward: bool) {
        let first = self.first_act(surface, space, |act| matches!(act, Act::Tabs { .. }));
        let Some(Act::Tabs {
            key,
            values,
            active,
        }) = first
        else {
            return;
        };
        if values.is_empty() {
            return;
        }
        let next = if forward {
            (active + 1) % values.len()
        } else {
            (active + values.len() - 1) % values.len()
        };
        self.set_ui(surface, key, values[next].clone());
    }

    /// The next (or previous) row of the surface's first `List`.
    fn move_selection(&mut self, surface: &str, space: Size, forward: bool) {
        let first = self.first_act(surface, space, |act| matches!(act, Act::List { .. }));
        let Some(Act::List {
            key,
            rows,
            selected,
        }) = first
        else {
            return;
        };
        if rows == 0 {
            return;
        }
        let row = match (selected, forward) {
            (None, _) => 0,
            (Some(row), true) => (row + 1).min(rows - 1),
            (Some(row), false) => row.saturating_sub(1),
        };
        self.set_ui(surface, key, row.to_string());
    }
}

impl Extension for DeclaredExtension {
    fn identity(&self) -> String {
        self.compiled.identity.to_string()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        self.compiled
            .manifest
            .surfaces
            .iter()
            .map(|surface| SurfaceInfo {
                id: surface.id.clone(),
                slot: match surface.slot {
                    uze_contract::Slot::View => SurfaceSlot::View,
                    uze_contract::Slot::Section => SurfaceSlot::Section,
                    uze_contract::Slot::Drawer => SurfaceSlot::Drawer,
                },
                title: self.title(surface),
            })
            .collect()
    }

    fn view(&self, surface: &str, space: Size) -> Option<View> {
        let (decl, _) = self.declaration(surface)?;
        if decl.slot == uze_contract::Slot::Section {
            return None;
        }
        let title = self.title(decl);
        let scroll = self.scroll.get(surface).copied().unwrap_or(0);
        let root = self.with_built(surface, space, |built| built.root.clone())?;
        Some(View {
            title: vec![Span::new(title, Role::Muted)],
            caption: Vec::new(),
            navigator: None,
            content: Content::Tree {
                heading: String::new(),
                scroll,
                root,
            },
            footer: vec![Command::Close],
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: Layout::default(),
            trail: Vec::new(),
        })
    }

    fn section(&self, surface: &str, collapsed: bool) -> Option<Section> {
        let (decl, _) = self.declaration(surface)?;
        if decl.slot != uze_contract::Slot::Section {
            return None;
        }
        let root = self.with_built(surface, Size::default(), |built| built.root.clone())?;
        Some(Section {
            title: self.title(decl),
            caption: Span::default(),
            collapsed,
            scroll: 0,
            rows: Vec::new(),
            tree: Some(root),
            lit: Vec::new(),
        })
    }

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome {
        let space = self.space_for(surface, space);
        match command {
            Command::Close => return Outcome::Close,
            Command::NextView => self.cycle_tab(surface, space, true),
            Command::PreviousView => self.cycle_tab(surface, space, false),
            Command::SelectNext => self.move_selection(surface, space, true),
            Command::SelectPrevious => self.move_selection(surface, space, false),
            _ => {}
        }
        Outcome::Stay
    }

    fn hit(&mut self, surface: &str, hit: ViewHit, space: Size) -> Outcome {
        let space = self.space_for(surface, space);
        match hit {
            ViewHit::Close => Outcome::Close,
            ViewHit::Node { node, act } => match self.act(surface, space, node) {
                Some(meaning) => self.perform(surface, meaning, act),
                None => Outcome::Stay,
            },
            _ => Outcome::Stay,
        }
    }

    fn scroll(
        &mut self,
        surface: &str,
        target: ScrollTarget,
        direction: ScrollDirection,
        _space: Size,
    ) {
        if target != ScrollTarget::Content {
            return;
        }
        let bound = self.scroll_bound(surface);
        let scroll = self.scroll.entry(surface.to_owned()).or_default();
        *scroll = match direction {
            ScrollDirection::Down => (*scroll + 1).min(bound),
            ScrollDirection::Up => scroll.saturating_sub(1),
        };
    }

    fn scroll_to(&mut self, surface: &str, first: usize, _space: Size) {
        let bound = self.scroll_bound(surface);
        self.scroll.insert(surface.to_owned(), first.min(bound));
    }

    /// Its source is asked again: what the operator allowed may have
    /// changed since it was last asked.
    fn open(&mut self, surface: &str, checkout: &Checkout) {
        let Some(decl) = self.compiled.manifest.surface(surface) else {
            return;
        };
        self.asked.remove(&decl.source);
        self.opened
            .insert(surface.to_owned(), checkout.root.clone());
    }

    fn close(&mut self, surface: &str) {
        self.opened.remove(surface);
    }

    fn focus(&mut self, checkout: Option<&Checkout>) {
        self.in_front = checkout.map(|checkout| checkout.root.clone());
    }

    /// A question for each source a surface shows whose latest question
    /// was about another checkout or input, or that was never asked.
    fn jobs(&mut self) -> Vec<Job> {
        let mut jobs = Vec::new();
        for (source, checkout) in self.shown_sources() {
            let input = self.input_of(&source);
            let current = self
                .asked
                .get(&source)
                .is_some_and(|asked| asked.checkout == checkout && asked.input == input);
            if !current {
                jobs.push(self.ask(source, checkout, input));
            }
        }
        jobs
    }

    /// An answer to a question that is no longer the latest is dropped.
    /// A failure is drawn in place of the data, said as the host said it.
    fn absorb(&mut self, answer: Opaque) -> Outcome {
        let Some(answered) = answer.downcast::<SourceAnswer>() else {
            return Outcome::Stay;
        };
        let latest = self.asked.get(&answered.source).map(|asked| asked.number);
        if latest != Some(answered.number) {
            return Outcome::Stay;
        }
        match answered.answer {
            Answer::Answered { envelope, .. } => self.set_data(answered.source, Ok(envelope.data)),
            Answer::Failed { reason, .. } => self.set_data(answered.source, Err(reason)),
            Answer::Withheld { reason } => self.withhold(answered.source, reason),
        }
        Outcome::Stay
    }

    fn place(&self, surface: &str) -> Option<Opaque> {
        self.declaration(surface)?;
        Some(Opaque::new(Place {
            ui: self.ui.get(surface).cloned().unwrap_or_default(),
            scroll: self.scroll.get(surface).copied().unwrap_or(0),
        }))
    }

    fn resume(&mut self, surface: &str, place: Opaque) {
        let Some(place) = place.downcast::<Place>() else {
            return;
        };
        self.ui.insert(surface.to_owned(), place.ui);
        self.scroll.insert(surface.to_owned(), place.scroll);
    }

    fn memory(&self) -> Option<Opaque> {
        Some(Opaque::new(Memory {
            recorded: self.recorded.clone(),
        }))
    }

    fn restore_memory(&mut self, memory: Opaque) {
        if let Some(memory) = memory.downcast::<Memory>() {
            self.recorded = memory.recorded;
        }
    }
}

/// A digest of a source's answer. Only ever compared within one process,
/// so the standard hasher's lack of a stable output does not matter.
fn digest(data: &SourceData) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match data {
        Ok(value) => {
            0_u8.hash(&mut hasher);
            serde_json::to_string(value)
                .unwrap_or_default()
                .hash(&mut hasher);
        }
        Err(reason) => {
            1_u8.hash(&mut hasher);
            reason.hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod board_tests;

#[cfg(test)]
mod source_tests;
