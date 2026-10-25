//! Extension surfaces, hosted through one generic path.
//!
//! Every surface is a *slot*: one entry of [`DeclaredExtensions::slots`],
//! keyed by the extension that owns it (a built-in, or one a plugin
//! declares) and the surface's id, answered by that extension's engine.
//! The built-ins that have moved onto this path are hosted from the start;
//! a declared one once its plugin's files compile.
//! A `view` slot opens full-frame from the tab strip, a `section` slot is a
//! block of the sidebar, a `drawer` slot is docked beside the pane from its
//! button in the strip, and every gesture on any of them reaches the engine
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
use uze_extensions::contract::{Checkout, Extension, Opaque, Outcome, SurfaceInfo, SurfaceSlot};
use uze_extensions::declared::{
    Absence, DataState, DeclaredExtension, NODE_CAP, RecordedAction, TraceEntry, TraceNode,
};
use uze_extensions::view::{Command, Role, ScrollTarget, Section, SectionRow, Size, View};

mod drawer;
pub(super) use drawer::*;

/// The host's number for a slot, handed out once and never reused, so a
/// hit drawn on one frame cannot land on another slot after a reload.
pub(super) type SlotId = u32;

/// How often a linked extension's files are looked at.
pub(super) const DECLARED_POLL: Duration = Duration::from_millis(250);
/// How long a save must stand before it is compiled: an editor writing a
/// file in two steps, or an author saving twice, is one recompile.
pub(super) const DECLARED_SETTLE: Duration = Duration::from_millis(150);

/// How often the installed plugins are looked at again for extensions,
/// besides whenever an install, update or removal in this process lands:
/// one made from another terminal is seen within this.
pub(super) const DECLARED_REDISCOVER: Duration = Duration::from_secs(5);

/// The longest the inspector prints a source's data, in lines.
const INSPECTED_DATA_LINES: usize = 200;

/// Who a slot's surface belongs to.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Owner {
    /// An extension UZE ships, by its catalog id.
    Builtin(&'static str),
    Declared(ExtensionIdentity),
}

impl Owner {
    pub(super) fn is_builtin(&self, id: &str) -> bool {
        matches!(self, Self::Builtin(builtin) if *builtin == id)
    }

    pub(super) fn declared(&self) -> Option<&ExtensionIdentity> {
        match self {
            Self::Builtin(_) => None,
            Self::Declared(identity) => Some(identity),
        }
    }

    /// The key the operator switches it on and off by, the same one the
    /// Extensions screen switches.
    pub(super) fn switch_id(&self) -> String {
        match self {
            Self::Builtin(id) => (*id).to_owned(),
            Self::Declared(identity) => switch_id(identity),
        }
    }
}

/// What the host asks of a hosted extension beyond the contract: taking a
/// newer compile of itself, its sources' data, and saying for the inspector
/// how it came to draw what it drew. A built-in is never recompiled, reads
/// no fixtures and is not inspected, so each does nothing by default.
pub(super) trait Hosted: Extension {
    /// A newer compile of the same extension. Where the viewer is, the UI
    /// state it holds, is kept.
    fn replace(&mut self, _compiled: CompiledExtension) {}
    fn set_data(&mut self, _source: &str, _data: SourceData) {}
    fn inspect(&self, _surface: &str) -> Inspection {
        Inspection::default()
    }
    /// Lets go of where the viewer was on `surface`. Nothing by default:
    /// an engine that keeps no place has none to forget.
    fn forget(&mut self, _surface: &str) {}
}

impl Hosted for DeclaredExtension {
    fn replace(&mut self, compiled: CompiledExtension) {
        DeclaredExtension::replace(self, compiled);
    }

    fn set_data(&mut self, source: &str, data: SourceData) {
        DeclaredExtension::set_data(self, source, data);
    }

    fn forget(&mut self, surface: &str) {
        DeclaredExtension::forget(self, surface);
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

impl Hosted for uze_extensions::architect::ArchitectExtension {}

/// The built-ins hosted on the slot path, each with its engine.
fn builtins() -> Vec<(&'static str, Box<dyn Hosted>)> {
    vec![(
        architect::CATALOG.id,
        Box::new(architect::ArchitectExtension::default()),
    )]
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

/// Builds the engine a slot is answered by, for the operator's language.
/// A function rather than a constant so a test can host an engine of its
/// own.
pub(super) type EngineFactory = fn(CompiledExtension, &str) -> Box<dyn Hosted>;

fn declared_engine(compiled: CompiledExtension, locale: &str) -> Box<dyn Hosted> {
    Box::new(DeclaredExtension::new(compiled, locale))
}

/// What the host last compiled each extension from: where its files are
/// and their stamp then. A discovery loads again only what differs.
pub(super) type Known = BTreeMap<ExtensionIdentity, (PathBuf, u64)>;

/// One surface an extension offers, as the host holds it.
pub(super) struct DeclaredSlot {
    pub(super) id: SlotId,
    pub(super) owner: Owner,
    pub(super) surface: SurfaceInfo,
    /// A section folded to its header.
    pub(super) collapsed: bool,
}

impl DeclaredSlot {
    pub(super) fn is_view(&self) -> bool {
        self.surface.slot == SurfaceSlot::View
    }

    pub(super) fn is_drawer(&self) -> bool {
        self.surface.slot == SurfaceSlot::Drawer
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
    /// One engine per extension, shared by its slots, so its view, its
    /// section and its drawer keep one state. None for a declared extension
    /// that has never compiled: its slot is a stand-in for its error.
    pub(super) engines: BTreeMap<Owner, Box<dyn Hosted>>,
    /// The built-ins first, then in identity order, each extension's
    /// surfaces in the order it declares them.
    pub(super) slots: Vec<DeclaredSlot>,
    pub(super) next_slot: SlotId,
    /// When the installed extensions were last asked for, this run.
    pub(super) discovered_at: Option<Instant>,
    /// The plugin set's revision that discovery answered for: a newer one
    /// is an install, update or removal to look at now.
    pub(super) plugins_seen: u64,
    pub(super) discovery_pending: bool,
    /// Extensions a recompile is out for.
    pub(super) recompiling: BTreeSet<ExtensionIdentity>,
    pub(super) stamps_pending: bool,
    pub(super) polled_at: Option<Instant>,
    /// Where the viewer was on each surface, per checkout.
    pub(super) places: BTreeMap<(Owner, String, PathBuf), Opaque>,
    /// The drawer open in each space: one at most, so the pane gives up
    /// one column of room and never two.
    pub(super) drawers: BTreeMap<SpaceId, SlotId>,
    /// Where the viewer was on each drawer, per space: a drawer is about
    /// the work in front, and each space is its own.
    pub(super) drawer_places: BTreeMap<(Owner, String, SpaceId), Opaque>,
    /// The space whose places the drawers' engines hold now.
    pub(super) drawer_space: Option<SpaceId>,
    pub(super) engine: EngineFactory,
    /// The language every engine draws its label tables in.
    pub(super) locale: String,
}

impl Default for DeclaredExtensions {
    fn default() -> Self {
        let mut hosted = Self {
            extensions: BTreeMap::new(),
            engines: BTreeMap::new(),
            slots: Vec::new(),
            next_slot: 0,
            discovered_at: None,
            plugins_seen: 0,
            discovery_pending: false,
            recompiling: BTreeSet::new(),
            stamps_pending: false,
            polled_at: None,
            places: BTreeMap::new(),
            drawers: BTreeMap::new(),
            drawer_places: BTreeMap::new(),
            drawer_space: None,
            engine: declared_engine,
            locale: uze_platform::locale::language(),
        };
        for (id, engine) in builtins() {
            let owner = Owner::Builtin(id);
            for surface in engine.surfaces() {
                let slot = hosted.take_slot_id();
                hosted.insert_slot(DeclaredSlot {
                    id: slot,
                    owner: owner.clone(),
                    surface,
                    collapsed: false,
                });
            }
            hosted.engines.insert(owner, engine);
        }
        hosted
    }
}

impl DeclaredExtensions {
    pub(super) fn slot(&self, id: SlotId) -> Option<&DeclaredSlot> {
        self.slots.iter().find(|slot| slot.id == id)
    }

    pub(super) fn slot_mut(&mut self, id: SlotId) -> Option<&mut DeclaredSlot> {
        self.slots.iter_mut().find(|slot| slot.id == id)
    }

    /// The engine of an extension, when it has one.
    pub(super) fn engine(&self, owner: &Owner) -> Option<&dyn Hosted> {
        self.engines.get(owner).map(|engine| engine.as_ref())
    }

    /// The slot a built-in's surface stands in.
    pub(super) fn builtin_slot(&self, id: &str, surface: &str) -> Option<SlotId> {
        self.slots
            .iter()
            .find(|slot| slot.owner.is_builtin(id) && slot.surface.id == surface)
            .map(|slot| slot.id)
    }

    /// A slot's surface id and its extension's engine.
    pub(super) fn slot_engine_mut(&mut self, id: SlotId) -> Option<(String, &mut Box<dyn Hosted>)> {
        let slot = self.slots.iter().find(|slot| slot.id == id)?;
        let surface = slot.surface.id.clone();
        let engine = self.engines.get_mut(&slot.owner)?;
        Some((surface, engine))
    }

    pub(super) fn errors_of(&self, owner: &Owner) -> &[String] {
        owner
            .declared()
            .and_then(|identity| self.extensions.get(identity))
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

    /// What each hosted extension was last compiled from.
    pub(super) fn known(&self) -> Known {
        self.extensions
            .iter()
            .map(|(identity, extension)| {
                (
                    identity.clone(),
                    (extension.installed.dir.clone(), extension.stamp),
                )
            })
            .collect()
    }

    /// Whether a discovery is due: none was asked yet this run, the plugin
    /// set moved since the last, or [`DECLARED_REDISCOVER`] passed.
    pub(super) fn discovery_due(&self, plugins: u64, now: Instant) -> bool {
        !self.discovery_pending
            && self.discovered_at.is_none_or(|at| {
                plugins != self.plugins_seen || now.duration_since(at) >= DECLARED_REDISCOVER
            })
    }

    /// Takes what a discovery found: extensions no plugin carries any more
    /// go, with their slots and the places kept on them. Answers whether
    /// any went.
    pub(super) fn absorb_discovery(&mut self, installed: &[InstalledExtension]) -> bool {
        let present: BTreeSet<&ExtensionIdentity> =
            installed.iter().map(|found| &found.identity).collect();
        let before = self.extensions.len() + self.slots.len();
        let stays = |owner: &Owner| owner.declared().is_none_or(|found| present.contains(found));
        self.extensions
            .retain(|identity, _| present.contains(identity));
        self.slots.retain(|slot| stays(&slot.owner));
        self.engines.retain(|owner, _| stays(owner));
        self.places.retain(|(owner, _, _), _| stays(owner));
        self.drawer_places.retain(|(owner, _, _), _| stays(owner));
        let slots = &self.slots;
        self.drawers
            .retain(|_, id| slots.iter().any(|slot| slot.id == *id));
        before != self.extensions.len() + self.slots.len()
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
                let title = identity.id.clone();
                let owner = Owner::Declared(identity);
                if !self.slots.iter().any(|slot| slot.owner == owner) {
                    let id = self.take_slot_id();
                    self.insert_slot(DeclaredSlot {
                        id,
                        surface: SurfaceInfo {
                            id: String::new(),
                            slot: SurfaceSlot::View,
                            title,
                        },
                        owner,
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
        let owner = Owner::Declared(identity.clone());
        // The stand-in of a compile that never went through, and surfaces
        // the extension no longer declares.
        let hosted = self.engines.contains_key(&owner);
        self.slots.retain(|slot| {
            slot.owner != owner
                || (hosted
                    && surfaces.iter().any(|surface| {
                        surface.id == slot.surface.id && surface.slot == slot.surface.slot
                    }))
        });
        match self.engines.get_mut(&owner) {
            Some(engine) => {
                engine.replace(compiled.clone());
                give_data(engine.as_mut(), data);
            }
            None => {
                let mut engine = (self.engine)(compiled.clone(), &self.locale);
                give_data(engine.as_mut(), data);
                self.engines.insert(owner.clone(), engine);
            }
        }
        let slots = &self.slots;
        self.drawers
            .retain(|_, id| slots.iter().any(|slot| slot.id == *id));
        for surface in surfaces {
            let existing = self
                .slots
                .iter_mut()
                .find(|slot| slot.owner == owner && slot.surface.id == surface.id);
            match existing {
                Some(slot) => slot.surface = surface,
                None => {
                    let id = self.take_slot_id();
                    self.insert_slot(DeclaredSlot {
                        id,
                        owner: owner.clone(),
                        surface,
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

    /// After the last slot of its own extension, or in owner order.
    fn insert_slot(&mut self, slot: DeclaredSlot) {
        let at = self
            .slots
            .iter()
            .rposition(|other| other.owner <= slot.owner)
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
                uze_contract::Slot::Drawer => SurfaceSlot::Drawer,
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

/// The key a declared extension is switched on and off by, the same one
/// the Extensions screen switches.
pub(super) fn switch_id(identity: &ExtensionIdentity) -> String {
    crate::ui::extension_switch::declared_id(identity)
}

impl WorkspaceModel {
    pub(super) fn offers_declared(&self, owner: &Owner) -> bool {
        self.offers_extension(&owner.switch_id())
    }

    /// The views plugins declare that the tab strip offers, in slot order:
    /// a built-in's has a door of its own.
    pub(super) fn declared_views(&self) -> impl Iterator<Item = &DeclaredSlot> {
        self.remembered.declared.slots.iter().filter(|slot| {
            slot.is_view() && slot.owner.declared().is_some() && self.offers_declared(&slot.owner)
        })
    }

    /// The view standing in the pane.
    pub(super) fn open_declared_slot(&self) -> Option<&DeclaredSlot> {
        self.remembered.declared.slot(self.declared_open?)
    }

    /// Whether `hit` is one the open surface drew, a built-in's or a
    /// slot's.
    pub(super) fn is_open_surfaces(&self, hit: WorkspaceHit) -> bool {
        match hit {
            WorkspaceHit::Extension(ExtensionHit::Declared { slot, .. }) => {
                self.declared_open == Some(slot)
            }
            other => other.is_open_surfaces(),
        }
    }

    /// Whether the built-in `id`'s view stands in the pane.
    pub(super) fn showing(&self, id: &str) -> bool {
        self.open_declared_slot()
            .is_some_and(|slot| slot.owner.is_builtin(id))
    }

    /// The open view's engine and surface, for a gesture on it.
    pub(super) fn open_engine_mut(&mut self) -> Option<(String, &mut Box<dyn Hosted>)> {
        let id = self.declared_open?;
        self.remembered.declared.slot_engine_mut(id)
    }

    /// How the open view is laid out now.
    pub(super) fn open_layout(&self) -> Option<uze_extensions::view::Layout> {
        let slot = self.open_declared_slot()?;
        self.remembered
            .declared
            .engine(&slot.owner)
            .map(|engine| engine.layout(&slot.surface.id))
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
        let (owner, surface) = (slot.owner.clone(), slot.surface.id.clone());
        let Some(engine) = declared.engines.get_mut(&owner) else {
            return;
        };
        let place = engine.place(&surface);
        engine.close(&surface);
        if let (Some(place), Some(root)) = (place, root) {
            declared.places.insert((owner, surface, root), place);
        }
    }

    /// What the engine of the open view answered, followed.
    pub(super) fn follow_declared(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Stay => {}
            Outcome::Close => self.close_declared(),
            Outcome::Notice(span) => self.declared_notice = Some(span),
            Outcome::OpenPath { project, target } => open_code_at(self, &project, &target),
            Outcome::Copy(text) => self.copy_answered(text),
            Outcome::Show(surface) => {
                if let Some(id) = self.declared_open {
                    self.show_surface(id, &surface);
                }
            }
        }
        self.dirty = true;
    }

    /// Opens the full-frame `surface` of the extension slot `from` belongs
    /// to, as one of its surfaces asked.
    pub(super) fn show_surface(&mut self, from: SlotId, surface: &str) {
        let declared = &self.remembered.declared;
        let Some(owner) = declared.slot(from).map(|slot| slot.owner.clone()) else {
            return;
        };
        let shown = declared
            .slots
            .iter()
            .find(|slot| slot.owner == owner && slot.surface.id == surface && slot.is_view())
            .map(|slot| slot.id);
        if let Some(id) = shown {
            open_declared(self, id);
        }
    }

    /// Text a surface asked to have copied: on the clipboard, and said.
    pub(super) fn copy_answered(&mut self, text: String) {
        self.raise_toast(ToastKind::Done, "copied", text.clone(), None);
        self.clipboard = Some(text);
    }

    /// Scrolls the open view to show line `first` at the top, as its held
    /// scrollbar asks.
    pub(super) fn scroll_declared_to(&mut self, first: usize, space: Size) {
        let Some(id) = self.declared_open else {
            return;
        };
        if let Some((surface, engine)) = self.remembered.declared.slot_engine_mut(id) {
            engine.scroll_to(&surface, first, space);
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
            .slot_engine_mut(id)
            .map(|(surface, engine)| engine.command(&surface, command, space))
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
            .slot_engine_mut(id)
            .map(|(surface, engine)| engine.hit(&surface, hit, space))
            .unwrap_or(Outcome::Stay);
        if let Outcome::Show(surface) = &outcome {
            self.show_surface(id, surface);
            self.dirty = true;
            return;
        }
        if self.open_drawer() == Some(id) {
            self.follow_drawer(outcome);
            return;
        }
        // A section is part of the sidebar: it has no "close" of its own.
        if self.declared_open == Some(id) || outcome != Outcome::Close {
            self.follow_declared(outcome);
        }
        self.dirty = true;
    }

    pub(super) fn scroll_declared(
        &mut self,
        target: ScrollTarget,
        direction: ScrollDirection,
        space: Size,
    ) {
        let Some(id) = self.declared_open else {
            return;
        };
        if let Some((surface, engine)) = self.remembered.declared.slot_engine_mut(id) {
            engine.scroll(&surface, target, direction, space);
        }
        self.dirty = true;
    }

    /// What the open view draws this frame: the engine's answer, or, when
    /// it has none to give yet, a message saying why. Its notice is the
    /// host's last word from the engine, when it said one.
    pub(super) fn declared_view(&self, space: Size) -> Option<View> {
        let slot = self.open_declared_slot()?;
        let mut view = self.surface_view(slot, space);
        if let Some(notice) = &self.declared_notice {
            view.notice = Some(notice.clone());
        }
        Some(view)
    }

    /// What a view or drawer slot draws in `space`: its engine's answer,
    /// or a message saying why it has none yet.
    pub(super) fn surface_view(&self, slot: &DeclaredSlot, space: Size) -> View {
        let errors = self.remembered.declared.errors_of(&slot.owner);
        let drawn = self
            .remembered
            .declared
            .engine(&slot.owner)
            .and_then(|engine| engine.view(&slot.surface.id, space));
        drawn.unwrap_or_else(|| {
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
        })
    }

    /// The declared sections the sidebar draws, in slot order: the engine's
    /// own, with the error said in its caption and, open, in its rows.
    /// A section with nothing to draw and nothing wrong takes no room.
    pub(super) fn declared_sections(&self) -> Vec<(SlotId, Section)> {
        let declared = &self.remembered.declared;
        declared
            .slots
            .iter()
            .filter(|slot| {
                slot.surface.slot == SurfaceSlot::Section && self.offers_declared(&slot.owner)
            })
            .filter_map(|slot| {
                let errors = declared.errors_of(&slot.owner);
                let drawn = declared
                    .engine(&slot.owner)
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
        if self
            .open_declared_slot()
            .is_some_and(|slot| slot.owner.declared().is_some())
        {
            self.inspecting = !self.inspecting;
            self.inspector_scroll.set(0);
            self.dirty = true;
        }
    }

    /// Scrolls the inspector by `rows`, down when positive. The frame
    /// stops it at its last line.
    pub(super) fn scroll_inspector(&mut self, rows: i16) {
        let scroll = self.inspector_scroll.get().saturating_add_signed(rows);
        self.inspector_scroll.set(scroll);
        self.dirty = true;
    }

    /// Scrolls the inspector a page, keeping a line of the last one.
    pub(super) fn page_inspector(&mut self, down: bool) {
        let page =
            i16::try_from(self.inspector_rows.get().saturating_sub(1).max(1)).unwrap_or(i16::MAX);
        self.scroll_inspector(if down { page } else { -page });
    }

    /// What the inspector shows of the open view.
    pub(super) fn inspection(&self) -> Option<Inspection> {
        let slot = self.open_declared_slot()?;
        let mut inspection = self
            .remembered
            .declared
            .engine(&slot.owner)
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
    if !slot.is_view() || !model.offers_declared(&slot.owner) {
        return;
    }
    // An agent's surface, never a shell of the space's: a shell works in
    // no project, wherever it stands.
    let Some(root) = model.focused_cwd() else {
        return;
    };
    model.close_extension();
    model.stand_extension_in_front();
    let checkout = Checkout {
        root: root.clone(),
        display: crate::ui::display_project_path(&root),
        target: model
            .remembered
            .targets
            .get(&evaluation_key(&root))
            .cloned(),
    };
    let declared = &mut model.remembered.declared;
    if let Some(slot) = declared.slots.iter().find(|slot| slot.id == id) {
        let key = (slot.owner.clone(), slot.surface.id.clone(), root.clone());
        if let Some(engine) = declared.engines.get_mut(&key.0) {
            engine.open(&key.1, &checkout);
            if let Some(place) = declared.places.remove(&key) {
                engine.resume(&key.1, place);
            }
        }
    }
    model.declared_open = Some(id);
    model.declared_root = Some(root);
    model.code_tree_scroll = extension_view::NavigatorScroll::default();
    model.dirty = true;
}
