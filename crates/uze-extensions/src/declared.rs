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
//! - `notify`: what an answer asks to tell the operator outside the frame.
//!
//! What a source answers arrives one way: the engine asks the host, through
//! a [`Job`], to answer the source with its input about the checkout the
//! surface showing it is on, and takes the answer only while it is the
//! latest question asked of that source. What an answer says beside its
//! data is applied as it arrives: the sources it names are asked again,
//! its notice is said on the surfaces showing it, its toast is the host's,
//! and its notification and cue are the host's to send, under `notify`
//! and within the extension's rate.
//! A source is also asked again on its `every` while a surface showing it
//! is in view, and for the events its `refresh-on` names, which the host
//! watches because [`Extension::watches`] says so; an event while nothing
//! showing the source is in view is remembered, and asked once one is.

mod action;
mod authoring;
mod bind;
mod compose;

/// Whether `tree`, a composed source's answer, is one the workspace would
/// draw for `manifest`, or the first reason it is not: what `uze agent
/// plugin check --run` says of a composed source.
pub fn validate_composed(
    tree: &serde_json::Value,
    manifest: &uze_contract::Manifest,
) -> Result<(), String> {
    compose::compose(tree, manifest, "").map(drop)
}
mod applies;
mod events;
mod fixture_check;
mod input;
mod media;
mod notify;
mod resident;
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
use action::{ActionAnswer, Pending, Queued};
pub use action::{COPY_UNASKED_MAX, RECORDED_MAX};
pub use applies::RECHECK as APPLIES_RECHECK;
pub use authoring::fake_agents;
pub use bind::NODE_CAP;
use bind::{Act, Built};
pub use events::COALESCE;
pub use fixture_check::{MissingField, TAB_COMBINATIONS_MAX, missing_fields};
pub use input::FIELD_MAX;
pub use media::{IMAGE_FILE_MAX, MEDIA_BYTES_MAX};
pub use notify::{INTERVAL as NOTIFY_INTERVAL, PER_HOUR as NOTIFY_PER_HOUR};
pub use resident::{INPUT_QUEUED, RENDERS_PER_SECOND};
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

/// The item a surface's selection stood on when it was taken, and what
/// it was taken under: a change of either takes it again.
#[derive(Clone, Debug)]
struct Chosen {
    row: usize,
    gestures: BTreeMap<String, String>,
    item: serde_json::Value,
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
    /// The checkout each source's data was answered about, by source id;
    /// none for data no checkout answered (a fixture, events, agents).
    answered_for: BTreeMap<String, PathBuf>,
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
    /// When each source's answer last changed, on the host's clock: what a
    /// click on a composed surface is held back by.
    laid_out: BTreeMap<String, Instant>,
    /// The events waiting for the sources they ask again.
    delivery: events::Delivery,
    /// The workspace's agents as last handed over, for `agents` sources.
    agents: serde_json::Value,
    /// Prompts its agent events sent, waiting for the host to take.
    unattended: Vec<crate::contract::AgentRequest>,
    /// Notifications let through, waiting for the host to take.
    outgoing: Vec<crate::contract::Notify>,
    /// How many notifications went out lately, and how many were held back.
    rate: notify::Rate,
    /// The number of the last question each source answered.
    answered: BTreeMap<String, u64>,
    /// The events this extension saw, newest first, at most
    /// [`uze_contract::EVENTS_MAX`]: what an `events` source answers.
    log: std::collections::VecDeque<serde_json::Value>,
    /// Events gathered for their window to close.
    window: events::Window,
    /// Sources an event or their `every` asked again, asked once a surface
    /// showing them is in view.
    due: BTreeSet<String>,
    /// The surfaces in view at the host's last pass; `None` until a host
    /// passes, when every build counts as drawn.
    in_view: Option<Vec<String>>,
    /// How many questions were asked, numbering each.
    questions: u64,
    /// The row selected in each surface's first list and its item as it was
    /// drawn when the operator chose it: what a source's input reads, so
    /// an answer renaming that row never feeds the next question.
    chosen: BTreeMap<String, Chosen>,
    /// `ui.<id>` values, by surface then id.
    ui: BTreeMap<String, BTreeMap<String, String>>,
    /// The first row shown, by surface.
    scroll: BTreeMap<String, usize>,
    locale: String,
    /// The operator's preferences, each admitted against its declaration:
    /// what `{pref.<name>}` reads.
    preferences: serde_json::Value,
    recorded: Vec<RecordedAction>,
    /// The resident sources' sessions, by source id.
    residents: BTreeMap<String, resident::Session>,
    /// Sessions closed since the last jobs: their `Stop`s to send, with
    /// the checkout and number each was opened with.
    closing: Vec<(String, PathBuf, u64)>,
    /// How many times a view was drawn, which a render waits on: at most
    /// one per frame.
    frames: std::cell::Cell<u64>,
    /// The last build of each surface. Behind a cell because drawing asks
    /// through `&self`; the contract needs `Send`, never `Sync`.
    memo: RefCell<BTreeMap<String, Memo>>,
    /// How many builds have run, so the inspector can say what drawing
    /// costs and a test can tell a served build from a fresh one.
    builds: std::cell::Cell<u64>,
    /// The pictures its views name, read one at a time.
    media: media::Shelf,
    /// Whether each checkout has each path an `applies-when` names.
    markers: applies::Markers,
    /// What the author of a linked extension did to it from the inspector.
    authoring: authoring::Authoring,
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

/// How long after a composed answer changed a click on its surface is
/// ignored.
const RELAYOUT_GUARD: std::time::Duration = std::time::Duration::from_millis(300);

/// What a source's job answers when its thread stopped before answering.
const STOPPED: &str = "the question stopped before it was answered";

/// What a build depends on. The same key is the same tree, so a frame that
/// changed nothing costs a comparison.
#[derive(Clone, Debug, PartialEq)]
struct MemoKey {
    generation: u64,
    /// The shelf's, so a picture read since is drawn.
    media: u64,
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
        let mut extension = Self::unanswered(compiled, locale);
        extension.answer_event_sources();
        extension
    }

    fn unanswered(compiled: CompiledExtension, locale: impl Into<String>) -> Self {
        Self {
            compiled,
            generation: 0,
            data: BTreeMap::new(),
            withheld: BTreeMap::new(),
            digests: BTreeMap::new(),
            answered_for: BTreeMap::new(),
            in_front: None,
            opened: BTreeMap::new(),
            asked: BTreeMap::new(),
            stale: BTreeSet::new(),
            refreshed_by_answer: BTreeSet::new(),
            pending: None,
            queued: Vec::new(),
            notices: BTreeMap::new(),
            now: None,
            answered: BTreeMap::new(),
            agents: serde_json::Value::Array(Vec::new()),
            unattended: Vec::new(),
            outgoing: Vec::new(),
            rate: notify::Rate::default(),
            delivery: events::Delivery::default(),
            log: std::collections::VecDeque::new(),
            laid_out: BTreeMap::new(),
            window: events::Window::default(),
            due: BTreeSet::new(),
            in_view: None,
            questions: 0,
            chosen: BTreeMap::new(),
            ui: BTreeMap::new(),
            scroll: BTreeMap::new(),
            locale: locale.into(),
            preferences: serde_json::Value::Object(serde_json::Map::new()),
            recorded: Vec::new(),
            residents: BTreeMap::new(),
            closing: Vec::new(),
            frames: std::cell::Cell::new(0),
            memo: RefCell::default(),
            builds: std::cell::Cell::new(0),
            media: media::Shelf::default(),
            markers: applies::Markers::default(),
            authoring: authoring::Authoring::default(),
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
        self.close_residents();
        self.answer_event_sources();
        self.memo.get_mut().clear();
        self.media.clear();
        self.asked.clear();
        self.chosen.clear();
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
        self.close_residents();
        for source in std::mem::take(&mut self.withheld).into_keys() {
            self.digests.remove(&source);
        }
    }

    pub fn set_data(&mut self, source: impl Into<String>, data: SourceData) {
        let source = source.into();
        self.withheld.remove(&source);
        self.answered_for.remove(&source);
        let digest = digest(&data);
        if self.digests.insert(source.clone(), digest) != Some(digest)
            && let Some(now) = self.now
        {
            self.laid_out.insert(source.clone(), now);
        }
        self.data.insert(source, data);
    }

    /// Whether a click on `surface` lands within [`RELAYOUT_GUARD`] of its
    /// composed source answering a new tree: what was under the pointer
    /// may not be what the operator aimed at (D15).
    fn just_laid_out(&self, surface: &str) -> bool {
        let Some(decl) = self.compiled.manifest.surface(surface) else {
            return false;
        };
        let composed = self
            .compiled
            .manifest
            .source(&decl.source)
            .is_some_and(|source| source.output == uze_contract::Output::View);
        composed
            && self
                .laid_out
                .get(&decl.source)
                .zip(self.now)
                .is_some_and(|(at, now)| now.saturating_duration_since(*at) < RELAYOUT_GUARD)
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
    /// chosen there, `ui.*` and `context.*`. A lone reference keeps its
    /// type; one to nothing is `null`.
    fn input_of(&self, source: &str, surface: &str) -> serde_json::Value {
        let Some(declared) = self.compiled.manifest.source(source) else {
            return serde_json::Value::Object(serde_json::Map::new());
        };
        let chosen = self
            .chosen
            .get(surface)
            .map(|chosen| chosen.item.clone())
            .unwrap_or_default();
        let item = action::template_scope(&chosen);
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
            preferences: &self.preferences,
        };
        env.bind(&[scope], &binding)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or(serde_json::Value::Null)
    }

    /// `template` resolved to text against `scope`, an action's item with
    /// what it reads beside it.
    fn resolve_scoped(&self, surface: &str, template: &str, scope: &serde_json::Value) -> String {
        value::scalar_text(&self.bind_template(surface, template, scope))
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

    /// Takes the item of the row selected in `surface`'s first list when
    /// the selection moved to another row, or a gesture (a filter typed, a
    /// choice) may have put another item under it, since it was last taken;
    /// never because an answer redrew the row under it: a source reading the
    /// selection of its own list would otherwise ask again on each of its
    /// answers. Rows still answered about another checkout give none.
    fn capture_selection(&mut self, surface: &str) {
        if self.rows_of_another_checkout(surface) {
            self.chosen.remove(surface);
            return;
        }
        let space = self.last_space(surface);
        // The row and its item only: the list itself is not copied each pass.
        let selected = self
            .with_built(surface, space, |built| {
                match built
                    .acts
                    .values()
                    .find(|act| matches!(act, Act::List { .. }))
                {
                    Some(Act::List {
                        selected: Some(row),
                        items,
                        ..
                    }) => Some((*row, items.get(*row).cloned())),
                    _ => None,
                }
            })
            .flatten();
        let Some((row, item)) = selected else {
            self.chosen.remove(surface);
            return;
        };
        let gestures = self.ui.get(surface).cloned().unwrap_or_default();
        let taken = self.chosen.get(surface);
        if taken.is_none_or(|chosen| chosen.row != row || chosen.gestures != gestures) {
            let item = item.unwrap_or_default();
            self.chosen.insert(
                surface.to_owned(),
                Chosen {
                    row,
                    gestures,
                    item,
                },
            );
        }
    }

    /// The pictures the surfaces in view at the last pass draw. A build
    /// out of view that names one is let go, so a folded section, a closed
    /// drawer or a hidden band or overlay holds nothing against the budget;
    /// one naming none is kept, so a surface drawn folded is not rebuilt.
    fn pictures_in_view(&mut self) -> BTreeSet<media::MediaKey> {
        let memo = self.memo.get_mut();
        if let Some(in_view) = &self.in_view {
            memo.retain(|surface, memo| in_view.contains(surface) || memo.built.media.is_empty());
        }
        memo.values()
            .flat_map(|memo| memo.built.media.iter().cloned())
            .collect()
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
        // What asked it again rides with the question and is no part of
        // what the question is remembered by: the next frame's input has no
        // event, and is the same question.
        let mut input = input;
        if matches!(asking, Asking::Refresh)
            && let Some(event) = self.delivery.take(&source)
            && let serde_json::Value::Object(fields) = &mut input
        {
            fields.insert("event".to_owned(), event);
        }
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
        self.gather_value(happened, events::event_value(happened), now);
    }

    /// [`Self::gather`], with what the sources are asked with as `event`.
    fn gather_value(&mut self, happened: &CheckoutEvent, event: serde_json::Value, now: Instant) {
        self.log.push_front(event.clone());
        self.log.truncate(usize::from(uze_contract::EVENTS_MAX));
        for (source, checkout, _) in self.shown_sources() {
            let Some(declared) = self.compiled.manifest.source(&source) else {
                continue;
            };
            let reason = checkout == happened.checkout
                && declared
                    .refresh_on
                    .iter()
                    .any(|declared| events::is_reason(declared, &happened.event));
            if reason {
                let each = declared.deliver == uze_contract::Deliver::Each;
                self.delivery.keep(&source, event.clone(), each);
                self.window.add(source, now);
            }
        }
        self.answer_event_sources();
    }

    /// What the workspace's agents did: logged, handed to each source whose
    /// `refresh-on` names it (on any checkout: an agent is the workspace's,
    /// not the surface's), and the unattended prompts it sends queued for
    /// the host, each aimed at the agent that did it.
    fn take_agent_events(&mut self, happened: &[crate::contract::AgentHappened], now: Instant) {
        for event in happened {
            let mut value = serde_json::json!({
                "name": event.event.name(),
                "agent": event.agent,
                "harness": event.harness,
                "checkout": event.checkout.display().to_string(),
            });
            if let (serde_json::Value::Object(fields), serde_json::Value::Object(detail)) =
                (&mut value, &event.detail)
            {
                for (key, detail) in detail {
                    fields.entry(key.clone()).or_insert_with(|| detail.clone());
                }
            }
            self.log.push_front(value.clone());
            let named = uze_contract::EventName::Agent(event.event);
            let reasons: Vec<(String, bool)> = self
                .compiled
                .manifest
                .sources
                .iter()
                .filter(|source| source.refresh_on.contains(&named))
                .map(|source| {
                    (
                        source.id.clone(),
                        source.deliver == uze_contract::Deliver::Each,
                    )
                })
                .collect();
            for (source, each) in reasons {
                self.delivery.keep(&source, value.clone(), each);
                self.window.add(source, now);
            }
            let prompts: Vec<crate::contract::AgentRequest> = self
                .compiled
                .manifest
                .actions
                .iter()
                .filter_map(|action| match &action.does {
                    uze_contract::Does::AgentPrompt {
                        prompt,
                        auto: Some(auto),
                        ..
                    } if auto.on == event.event => Some(crate::contract::AgentRequest {
                        what: match prompt {
                            uze_contract::PromptText::Text(text) => {
                                crate::contract::AgentAsk::Prompt { text: text.clone() }
                            }
                            uze_contract::PromptText::Skill { skill, args } => {
                                crate::contract::AgentAsk::Skill {
                                    skill: skill.clone(),
                                    args: args.clone(),
                                }
                            }
                        },
                        agent: Some(event.agent.clone()),
                        action: action.id.clone(),
                        label: Clean::label(&action.label).into_string(),
                        unattended: true,
                    }),
                    _ => None,
                })
                .collect();
            self.unattended.extend(prompts);
        }
        self.log.truncate(usize::from(uze_contract::EVENTS_MAX));
        self.answer_event_sources();
    }

    /// Every `events` source, answered from what this extension saw: the
    /// host is never asked about one.
    fn answer_event_sources(&mut self) {
        let sources: Vec<(String, u16)> = self
            .compiled
            .manifest
            .sources
            .iter()
            .filter_map(|source| match source.kind {
                uze_contract::SourceKind::Events(kept) => Some((source.id.clone(), kept)),
                _ => None,
            })
            .collect();
        for (source, kept) in sources {
            let events: Vec<serde_json::Value> =
                self.log.iter().take(usize::from(kept)).cloned().collect();
            self.set_data(source, Ok(serde_json::Value::Array(events)));
        }
    }

    pub fn data(&self, source: &str) -> Option<&SourceData> {
        self.data.get(source)
    }

    pub fn ui(&self, surface: &str) -> Option<&BTreeMap<String, String>> {
        self.ui.get(surface)
    }

    /// Whether `source`'s latest question is still out.
    fn awaiting(&self, source: &str) -> bool {
        self.stale.contains(source) || self.question_out(source)
    }

    /// Whether a question asked of `source` has not been answered yet.
    fn question_out(&self, source: &str) -> bool {
        self.asked
            .get(source)
            .is_some_and(|asked| self.answered.get(source) != Some(&asked.number))
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
        self.clean_title(decl).into_string()
    }

    fn clean_title(&self, decl: &SurfaceDecl) -> Clean {
        let title = decl.title.as_ref().unwrap_or(&self.compiled.manifest.name);
        Clean::label(title)
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
            Some(
                uze_contract::Slot::View
                | uze_contract::Slot::Drawer
                | uze_contract::Slot::Status
                | uze_contract::Slot::Band
                | uze_contract::Slot::Overlay,
            ) => space,
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
            media: self.media.generation,
            digest: self.digests.get(&decl.source).copied(),
            space,
            ui: held.cloned().unwrap_or_default(),
        };
        let mut memo = self.memo.borrow_mut();
        if memo.get(surface).is_none_or(|last| last.key != key) {
            let before = memo.get(surface).map(|last| last.built.playing.clone());
            let built = self.build(decl, view, held, space, before.as_ref());
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
        before: Option<&BTreeMap<String, media::Playing>>,
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
            preferences: &self.preferences,
        };
        let lookup = media::Lookup {
            shelf: &self.media,
            sprites: &self.compiled.manifest.sprites,
            icons: &self.compiled.manifest.icons,
            before,
            now: self.now,
        };
        // A composed source answered the tree itself: its view file is
        // what draws while it has not, and the tree replaces it once it has.
        let composed = self
            .compiled
            .manifest
            .source(&decl.source)
            .is_some_and(|source| source.output == uze_contract::Output::View)
            || self.is_resident(&decl.source);
        if composed {
            return match compose::compose(data, &self.compiled.manifest, &view.file) {
                Ok(tree) => bind::build(&tree, &serde_json::Value::Null, env, lookup),
                Err(why) => Built::notice(view, &why, Role::Danger),
            };
        }
        bind::build(view, data, env, lookup)
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
            (Act::Cells { regions }, NodeAct::Region(index)) => match regions.get(index).cloned() {
                Some((action, item)) => self.act_on(surface, &action, item),
                None => Outcome::Stay,
            },
            (Act::List { key, rows, .. }, NodeAct::Row(row)) if row < rows => {
                self.set_ui(surface, key, row.to_string());
                Outcome::Stay
            }
            (act @ Act::Select { .. }, NodeAct::Step { forward }) => {
                self.step(surface, act, forward);
                Outcome::Stay
            }
            (act @ Act::Field { .. }, NodeAct::Edit) => {
                self.press_field(surface, act);
                Outcome::Stay
            }
            (act @ Act::Form { .. }, NodeAct::Press) => self.submit(surface, act),
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
                opens: surface.bar.as_ref().and_then(|bar| bar.opens.clone()),
                id: surface.id.clone(),
                slot: match surface.slot {
                    uze_contract::Slot::View => SurfaceSlot::View,
                    uze_contract::Slot::Section => SurfaceSlot::Section,
                    uze_contract::Slot::Drawer => SurfaceSlot::Drawer,
                    uze_contract::Slot::Status => SurfaceSlot::Status,
                    uze_contract::Slot::Band => SurfaceSlot::Band {
                        rows: surface.band.map_or(1, |band| band.rows),
                        bottom: surface
                            .band
                            .is_some_and(|band| band.place == uze_contract::BandPlace::Bottom),
                    },
                    uze_contract::Slot::Overlay => SurfaceSlot::Overlay,
                    uze_contract::Slot::Bar => SurfaceSlot::Bar {
                        right: surface
                            .bar
                            .as_ref()
                            .is_some_and(|bar| bar.align == uze_contract::BarAlign::Right),
                    },
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
        // A view already open stays drawn; every other slot is drawn only
        // where it applies, as its door is offered.
        if decl.slot != uze_contract::Slot::View && !self.surface_applies(surface) {
            return None;
        }
        let title = self.clean_title(decl);
        let scroll = self.scroll.get(surface).copied().unwrap_or(0);
        let root = self.with_built(surface, space, |built| built.root.clone())?;
        self.frames.set(self.frames.get() + 1);
        Some(View {
            title: vec![Span::clean(title, Role::Muted)],
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
        if decl.slot != uze_contract::Slot::Section || !self.surface_applies(surface) {
            return None;
        }
        let root = self.with_built(surface, Size::default(), |built| built.root.clone())?;
        self.frames.set(self.frames.get() + 1);
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
        if let Some(outcome) = self.command_while_editing(surface, &command, space) {
            return outcome;
        }
        match command {
            Command::Close => return Outcome::Close,
            Command::FocusNext => self.focus_next(surface, space),
            Command::Activate => return self.activate(surface, space),
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
        if matches!(hit, ViewHit::Node { .. }) && self.just_laid_out(surface) {
            return Outcome::Stay;
        }
        let space = self.space_for(surface, space);
        // A press anywhere but the field typed into ends the typing, as a
        // click away from any input does.
        if !matches!(
            hit,
            ViewHit::Node {
                act: NodeAct::Edit,
                ..
            }
        ) {
            self.stop_editing(surface);
        }
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
    fn agent_events(&mut self, happened: &[crate::contract::AgentHappened], now: Instant) {
        self.take_agent_events(happened, now);
    }

    fn agents(&mut self, agents: &serde_json::Value) {
        if self.authoring.faking_agents {
            self.authoring.real_agents = agents.clone();
            return;
        }
        self.give_agents(agents);
    }

    fn notifications(&mut self) -> Vec<crate::contract::Notify> {
        self.take_outgoing()
    }

    fn requests(&mut self) -> Vec<Outcome> {
        std::mem::take(&mut self.unattended)
            .into_iter()
            .map(|request| Outcome::Agent(Box::new(request)))
            .collect()
    }

    fn configure(&mut self, preferences: &serde_json::Value) {
        let admitted: serde_json::Map<String, serde_json::Value> = self
            .compiled
            .manifest
            .preferences
            .iter()
            .map(|decl| (decl.name.clone(), decl.admit(preferences.get(&decl.name))))
            .collect();
        let admitted = serde_json::Value::Object(admitted);
        if admitted != self.preferences {
            self.preferences = admitted;
            // What a build read is no longer what it would read.
            self.generation += 1;
            // The host adds the preferences to a source's input as it asks,
            // so the question remembered here never changes with them: a
            // source that reads them is asked again by forgetting it.
            for source in &self.compiled.manifest.sources {
                if source.with_preferences {
                    self.asked.remove(&source.id);
                }
            }
        }
    }

    fn open(&mut self, surface: &str, checkout: &Checkout) {
        let Some(decl) = self.compiled.manifest.surface(surface) else {
            return;
        };
        self.asked.remove(&decl.source);
        self.chosen.remove(surface);
        self.opened
            .insert(surface.to_owned(), checkout.root.clone());
    }

    /// A question standing on it goes unanswered, so nothing runs.
    fn close(&mut self, surface: &str) {
        self.opened.remove(surface);
        self.chosen.remove(surface);
        // What it drew no longer asks for its pictures.
        self.memo.get_mut().remove(surface);
        self.pending.take_if(|pending| pending.surface == surface);
        self.notices.remove(surface);
    }

    fn focus(&mut self, checkout: Option<&Checkout>) {
        self.in_front = checkout.map(|checkout| checkout.root.clone());
    }

    fn reads_raw_input(&self, surface: &str) -> bool {
        self.reads_raw(surface)
    }

    /// While a field is typed into, a letter is text.
    fn typing(&self, surface: &str) -> bool {
        self.editing(surface, self.last_space(surface)).is_some()
    }

    fn paste(&mut self, surface: &str, text: &str, space: Size) {
        let space = self.space_for(surface, space);
        self.paste_into(surface, text, space);
    }

    fn missed(&mut self, surface: &str, _space: Size) {
        self.stop_editing(surface);
    }

    fn applies(&self, surface: &str) -> bool {
        self.surface_applies(surface)
    }

    fn raw_input(&mut self, surface: &str, input: crate::contract::RawInput) {
        self.take_raw_input(surface, input);
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
        self.in_view = Some(pass.visible.to_vec());
        for happened in pass.happened {
            self.gather(happened, pass.now);
        }
        let gathered = self.window.take(pass.now);
        self.due.extend(gathered);
        // A source delivered `each` is asked again for its next event once
        // its last question has been answered.
        let waiting: Vec<String> = self
            .delivery
            .queued
            .keys()
            .filter(|source| {
                self.delivery.waiting(source)
                    && !self.awaiting(source)
                    && !self.window.holds(source)
            })
            .cloned()
            .collect();
        self.due.extend(waiting);
        let in_view = self.sources_in_view(pass.visible);
        let every: Vec<String> = in_view
            .iter()
            .filter(|source| self.every_passed(source, pass.now))
            .cloned()
            .collect();
        self.due.extend(every);
        self.pass_residents(pass.visible, pass.now);
        // A source whose question is still out stays due: asking again
        // now would drop the answer on its way, and a source slower than
        // its `every` would never show one.
        let asked: Vec<String> = self
            .due
            .iter()
            .filter(|source| in_view.contains(*source) && !self.question_out(source))
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
        jobs.extend(self.resident_jobs());
        jobs.extend(self.marker_jobs());
        for (source, checkout, surface) in self.shown_sources() {
            if !self.source_applies(&source, &checkout) {
                continue;
            }
            let local = self
                .compiled
                .manifest
                .source(&source)
                .is_some_and(|declared| {
                    matches!(
                        declared.kind,
                        uze_contract::SourceKind::Events(_)
                            | uze_contract::SourceKind::Agents
                            | uze_contract::SourceKind::Resident(_)
                    )
                });
            if local || self.authoring.fixtured.contains(&source) {
                continue;
            }
            self.capture_selection(&surface);
            let input = self.input_now(&source, &surface);
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
        let wanted = self.pictures_in_view();
        jobs.extend(self.media.next(
            &wanted,
            &self.compiled.identity,
            &self.compiled.manifest.sprites,
        ));
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
        let answer = match answer.downcast::<media::MediaAnswer>() {
            Ok(read) => {
                self.media.absorb(*read);
                return Outcome::Stay;
            }
            Err(other) => other,
        };
        let answer = match answer.downcast::<resident::ResidentAnswer>() {
            Ok(answered) => return self.absorb_resident(*answered),
            Err(other) => other,
        };
        let answer = match answer.downcast::<applies::MarkerAnswer>() {
            Ok(found) => {
                self.absorb_marker(*found);
                return Outcome::Stay;
            }
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
        let about = asked.checkout.clone();
        let source = answered.source;
        self.answered.insert(source.clone(), answered.number);
        match answered.answer {
            Answer::Answered { envelope, .. } => {
                self.set_data(source.clone(), Ok(envelope.data.clone()));
                self.answered_for.insert(source.clone(), about);
                // What its program wrote may be a new picture.
                self.media.stale_scratch();
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
                self.set_data(source.clone(), Err(reason));
                self.answered_for.insert(source, about);
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
            let over = memory.recorded.len().saturating_sub(RECORDED_MAX);
            self.recorded = memory.recorded;
            self.recorded.drain(..over);
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

#[cfg(test)]
mod media_tests;

#[cfg(test)]
mod resident_tests;

#[cfg(test)]
mod input_tests;

#[cfg(test)]
mod applies_tests;

#[cfg(test)]
mod authoring_tests;
