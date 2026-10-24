//! What a `tests/*.test.kdl` declares, once read.
//!
//! A test names the world a surface is drawn in, then a list of steps: the
//! gestures an operator could make and what should follow from them. The
//! runner drives the same engine, renderer and keymap the workspace does,
//! so these types say only *what* a test asks, never how it is checked.

use serde::{Deserialize, Serialize};

use crate::{NodeName, Span};

/// One test file: its tests, in the order it writes them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TestFile {
    pub tests: Vec<TestDecl>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestDecl {
    pub name: String,
    pub span: Span,
    pub world: World,
    pub steps: Vec<Step>,
}

/// What the surface is drawn in. Whatever a test leaves unsaid takes the
/// stated default: the source's own fixture, the slot's size, the dark
/// theme and `en`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub surface: String,
    /// Data given to a source, overriding its declared fixture.
    pub given: Vec<Given>,
    pub size: Option<(u16, u16)>,
    pub theme: Option<ThemeName>,
    pub locale: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Given {
    pub source: String,
    pub data: GivenData,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GivenData {
    /// A JSON file, relative to the extension's directory.
    Fixture(String),
    /// JSON written in the test itself.
    Json(serde_json::Value),
}

/// The two built-in palettes a snapshot is taken in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ThemeName {
    Dark,
    Light,
}

impl ThemeName {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub kind: StepKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StepKind {
    /// The text is drawn somewhere on the surface.
    ExpectText(String),
    /// The text is drawn nowhere on the surface.
    ExpectNoText(String),
    /// A node of this kind was not drawn, for want of `because`: the field
    /// a `when` or a reference missed, or the UI value a `show` compared.
    ExpectAbsent { node: NodeName, because: String },
    /// The action was recorded, with each of `args` equal to the same key
    /// of the item it received.
    ExpectAction {
        action: String,
        args: Vec<(String, serde_json::Value)>,
    },
    /// A click on what draws this text: a tab, a row, a toggle.
    Click(String),
    /// A chord, as the keymap spells it (`down`, `enter`, `tab`).
    Press(String),
    /// A press of the button drawn with this label.
    PressButton(String),
    /// The cells drawn match the snapshot of this name.
    Snapshot(String),
}

/// The directives a test's body may hold, with what each does: the world
/// first, then the steps. One table for the decoder, the runner's errors and
/// the language server.
pub const STEPS: &[(&str, &str)] = &[
    ("surface", "The surface this test opens, by its id in the manifest."),
    ("given", "Data for a source: `fixture=\"…\"` or `json=#\"…\"#`."),
    ("size", "The columns and rows the surface is drawn in."),
    ("theme", "`dark` or `light`."),
    ("locale", "The locale lookups are read in, such as `pt`."),
    ("expect-text", "The text is drawn somewhere on the surface."),
    ("expect-no-text", "The text is drawn nowhere on the surface."),
    ("expect-absent", "A node of this kind was not drawn, `because` a field or UI value."),
    ("expect-action", "The action was recorded; each property must equal the same key of its item."),
    ("click", "Click what draws this text: a tab, a row, a toggle."),
    ("press", "Press a chord, as the keymap spells it."),
    ("press-button", "Press the button drawn with this label."),
    ("snapshot", "Compare the cells drawn with the snapshot of this name."),
];

/// The one-line description of a test directive.
pub fn step_doc(name: &str) -> Option<&'static str> {
    STEPS
        .iter()
        .find(|(step, _)| *step == name)
        .map(|(_, doc)| *doc)
}
