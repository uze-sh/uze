//! The surfaces plugins declare, hosted through one generic path.
//!
//! A built-in extension is wired into this client field by field; a
//! declared one is not wired at all. Every declared surface is a *slot*:
//! one entry of [`DeclaredExtensions::slots`], keyed by the extension's
//! identity and the surface's id, holding the engine that answers for it.
//! A `view` slot opens full-frame from the tab strip, a `section` slot is a
//! block of the sidebar, and every gesture on either reaches the engine
//! through the [`Extension`] contract and nothing else.
//!
//! What a slot draws is the engine's, from the last compile and the last
//! data that were good: a compile or a fixture that fails is never handed
//! to it, so it keeps drawing what it drew, and the error is said over it.

use super::*;
use uze_application::{
    CompiledExtension, ExtensionDiagnostic, ExtensionIdentity, InstalledExtension, SourceData,
};
use uze_contract::Clean;
use uze_extensions::contract::{Extension, Opaque, Outcome, SurfaceInfo, SurfaceSlot};
use uze_extensions::declared::{
    Absence, DataState, DeclaredExtension, NODE_CAP, RecordedAction, TraceEntry, TraceNode,
};
use uze_extensions::view::{Command, Role, ScrollTarget, Section, SectionRow, Size, View};

/// The host's number for a slot, handed out once and never reused, so a
/// hit drawn on one frame cannot land on another slot after a reload.
pub(super) type SlotId = u32;

/// How often a linked extension's files are looked at.
pub(super) const DECLARED_POLL: Duration = Duration::from_millis(250);
/// How long a save must stand before it is compiled: an editor writing a
/// file in two steps, or an author saving twice, is one recompile.
pub(super) const DECLARED_SETTLE: Duration = Duration::from_millis(150);

/// The locale lookups are drawn in. The operator's own locale reaches the
/// engine here once the workspace knows it; until then every table falls
/// back to its own default.
const LOCALE: &str = "en";

/// The longest the inspector prints a source's data, in lines.
const INSPECTED_DATA_LINES: usize = 200;

/// What the host asks of a declared extension beyond the contract: taking a
/// newer compile of itself, its sources' data, and saying for the inspector
/// how it came to draw what it drew.
pub(super) trait Hosted: Extension {
    /// A newer compile of the same extension. Where the viewer is, the UI
    /// state it holds, is kept.
    fn replace(&mut self, compiled: CompiledExtension);
    fn set_data(&mut self, source: &str, data: SourceData);
    fn inspect(&self, surface: &str) -> Inspection;
}

impl Hosted for DeclaredExtension {
    fn replace(&mut self, compiled: CompiledExtension) {
        DeclaredExtension::replace(self, compiled);
    }

    fn set_data(&mut self, source: &str, data: SourceData) {
        DeclaredExtension::set_data(self, source, data);
    }

    fn inspect(&self, surface: &str) -> Inspection {
        let compiled = self.compiled();
        let declared = compiled.manifest.surface(surface);
        let trace = self.trace(surface);
        let mut nodes = Vec::new();
        if let Some(trace) = &trace {
            match &trace.data {
                DataState::Bound => {}
                DataState::Waiting => nodes.push("waiting for data".to_owned()),
                DataState::Failed(reason) => nodes.push(format!("no data: {reason}")),
            }
            outline(&trace.items, 0, &mut nodes);
            for (name, body) in &trace.templates {
                nodes.push(format!("template {}", Clean::label(name).as_str()));
                outline(body, 1, &mut nodes);
            }
            if trace.folded {
                nodes.push(format!("folded at {NODE_CAP} nodes"));
            }
        }
        // What a fixture holds is a third party's text: drawn only once
        // it is clean, like everything else the plugin supplies.
        let data = declared
            .and_then(|declared| self.data(&declared.source))
            .map(|data| match data {
                Ok(value) => serde_json::to_string_pretty(value)
                    .unwrap_or_default()
                    .lines()
                    .map(|line| Clean::label(line).into_string())
                    .collect(),
                Err(reason) => vec![Clean::label(reason).into_string()],
            })
            .unwrap_or_default();
        Inspection {
            nodes,
            traced: trace.is_some(),
            data,
            ui: self
                .ui(surface)
                .map(|ui| {
                    ui.iter()
                        .map(|(id, value)| {
                            (
                                format!("ui.{}", Clean::label(id).as_str()),
                                Clean::label(value).into_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            recorded: trace
                .map(|trace| trace.recorded.iter().map(recorded_line).collect())
                .unwrap_or_default(),
        }
    }
}

fn recorded_line(action: &RecordedAction) -> String {
    Clean::label(&format!("{} {}", action.action, action.item)).into_string()
}

/// The traced view as an indented outline, one node a line: how often it
/// was drawn of how often it was reached, and why it was not.
fn outline(nodes: &[TraceNode], depth: usize, lines: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    for node in nodes {
        let name = match &node.entry {
            TraceEntry::Node(name) => name.to_string(),
            TraceEntry::Use(template) => format!("use {}", Clean::label(template).as_str()),
        };
        let standing = match (node.reached, node.drawn) {
            (0, _) => " · not reached".to_owned(),
            (reached, drawn) if drawn == reached => String::new(),
            (reached, drawn) => format!(" · drawn {drawn} of {reached}"),
        };
        lines.push(format!("{indent}{name}{standing}"));
        for absence in &node.absent {
            lines.push(format!("{indent}  absent: {}", absence_text(absence)));
        }
        outline(&node.children, depth + 1, lines);
    }
}

fn absence_text(absence: &Absence) -> String {
    let said = match absence {
        Absence::When {
            field,
            missing: true,
        } => format!("`{field}` is missing from the data"),
        Absence::When { field, .. } => format!("`{field}` is empty"),
        Absence::Unless { field } => format!("`{field}` is present"),
        Absence::Show { id, value, actual } => match actual {
            Some(actual) => format!("ui.{id} is {actual}, not {value}"),
            None => format!("ui.{id} is not {value}"),
        },
        Absence::ShowAny => "none of its show-any pairs hold".to_owned(),
        Absence::EachMissing { field } => format!("`{field}` is missing from the data"),
        Absence::EachEmpty { field } => format!("`{field}` is an empty list"),
        Absence::EachNotList { field } => format!("`{field}` is not a list"),
        Absence::Missing { field } => format!("`{field}` is missing from the data"),
        Absence::CapReached => format!("the view reached {NODE_CAP} nodes"),
        Absence::UnknownTemplate => "no template of that name".to_owned(),
        Absence::Misplaced => "it only means something inside another node".to_owned(),
    };
    Clean::label(&said).into_string()
}

/// What the inspector shows of one surface.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Inspection {
    /// The compiled view, one node a line.
    pub(super) nodes: Vec<String>,
    /// Whether `nodes` says which were drawn and why the rest were not.
    pub(super) traced: bool,
    /// The source's data, pretty-printed.
    pub(super) data: Vec<String>,
    pub(super) ui: Vec<(String, String)>,
    /// What a pressed button would have run, and with what.
    pub(super) recorded: Vec<String>,
}

/// Builds the engine a slot is answered by. A function rather than a
/// constant so a test can host an engine of its own.
pub(super) type EngineFactory = fn(CompiledExtension) -> Box<dyn Hosted>;

fn declared_engine(compiled: CompiledExtension) -> Box<dyn Hosted> {
    Box::new(DeclaredExtension::new(compiled, LOCALE))
}

/// One surface a plugin declared, as the host holds it.
pub(super) struct DeclaredSlot {
    pub(super) id: SlotId,
    pub(super) identity: ExtensionIdentity,
    pub(super) surface: SurfaceInfo,
    /// `None` for the stand-in of an extension that has never compiled: it
    /// has no surfaces to speak of yet, and its error needs somewhere to be
    /// said.
    pub(super) engine: Option<Box<dyn Hosted>>,
    /// A section folded to its header.
    pub(super) collapsed: bool,
}

impl DeclaredSlot {
    pub(super) fn is_view(&self) -> bool {
        self.surface.slot == SurfaceSlot::View
    }
}

/// One extension an installed plugin carries, as last read.
pub(super) struct HostedExtension {
    pub(super) installed: InstalledExtension,
    /// The stamp of its files when it was last compiled.
    pub(super) stamp: u64,
    /// A save the poll saw and when, waiting for [`DECLARED_SETTLE`].
    pub(super) changed: Option<(u64, Instant)>,
    /// What was wrong with the last compile or fixture read, one problem a
    /// line, said where it is. Empty when it all went through.
    pub(super) errors: Vec<String>,
}

/// Every declared extension the client hosts, and the slots their surfaces
/// stand in. Kept across attaches with the rest of what the client
/// resolved: a compile is a resolution, and where a viewer was on one is
/// remembered per checkout.
pub(super) struct DeclaredExtensions {
    pub(super) extensions: BTreeMap<ExtensionIdentity, HostedExtension>,
    /// In identity order, each extension's surfaces in the order it
    /// declares them.
    pub(super) slots: Vec<DeclaredSlot>,
    next_slot: SlotId,
    /// Whether the installed extensions were asked for, this run.
    pub(super) discovered: bool,
    pub(super) discovery_pending: bool,
    /// Extensions a recompile is out for.
    pub(super) recompiling: BTreeSet<ExtensionIdentity>,
    pub(super) stamps_pending: bool,
    pub(super) polled_at: Option<Instant>,
    /// Where the viewer was on each surface, per checkout.
    pub(super) places: BTreeMap<(ExtensionIdentity, String, PathBuf), Opaque>,
    pub(super) engine: EngineFactory,
}

impl Default for DeclaredExtensions {
    fn default() -> Self {
        Self {
            extensions: BTreeMap::new(),
            slots: Vec::new(),
            next_slot: 0,
            discovered: false,
            discovery_pending: false,
            recompiling: BTreeSet::new(),
            stamps_pending: false,
            polled_at: None,
            places: BTreeMap::new(),
            engine: declared_engine,
        }
    }
}

impl DeclaredExtensions {
    pub(super) fn slot(&self, id: SlotId) -> Option<&DeclaredSlot> {
        self.slots.iter().find(|slot| slot.id == id)
    }

    pub(super) fn slot_mut(&mut self, id: SlotId) -> Option<&mut DeclaredSlot> {
        self.slots.iter_mut().find(|slot| slot.id == id)
    }

    pub(super) fn errors_of(&self, identity: &ExtensionIdentity) -> &[String] {
        self.extensions
            .get(identity)
            .map_or(&[], |extension| extension.errors.as_slice())
    }

    /// The linked extensions' directories, which the poll looks at.
    pub(super) fn watched(&self) -> Vec<(ExtensionIdentity, PathBuf)> {
        self.extensions
            .values()
            .filter(|extension| extension.installed.linked)
            .map(|extension| {
                (
                    extension.installed.identity.clone(),
                    extension.installed.dir.clone(),
                )
            })
            .collect()
    }

    /// Takes what a discovery found: extensions no plugin carries any more
    /// go, with their slots.
    pub(super) fn absorb_discovery(&mut self, installed: &[InstalledExtension]) {
        let present: BTreeSet<&ExtensionIdentity> =
            installed.iter().map(|found| &found.identity).collect();
        self.extensions
            .retain(|identity, _| present.contains(identity));
        self.slots.retain(|slot| present.contains(&slot.identity));
    }

    /// Takes one extension's compile and data. A compile that succeeded
    /// replaces what each of its surfaces' engines holds, keeping where the
    /// viewer is; one that failed changes nothing but the error said over
    /// them.
    pub(super) fn absorb_loaded(&mut self, loaded: LoadedExtension) {
        let identity = loaded.installed.identity.clone();
        let entry = self
            .extensions
            .entry(identity.clone())
            .or_insert_with(|| HostedExtension {
                installed: loaded.installed.clone(),
                stamp: loaded.stamp,
                changed: None,
                errors: Vec::new(),
            });
        entry.installed = loaded.installed;
        entry.stamp = loaded.stamp;
        if entry
            .changed
            .is_some_and(|(stamp, _)| stamp == loaded.stamp)
        {
            entry.changed = None;
        }
        match loaded.compiled {
            Ok(compiled) => {
                entry.errors = loaded
                    .data
                    .values()
                    .filter_map(|data| data.as_ref().err().cloned())
                    .collect();
                self.host_compiled(&identity, compiled, &loaded.data);
            }
            Err(diagnostics) => {
                entry.errors = diagnostics.iter().map(ToString::to_string).collect();
                if !self.slots.iter().any(|slot| slot.identity == identity) {
                    let id = self.take_slot_id();
                    self.insert_slot(DeclaredSlot {
                        id,
                        surface: SurfaceInfo {
                            id: String::new(),
                            slot: SurfaceSlot::View,
                            title: identity.id.clone(),
                        },
                        identity,
                        engine: None,
                        collapsed: false,
                    });
                }
            }
        }
    }

    fn host_compiled(
        &mut self,
        identity: &ExtensionIdentity,
        compiled: CompiledExtension,
        data: &BTreeMap<String, SourceData>,
    ) {
        let surfaces = surfaces_of(&compiled);
        // The stand-in of a compile that never went through, and surfaces
        // the extension no longer declares.
        self.slots.retain(|slot| {
            &slot.identity != identity
                || (slot.engine.is_some()
                    && surfaces.iter().any(|surface| {
                        surface.id == slot.surface.id && surface.slot == slot.surface.slot
                    }))
        });
        for surface in surfaces {
            let existing = self
                .slots
                .iter_mut()
                .find(|slot| &slot.identity == identity && slot.surface.id == surface.id);
            match existing {
                Some(slot) => {
                    if let Some(engine) = slot.engine.as_mut() {
                        engine.replace(compiled.clone());
                        give_data(engine.as_mut(), data);
                    }
                    slot.surface = surface;
                }
                None => {
                    let mut engine = (self.engine)(compiled.clone());
                    give_data(engine.as_mut(), data);
                    let id = self.take_slot_id();
                    self.insert_slot(DeclaredSlot {
                        id,
                        identity: identity.clone(),
                        surface,
                        engine: Some(engine),
                        collapsed: false,
                    });
                }
            }
        }
    }

    fn take_slot_id(&mut self) -> SlotId {
        let id = self.next_slot;
        self.next_slot = self.next_slot.wrapping_add(1);
        id
    }

    /// After the last slot of its own extension, or in identity order.
    fn insert_slot(&mut self, slot: DeclaredSlot) {
        let at = self
            .slots
            .iter()
            .rposition(|other| other.identity <= slot.identity)
            .map_or(0, |index| index + 1);
        self.slots.insert(at, slot);
    }

    /// Takes a poll of the linked extensions' stamps: one that moved starts
    /// (or restarts) its wait to settle.
    pub(super) fn absorb_stamps(&mut self, stamps: Vec<(ExtensionIdentity, u64)>, now: Instant) {
        for (identity, stamp) in stamps {
            let Some(extension) = self.extensions.get_mut(&identity) else {
                continue;
            };
            let seen = extension.changed.map(|(seen, _)| seen);
            if stamp == extension.stamp {
                extension.changed = None;
            } else if seen != Some(stamp) {
                extension.changed = Some((stamp, now));
            }
        }
    }

    /// The extensions whose save has settled and is not being compiled
    /// already.
    pub(super) fn settled(&self, now: Instant) -> Vec<InstalledExtension> {
        self.extensions
            .values()
            .filter(|extension| {
                extension
                    .changed
                    .is_some_and(|(_, at)| now.duration_since(at) >= DECLARED_SETTLE)
                    && !self.recompiling.contains(&extension.installed.identity)
            })
            .map(|extension| extension.installed.clone())
            .collect()
    }
}

/// Hands an engine every source that answered. One that failed is held
/// back, so the engine keeps the data it drew last.
fn give_data(engine: &mut dyn Hosted, data: &BTreeMap<String, SourceData>) {
    for (source, answer) in data {
        if answer.is_ok() {
            engine.set_data(source, answer.clone());
        }
    }
}

/// The surfaces a compile declares, as the contract names them.
fn surfaces_of(compiled: &CompiledExtension) -> Vec<SurfaceInfo> {
    compiled
        .manifest
        .surfaces
        .iter()
        .map(|surface| SurfaceInfo {
            id: surface.id.clone(),
            slot: match surface.slot {
                uze_contract::Slot::View => SurfaceSlot::View,
                uze_contract::Slot::Section => SurfaceSlot::Section,
            },
            title: surface
                .title
                .clone()
                .unwrap_or_else(|| compiled.manifest.name.clone()),
        })
        .collect()
}

/// What a load answered for one extension.
pub(super) struct LoadedExtension {
    pub(super) installed: InstalledExtension,
    /// The stamp of its files, taken before they were read.
    pub(super) stamp: u64,
    pub(super) compiled: std::result::Result<CompiledExtension, Vec<ExtensionDiagnostic>>,
    /// By source id; empty when the compile failed.
    pub(super) data: BTreeMap<String, SourceData>,
}

/// The key a declared extension is switched on and off by: the attribution
/// it is drawn with, which no built-in's id can collide with.
pub(super) fn switch_id(identity: &ExtensionIdentity) -> String {
    identity.to_string()
}

impl WorkspaceModel {
    pub(super) fn offers_declared(&self, identity: &ExtensionIdentity) -> bool {
        self.offers_extension(&switch_id(identity))
    }

    /// The declared views the tab strip offers, in slot order.
    pub(super) fn declared_views(&self) -> impl Iterator<Item = &DeclaredSlot> {
        self.remembered
            .declared
            .slots
            .iter()
            .filter(|slot| slot.is_view() && self.offers_declared(&slot.identity))
    }

    /// The declared view standing in the pane.
    pub(super) fn open_declared_slot(&self) -> Option<&DeclaredSlot> {
        self.remembered.declared.slot(self.declared_open?)
    }

    /// Closes the declared view, keeping where the viewer was on this
    /// checkout — every way out goes through here, as with the built-ins.
    pub(super) fn close_declared(&mut self) {
        let Some(id) = self.declared_open.take() else {
            return;
        };
        self.inspecting = false;
        self.declared_notice = None;
        let root = self.declared_root.take();
        let declared = &mut self.remembered.declared;
        let Some(slot) = declared.slot(id) else {
            return;
        };
        let place = slot
            .engine
            .as_ref()
            .and_then(|engine| engine.place(&slot.surface.id));
        if let (Some(place), Some(root)) = (place, root) {
            let key = (slot.identity.clone(), slot.surface.id.clone(), root);
            declared.places.insert(key, place);
        }
    }

    /// What the engine of the open view answered, followed.
    pub(super) fn follow_declared(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Stay => {}
            Outcome::Close => self.close_declared(),
            Outcome::Notice(span) => self.declared_notice = Some(span),
        }
        self.dirty = true;
    }

    /// Hands the open view's engine a command.
    pub(super) fn tell_declared(&mut self, command: Command, space: Size) {
        let Some(id) = self.declared_open else {
            return;
        };
        let outcome = self
            .remembered
            .declared
            .slot_mut(id)
            .and_then(|slot| {
                let surface = slot.surface.id.clone();
                Some(slot.engine.as_mut()?.command(&surface, command, space))
            })
            .unwrap_or(Outcome::Stay);
        self.follow_declared(outcome);
    }

    /// Hands a slot's engine a gesture on what it drew.
    pub(super) fn hit_declared(&mut self, id: SlotId, hit: ViewHit, space: Size) {
        if hit == ViewHit::ToggleSection {
            if let Some(slot) = self.remembered.declared.slot_mut(id) {
                slot.collapsed = !slot.collapsed;
            }
            self.dirty = true;
            return;
        }
        let outcome = self
            .remembered
            .declared
            .slot_mut(id)
            .and_then(|slot| {
                let surface = slot.surface.id.clone();
                Some(slot.engine.as_mut()?.hit(&surface, hit, space))
            })
            .unwrap_or(Outcome::Stay);
        // A section is part of the sidebar: it has no "close" of its own.
        if self.declared_open == Some(id) || outcome != Outcome::Close {
            self.follow_declared(outcome);
        }
        self.dirty = true;
    }

    pub(super) fn scroll_declared(&mut self, target: ScrollTarget, direction: ScrollDirection) {
        let Some(id) = self.declared_open else {
            return;
        };
        if let Some(slot) = self.remembered.declared.slot_mut(id) {
            let surface = slot.surface.id.clone();
            if let Some(engine) = slot.engine.as_mut() {
                engine.scroll(&surface, target, direction);
            }
        }
        self.dirty = true;
    }

    /// What the open view draws this frame: the engine's answer, or, when
    /// it has none to give yet, a message saying why. Its notice is the
    /// host's last word from the engine, when it said one.
    pub(super) fn declared_view(&self, space: Size) -> Option<View> {
        let slot = self.open_declared_slot()?;
        let errors = self.remembered.declared.errors_of(&slot.identity);
        let drawn = slot
            .engine
            .as_ref()
            .and_then(|engine| engine.view(&slot.surface.id, space));
        let mut view = drawn.unwrap_or_else(|| {
            let (text, role) = if errors.is_empty() {
                (format!("Reading {}…", slot.surface.title), Role::Muted)
            } else {
                (
                    format!("{} can't be drawn", slot.surface.title),
                    Role::Danger,
                )
            };
            View {
                title: vec![uze_extensions::view::Span::new(
                    slot.surface.title.clone(),
                    Role::Muted,
                )],
                caption: Vec::new(),
                navigator: None,
                content: uze_extensions::view::Content::Message {
                    text,
                    hint: None,
                    role,
                },
                footer: vec![Command::Close],
                notice: None,
                confirm: None,
                modes: Vec::new(),
                subjects: Vec::new(),
                layout: uze_extensions::view::Layout::Sidebar,
                trail: Vec::new(),
            }
        });
        if let Some(notice) = &self.declared_notice {
            view.notice = Some(notice.clone());
        }
        Some(view)
    }

    /// The declared sections the sidebar draws, in slot order: the engine's
    /// own, with the error said in its caption and, open, in its rows.
    /// A section with nothing to draw and nothing wrong takes no room.
    pub(super) fn declared_sections(&self) -> Vec<(SlotId, Section)> {
        let declared = &self.remembered.declared;
        declared
            .slots
            .iter()
            .filter(|slot| !slot.is_view() && self.offers_declared(&slot.identity))
            .filter_map(|slot| {
                let errors = declared.errors_of(&slot.identity);
                let drawn = slot
                    .engine
                    .as_ref()
                    .and_then(|engine| engine.section(&slot.surface.id, slot.collapsed));
                let section = match (drawn, errors.is_empty()) {
                    (Some(section), true) => section,
                    (None, true) => return None,
                    (drawn, false) => with_errors(drawn, slot, errors),
                };
                Some((slot.id, section))
            })
            .collect()
    }

    /// Opens or closes the inspector beside the open view.
    pub(super) fn toggle_inspector(&mut self) {
        if self.declared_open.is_some() {
            self.inspecting = !self.inspecting;
            self.dirty = true;
        }
    }

    /// What the inspector shows of the open view.
    pub(super) fn inspection(&self) -> Option<Inspection> {
        let slot = self.open_declared_slot()?;
        let mut inspection = slot
            .engine
            .as_ref()
            .map(|engine| engine.inspect(&slot.surface.id))
            .unwrap_or_default();
        inspection.data.truncate(INSPECTED_DATA_LINES);
        Some(inspection)
    }
}

/// A section whose extension failed: what it drew last, or its header
/// alone, with the error as its caption and, while open, a row a problem.
fn with_errors(drawn: Option<Section>, slot: &DeclaredSlot, errors: &[String]) -> Section {
    let mut section = drawn.unwrap_or_else(|| Section {
        title: slot.surface.title.clone(),
        collapsed: slot.collapsed,
        ..Section::default()
    });
    let count = if errors.len() == 1 {
        "1 error".to_owned()
    } else {
        format!("{} errors", errors.len())
    };
    section.caption = uze_extensions::view::Span::new(count, Role::Danger);
    if !section.collapsed && section.tree.is_none() && section.rows.is_empty() {
        section.rows = errors
            .iter()
            .map(|error| SectionRow {
                mark: uze_extensions::view::RowMark::Step { done: false },
                mark_role: Role::Danger,
                name: uze_extensions::view::Span::new(error.clone(), Role::Danger),
                trailing: uze_extensions::view::Span::default(),
            })
            .collect();
    }
    section
}

pub(super) fn open_declared(model: &mut WorkspaceModel, id: SlotId) {
    let Some(slot) = model.remembered.declared.slot(id) else {
        return;
    };
    if !slot.is_view() || !model.offers_declared(&slot.identity) {
        return;
    }
    let Some(root) = model.focused_cwd() else {
        return;
    };
    model.close_extension();
    model.stand_extension_in_front();
    let declared = &mut model.remembered.declared;
    if let Some(slot) = declared.slots.iter_mut().find(|slot| slot.id == id) {
        let key = (slot.identity.clone(), slot.surface.id.clone(), root.clone());
        if let (Some(place), Some(engine)) = (declared.places.remove(&key), slot.engine.as_mut()) {
            engine.resume(&slot.surface.id, place);
        }
    }
    model.declared_open = Some(id);
    model.declared_root = Some(root);
    model.code_tree_scroll = extension_view::NavigatorScroll::default();
    model.dirty = true;
}
