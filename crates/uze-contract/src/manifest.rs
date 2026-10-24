//! What an `extension.kdl` declares, once read.
//!
//! The first slice of the manifest (`extension-view-engine`): an extension's
//! surfaces and the sources that feed them. Sources are fixtures only here;
//! scripts, programs, actions, events and grants join with the changes that
//! run them, so nothing in this version can name something that would run.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub protocol: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub surfaces: Vec<SurfaceDecl>,
    pub sources: Vec<SourceDecl>,
}

impl Manifest {
    pub fn surface(&self, id: &str) -> Option<&SurfaceDecl> {
        self.surfaces.iter().find(|surface| surface.id == id)
    }

    pub fn source(&self, id: &str) -> Option<&SourceDecl> {
        self.sources.iter().find(|source| source.id == id)
    }
}

/// Where a surface is drawn. The host owns its geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Slot {
    /// The full frame, opened like the built-in surfaces.
    View,
    /// A collapsible block of the workspace sidebar.
    Section,
    /// A panel docked along the right of the agent area, opened by a
    /// gesture and laid beside the pane rather than over it.
    Drawer,
}

impl Slot {
    pub const NAMES: &'static [&'static str] = &["view", "section", "drawer"];

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "view" => Some(Self::View),
            "section" => Some(Self::Section),
            "drawer" => Some(Self::Drawer),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Section => "section",
            Self::Drawer => "drawer",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SurfaceDecl {
    pub id: String,
    pub slot: Slot,
    /// The view file, relative to the extension's directory.
    pub view: String,
    /// The source whose data the view is bound to.
    pub source: String,
    /// What the host titles the surface with; the extension's name when
    /// absent.
    pub title: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceDecl {
    pub id: String,
    /// A JSON file, relative to the extension's directory, whose contents
    /// are the source's data.
    pub fixture: Option<String>,
}
