//! A declared extension: one a plugin wrote in KDL, hosted through the
//! [`Extension`] contract.
//!
//! The engine holds what it was handed (the compiled extension and its
//! sources' data) and the UI state the host keeps for it, and binds the one
//! to the other into a [`View`] or a [`Section`] on demand. It parses
//! nothing: what reaches it was compiled by `uze-core` and read by the host.

use std::collections::BTreeMap;

use uze_contract::CompiledExtension;

use crate::contract::{Extension, Outcome, SurfaceInfo, SurfaceSlot};
use crate::view::{Command, ScrollDirection, ScrollTarget, Section, Size, View, ViewHit};

/// What a source answered: its data, or why there is none.
pub type SourceData = Result<serde_json::Value, String>;

/// A gesture that would have run an action, kept for the inspector: this
/// version runs nothing an extension ships.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedAction {
    pub surface: String,
    pub action: String,
    /// The `item` the action would have received.
    pub item: serde_json::Value,
}

pub struct DeclaredExtension {
    compiled: CompiledExtension,
    /// By source id.
    data: BTreeMap<String, SourceData>,
    /// `ui.<id>` values, by surface then id.
    ui: BTreeMap<String, BTreeMap<String, String>>,
    locale: String,
    recorded: Vec<RecordedAction>,
}

impl DeclaredExtension {
    pub fn new(compiled: CompiledExtension, locale: impl Into<String>) -> Self {
        Self {
            compiled,
            data: BTreeMap::new(),
            ui: BTreeMap::new(),
            locale: locale.into(),
            recorded: Vec::new(),
        }
    }

    pub fn compiled(&self) -> &CompiledExtension {
        &self.compiled
    }

    /// A newer compile of the same extension, such as after a linked
    /// extension was saved. UI state is kept where its ids still exist.
    pub fn replace(&mut self, compiled: CompiledExtension) {
        self.compiled = compiled;
    }

    pub fn set_data(&mut self, source: impl Into<String>, data: SourceData) {
        self.data.insert(source.into(), data);
    }

    pub fn data(&self, source: &str) -> Option<&SourceData> {
        self.data.get(source)
    }

    pub fn ui(&self, surface: &str) -> Option<&BTreeMap<String, String>> {
        self.ui.get(surface)
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn recorded(&self) -> &[RecordedAction] {
        &self.recorded
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
                },
                title: surface
                    .title
                    .clone()
                    .unwrap_or_else(|| self.compiled.manifest.name.clone()),
            })
            .collect()
    }

    fn view(&self, _surface: &str, _space: Size) -> Option<View> {
        None
    }

    fn section(&self, _surface: &str, _collapsed: bool) -> Option<Section> {
        None
    }

    fn command(&mut self, _surface: &str, command: Command, _space: Size) -> Outcome {
        match command {
            Command::Close => Outcome::Close,
            _ => Outcome::Stay,
        }
    }

    fn hit(&mut self, _surface: &str, hit: ViewHit, _space: Size) -> Outcome {
        match hit {
            ViewHit::Close => Outcome::Close,
            _ => Outcome::Stay,
        }
    }

    fn scroll(&mut self, _surface: &str, _target: ScrollTarget, _direction: ScrollDirection) {}
}
