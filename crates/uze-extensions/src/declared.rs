//! A declared extension: one a plugin wrote in KDL, hosted through the
//! [`Extension`] contract.
//!
//! The engine holds what it was handed (the compiled extension and its
//! sources' data) and the UI state the host keeps for it, and binds the one
//! to the other into a [`View`] or a [`Section`] on demand. It parses
//! nothing: what reaches it was compiled by `uze-core` and read by the host.
//!
//! - `bind`: the binder, compiled view + data + UI state into a node tree.
//! - `action`: what a declared action does once a gesture reaches it.
//! - `value`: how a binding becomes a value (D23).
//! - `trace`: the inspector's account of a build.
//! - `time`: the instant an `Elapsed` counts from.
//! - `events`: what asks a source again beside its surfaces.
//!
//! What a source answers arrives one way: the engine asks the host, through
//! a [`Job`], to answer the source with its input about the checkout the
//! surface showing it is on, and takes the answer only while it is the
//! latest question asked of that source. What an answer says beside its
//! data is applied as it arrives: the sources it names are asked again,
//! its notice is said on the surfaces showing it, its toast is the host's.
//! A source is also asked again on its `every` while a surface showing it
//! is in view, and for the events its `refresh-on` names, which the host
//! watches because [`Extension::watches`] says so; an event while nothing
//! showing the source is in view is remembered, and asked once one is.

mod action;
mod bind;
mod events;
mod fixture_check;
mod time;
mod trace;
mod value;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::Instant;

use uze_contract::{Answer, Asking, Clean, CompiledExtension, CompiledView, Question, SurfaceDecl};

use crate::contract::{
    Checkout, CheckoutEvent, Extension, Job, Opaque, Outcome, Pass, SurfaceInfo, SurfaceSlot,
};
use crate::view::{
    Command, Content, Layout, NodeAct, Role, ScrollDirection, ScrollTarget, Section, Size, Span,
    View, ViewHit,
};
pub use action::COPY_UNASKED_MAX;
use action::{ActionAnswer, Pending, Queued};
pub use bind::NODE_CAP;
use bind::{Act, Built};
pub use events::COALESCE;
pub use fixture_check::{MissingField, TAB_COMBINATIONS_MAX, missing_fields};
pub use trace::{Absence, DataState, Trace, TraceEntry, TraceNode};

/// What a source answered: its data, or why there is none.
pub type SourceData = Result<serde_json::Value, String>;

/// A gesture that reached an action, kept for the inspector: one the
/// manifest declares ran (or was asked about), one it does not ran nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedAction {
    pub surface: String,
    pub action: String,
    /// The `item` the action received, as the data holds it: whoever
    /// draws it cleans it first, as every other string.
    pub item: serde_json::Value,
    /// Whether the manifest declares it, so it ran.
    pub declared: bool,
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
    /// Sources a refresh asked again, which the next question asks past
    /// any answer the host remembers.
    stale: BTreeSet<String>,
    /// Of those, the ones another source's answer asked again.
    refreshed_by_answer: BTreeSet<String>,
    /// The action a question stands on, waiting for its answer.
    pending: Option<Pending>,
    /// Actions' scripts to hand to the host, in the order pressed.
    queued: Vec<Queued>,
    /// What the last answer said on each surface, by surface.
    notices: BTreeMap<String, Span>,
    /// The host's clock at its last pass, which a question is stamped
    /// with; `None` until a host counts time.
    now: Option<Instant>,
    /// Events gathered for their window to close.
    window: events::Window,
    /// Sources an event or their `every` asked again, asked once a surface
    /// showing them is in view.
    due: BTreeSet<String>,
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
    /// When it was asked, on the host's clock: what `every` counts from.
    at: Option<Instant>,
    /// A refresh asked by another source's answer: its own answer asks
    /// nothing again, so two sources cannot keep asking each other.
    by_answer: bool,
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
            stale: BTreeSet::new(),
            refreshed_by_answer: BTreeSet::new(),
            pending: None,
            queued: Vec::new(),
            notices: BTreeMap::new(),
            now: None,
            window: events::Window::default(),
            due: BTreeSet::new(),
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

    /// What the operator allowed it changed: consent, or enabling or
    /// disabling it in a project. Every source shown is asked again, and a
    /// reason it was withheld for no longer stands, so it waits for the
    /// new answer instead of saying the old one.
    pub fn reconsider(&mut self) {
        self.asked.clear();
        for source in std::mem::take(&mut self.withheld).into_keys() {
            self.digests.remove(&source);
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
    fn shown_sources(&self) -> Vec<(String, PathBuf, String)> {
        let surfaces = &self.compiled.manifest.surfaces;
        let opened = surfaces
            .iter()
            .filter_map(|surface| Some((surface, self.opened.get(&surface.id)?.clone())));
        let in_front = surfaces
            .iter()
            .filter(|surface| surface.slot != uze_contract::Slot::View)
            .filter_map(|surface| Some((surface, self.in_front.clone()?)));
        let mut shown: Vec<(String, PathBuf, String)> = Vec::new();
        for (surface, checkout) in opened.chain(in_front) {
            if !shown.iter().any(|(source, ..)| *source == surface.source) {
                shown.push((surface.source.clone(), checkout, surface.id.clone()));
            }
        }
        shown
    }

    /// The input `source` is asked with on `surface`: what it declares,
    /// each `{field}` resolved as a view's text is (D23), against the item
    /// selected there, `ui.*` and `context.*`. A lone reference keeps its
    /// type; one to nothing is `null`.
    fn input_of(&self, source: &str, surface: &str) -> serde_json::Value {
        let Some(declared) = self.compiled.manifest.source(source) else {
            return serde_json::Value::Object(serde_json::Map::new());
        };
        let item = action::template_scope(&self.selected_item(surface));
        serde_json::Value::Object(
            declared
                .input
                .iter()
                .map(|(name, template)| {
                    let value = self.bind_template(surface, template, &item);
                    (name.clone(), value)
                })
                .collect(),
        )
    }

    /// `template` bound against `scope` as an item, with `surface`'s UI
    /// state and lookups. Text that is not a template is itself.
    fn bind_template(
        &self,
        surface: &str,
        template: &str,
        scope: &serde_json::Value,
    ) -> serde_json::Value {
        let Ok(parts) = uze_contract::binding::parse_text(template) else {
            return serde_json::Value::String(template.to_owned());
        };
        let binding = uze_contract::binding::binding_of(parts);
        let view = self.declaration(surface).map(|(_, view)| view);
        let ui = view
            .map(|view| bind::effective_ui(view, self.ui.get(surface)))
            .unwrap_or_default();
        let context = serde_json::json!({
            "checkout": self.checkout_text(surface),
            "slot": self
                .compiled
                .manifest
                .surface(surface)
                .map(|decl| decl.slot.name()),
            "locale": self.locale,
        });
        let env = value::Env {
            ui: &ui,
            context: &context,
            lookups: view.map_or(&[], |view| view.lookups.as_slice()),
            locale: &self.locale,
        };
        env.bind(&[scope], &binding)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or(serde_json::Value::Null)
    }

    /// `template` resolved to text against `item`.
    fn resolve_text(&self, surface: &str, template: &str, item: &serde_json::Value) -> String {
        let scope = action::template_scope(item);
        value::scalar_text(&self.bind_template(surface, template, &scope))
    }

    fn checkout_text(&self, surface: &str) -> Option<String> {
        self.opened
            .get(surface)
            .or(self.in_front.as_ref())
            .map(|root| root.display().to_string())
    }

    /// The item of the row selected in `surface`'s first list, at its
    /// last build; `null` when nothing is selected.
    fn selected_item(&self, surface: &str) -> serde_json::Value {
        let space = self.last_space(surface);
        let first = self.first_act(surface, space, |act| matches!(act, Act::List { .. }));
        match first {
            Some(Act::List {
                selected: Some(row),
                items,
                ..
            }) => items.get(row).cloned().unwrap_or_default(),
            _ => serde_json::Value::Null,
        }
    }

    /// What a surface is called where it is named.
    fn surface_title(&self, surface: &str) -> String {
        match self.compiled.manifest.surface(surface) {
            Some(decl) => self.title(decl),
            None => Clean::label(surface).into_string(),
        }
    }

    /// The job asking `source` about `checkout`, numbered so its answer is
    /// taken only while it is the latest.
    fn ask(
        &mut self,
        source: String,
        checkout: PathBuf,
        input: serde_json::Value,
        asking: Asking,
        by_answer: bool,
    ) -> Job {
        self.questions += 1;
        let number = self.questions;
        self.asked.insert(
            source.clone(),
            Asked {
                checkout: checkout.clone(),
                input: input.clone(),
                number,
                at: self.now,
                by_answer,
            },
        );
        let question = Question {
            identity: self.compiled.identity.clone(),
            declaration: source.clone(),
            asking,
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

    /// The sources shown on a surface in `visible`.
    fn sources_in_view(&self, visible: &[String]) -> BTreeSet<String> {
        self.compiled
            .manifest
            .surfaces
            .iter()
            .filter(|surface| visible.contains(&surface.id))
            .map(|surface| surface.source.clone())
            .collect()
    }

    /// Whether `source`'s `every` has passed since it was last asked.
    fn every_passed(&self, source: &str, now: Instant) -> bool {
        let every = self
            .compiled
            .manifest
            .source(source)
            .and_then(|declared| declared.every);
        let asked = self.asked.get(source).and_then(|asked| asked.at);
        matches!((every, asked), (Some(every), Some(at)) if now.saturating_duration_since(at) >= every)
    }

    /// Gathers each shown source that `happened` is a reason for, on the
    /// checkout it is shown on.
    fn gather(&mut self, happened: &CheckoutEvent, now: Instant) {
        for (source, checkout, _) in self.shown_sources() {
            let reason = checkout == happened.checkout
                && self
                    .compiled
                    .manifest
                    .source(&source)
                    .is_some_and(|declared| {
                        declared
                            .refresh_on
                            .iter()
                            .any(|event| events::is_reason(event, &happened.event))
                    });
            if reason {
                self.window.add(source, now);
            }
        }
    }

    pub fn data(&self, source: &str) -> Option<&SourceData> {
        self.data.get(source)
    }

    pub fn ui(&self, surface: &str) -> Option<&BTreeMap<String, String>> {
        self.ui.get(surface)
    }

    /// The surfaces drawn from `source`.
    fn surfaces_showing(&self, source: &str) -> Vec<String> {
        self.compiled
            .manifest
            .surfaces
            .iter()
            .filter(|surface| surface.source == source)
            .map(|surface| surface.id.clone())
            .collect()
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

    /// The declared `action` asked for by a key or the index while
    /// `surface` is in front, on the item selected there. One the manifest
    /// does not offer on `surface` is nothing, and nothing runs while a
    /// question stands.
    pub fn press(&mut self, surface: &str, action: &str) -> Outcome {
        let offered = self
            .compiled
            .manifest
            .action(action)
            .is_some_and(|declared| declared.on == surface);
        if !offered || self.pending.is_some() {
            return Outcome::Stay;
        }
        let item = self.selected_item(surface);
        self.act_on(surface, action, item)
    }

    /// The question standing on `surface`, when one does.
    pub fn asking(&self, surface: &str) -> Option<&crate::view::Confirm> {
        self.pending
            .as_ref()
            .filter(|pending| pending.surface == surface)
            .map(|pending| &pending.confirm)
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
            (Act::Button { action, item }, NodeAct::Press) => self.act_on(surface, &action, item),
            (Act::List { key, rows, .. }, NodeAct::Row(row)) if row < rows => {
                self.set_ui(surface, key, row.to_string());
                Outcome::Stay
            }
            _ => Outcome::Stay,
        }
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
            ..
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
            notice: self.notices.get(surface).cloned(),
            confirm: self.asking(surface).cloned(),
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
            caption: self.notices.get(surface).cloned().unwrap_or_default(),
            collapsed,
            scroll: 0,
            rows: Vec::new(),
            tree: Some(root),
            lit: Vec::new(),
        })
    }

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome {
        if let Some(outcome) = self.command_while_asking(surface, &command) {
            return outcome;
        }
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
        if let Some(outcome) = self.hit_while_asking(surface, &hit) {
            return outcome;
        }
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

    /// A question standing on it goes unanswered, so nothing runs.
    fn close(&mut self, surface: &str) {
        self.opened.remove(surface);
        self.pending.take_if(|pending| pending.surface == surface);
        self.notices.remove(surface);
    }

    fn focus(&mut self, checkout: Option<&Checkout>) {
        self.in_front = checkout.map(|checkout| checkout.root.clone());
    }

    /// Each checkout a source declaring `refresh-on` is shown on, with
    /// what it declares.
    fn watches(&self) -> Vec<CheckoutEvent> {
        let watched = self
            .shown_sources()
            .into_iter()
            .flat_map(|(source, checkout, _)| {
                let declared = self.compiled.manifest.source(&source);
                declared
                    .into_iter()
                    .flat_map(|declared| declared.refresh_on.iter().cloned())
                    .map(move |event| (checkout.clone(), event))
                    .collect::<Vec<_>>()
            });
        events::merged(watched)
    }

    /// Events are gathered for their window; what it gathered, and every
    /// source in view whose `every` passed, is asked again past what the
    /// host remembers, once a surface showing it is in view.
    fn pass(&mut self, pass: &Pass<'_>) {
        self.now = Some(pass.now);
        for happened in pass.happened {
            self.gather(happened, pass.now);
        }
        let gathered = self.window.take(pass.now);
        self.due.extend(gathered);
        let in_view = self.sources_in_view(pass.visible);
        let every: Vec<String> = in_view
            .iter()
            .filter(|source| self.every_passed(source, pass.now))
            .cloned()
            .collect();
        self.due.extend(every);
        let asked: Vec<String> = self
            .due
            .iter()
            .filter(|source| in_view.contains(*source))
            .cloned()
            .collect();
        for source in &asked {
            self.due.remove(source);
        }
        self.refresh(&asked, None);
    }

    /// The actions' scripts pressed since, then a question for each source
    /// a surface shows whose latest question was about another checkout or
    /// input, that a refresh asked again, or that was never asked.
    fn jobs(&mut self) -> Vec<Job> {
        let mut jobs = self.action_jobs();
        for (source, checkout, surface) in self.shown_sources() {
            let input = self.input_of(&source, &surface);
            let current = self
                .asked
                .get(&source)
                .is_some_and(|asked| asked.checkout == checkout && asked.input == input);
            if current {
                continue;
            }
            let (asking, by_answer) = match self.stale.take(&source) {
                Some(_) => (Asking::Refresh, self.refreshed_by_answer.remove(&source)),
                None => (Asking::Source, false),
            };
            jobs.push(self.ask(source, checkout, input, asking, by_answer));
        }
        jobs
    }

    /// An answer to a question that is no longer the latest is dropped.
    /// A failure is drawn in place of the data, said as the host said it.
    /// What the envelope says beside its data is applied with it.
    fn absorb(&mut self, answer: Opaque) -> Outcome {
        let answer = match answer.0.downcast::<ActionAnswer>() {
            Ok(answered) => return self.absorb_action(*answered),
            Err(other) => other,
        };
        let Ok(answered) = answer.downcast::<SourceAnswer>() else {
            return Outcome::Stay;
        };
        let Some(asked) = self.asked.get(&answered.source) else {
            return Outcome::Stay;
        };
        if asked.number != answered.number {
            return Outcome::Stay;
        }
        let by_answer = asked.by_answer;
        let source = answered.source;
        match answered.answer {
            Answer::Answered { envelope, .. } => {
                self.set_data(source.clone(), Ok(envelope.data.clone()));
                let surfaces = self.surfaces_showing(&source);
                for surface in &surfaces {
                    self.notices.remove(surface);
                }
                let mut asks = envelope.clone();
                if by_answer {
                    asks.refresh.clear();
                }
                for refreshed in &asks.refresh {
                    self.refreshed_by_answer.insert(refreshed.clone());
                }
                self.apply(&asks, &surfaces, Some(&source))
            }
            Answer::Failed { reason, .. } => {
                self.set_data(source, Err(reason));
                Outcome::Stay
            }
            Answer::Withheld { reason } => {
                self.withhold(source, reason);
                Outcome::Stay
            }
        }
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

#[cfg(test)]
mod action_tests;

#[cfg(test)]
mod event_tests;
