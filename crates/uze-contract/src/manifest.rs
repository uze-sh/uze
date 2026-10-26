//! What an `extension.kdl` declares, once read.
//!
//! An extension's surfaces, the sources that feed them, the actions it
//! offers and the grants it wants (`extension-manifest-and-scripts`). A
//! manifest says what would run; whether anything does is the consent
//! record's, so nothing here runs or reaches anything by being declared.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The shortest interval `every` may ask a source to be refreshed at.
pub const EVERY_FLOOR: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub protocol: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    /// What the extension asks to reach, in the order written.
    pub wants: Vec<Grant>,
    pub surfaces: Vec<SurfaceDecl>,
    pub sources: Vec<SourceDecl>,
    pub actions: Vec<ActionDecl>,
}

impl Manifest {
    pub fn surface(&self, id: &str) -> Option<&SurfaceDecl> {
        self.surfaces.iter().find(|surface| surface.id == id)
    }

    pub fn source(&self, id: &str) -> Option<&SourceDecl> {
        self.sources.iter().find(|source| source.id == id)
    }

    pub fn action(&self, id: &str) -> Option<&ActionDecl> {
        self.actions.iter().find(|action| action.id == id)
    }

    pub fn wants(&self, grant: Grant) -> bool {
        self.wants.contains(&grant)
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
    /// Who answers the source.
    pub kind: SourceKind,
    /// What the answer is asked with, by name: text in which `{field}`
    /// refers to a value the host resolves when it asks.
    pub input: BTreeMap<String, String>,
    /// How often the source is asked again while a surface showing it is
    /// visible; never under [`EVERY_FLOOR`].
    pub every: Option<Duration>,
    /// What else asks it again.
    pub refresh_on: Vec<EventName>,
}

impl SourceDecl {
    /// A source answered by `kind`, with no input and nothing that
    /// refreshes it.
    pub fn new(id: impl Into<String>, kind: SourceKind) -> Self {
        Self {
            id: id.into(),
            kind,
            input: BTreeMap::new(),
            every: None,
            refresh_on: Vec::new(),
        }
    }

    /// The JSON file the source is fed by, when a fixture answers it.
    pub fn fixture(&self) -> Option<&str> {
        match &self.kind {
            SourceKind::Fixture(file) => Some(file),
            SourceKind::Script(_) | SourceKind::Run(_) => None,
        }
    }

    /// The Rhai file that answers the source, when a script does.
    pub fn script(&self) -> Option<&str> {
        match &self.kind {
            SourceKind::Script(file) => Some(file),
            SourceKind::Fixture(_) | SourceKind::Run(_) => None,
        }
    }
}

/// Who answers a source. Every file is relative to the extension's
/// directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceKind {
    /// A Rhai file whose `answer(input, context)` returns the envelope.
    Script(String),
    /// A program and its arguments. Decoded so the check can name the
    /// change that brings programs; no manifest that holds one is hosted.
    Run(Vec<String>),
    /// A JSON file whose contents are the source's data.
    Fixture(String),
}

impl SourceKind {
    /// How a manifest spells each kind.
    pub const NAMES: &'static [&'static str] = &["script", "run", "fixture"];
}

/// A reason, other than time, to ask a source again.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventName {
    /// A file beneath one of these paths, relative to the checkout,
    /// changed.
    PathsChanged { paths: Vec<String> },
    /// The checkout's branch, head or working tree changed.
    CheckoutChanged,
}

impl EventName {
    pub const PATHS_CHANGED: &'static str = "paths.changed";
    pub const CHECKOUT_CHANGED: &'static str = "checkout.changed";
    pub const NAMES: &'static [&'static str] = &[Self::PATHS_CHANGED, Self::CHECKOUT_CHANGED];

    pub fn name(&self) -> &'static str {
        match self {
            Self::PathsChanged { .. } => Self::PATHS_CHANGED,
            Self::CheckoutChanged => Self::CHECKOUT_CHANGED,
        }
    }
}

/// Something an extension offers the operator to do on one of its
/// surfaces.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionDecl {
    pub id: String,
    /// What the footer, the action index and a menu call it.
    pub label: String,
    /// The surface it is offered on.
    pub on: String,
    /// The chord the extension suggests; bound only where it is free.
    pub key: Option<KeySuggestion>,
    /// The question asked before it runs, with `{field}` references to the
    /// item it runs on. An action without one runs on the gesture.
    pub confirm: Option<String>,
    pub does: Does,
}

/// What an action does when it runs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Does {
    /// Runs this Rhai file's `answer`, relative to the extension's
    /// directory, and applies its envelope.
    Script(String),
    /// Asks these sources again.
    Refresh(Vec<String>),
    /// Copies this text, with `{field}` references, to the clipboard.
    Clipboard(String),
    /// Opens the file at this path, with `{field}` references, inside the
    /// checkout.
    OpenPath(String),
    /// Opens this surface of the same extension.
    Open(String),
}

impl Does {
    /// How a manifest spells each, as the node inside an `action`.
    pub const NAMES: &'static [&'static str] =
        &["script", "refresh", "clipboard", "open-path", "open"];

    /// The grant it cannot run without, when it needs one of its own.
    pub fn needs(&self) -> Option<Grant> {
        match self {
            Self::Clipboard(_) => Some(Grant::Clipboard),
            Self::OpenPath(_) => Some(Grant::OpenPath),
            Self::Script(_) | Self::Refresh(_) | Self::Open(_) => None,
        }
    }
}

/// What an extension may reach beyond drawing. Only the grants this build
/// enforces exist here; the decoder names the change that brings any
/// other.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum Grant {
    /// Reading files beneath the checkout the surface is on.
    ReadCheckout,
    /// Writing text to the clipboard, on a gesture.
    Clipboard,
    /// Opening a file inside the checkout, on a gesture.
    OpenPath,
}

impl Grant {
    pub const ALL: &'static [Grant] = &[Self::ReadCheckout, Self::Clipboard, Self::OpenPath];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|grant| grant.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::ReadCheckout => "read:checkout",
            Self::Clipboard => "clipboard",
            Self::OpenPath => "open-path",
        }
    }
}

impl std::fmt::Display for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A chord as a manifest suggests one: `ctrl-r`, `alt-shift-x`, `f5`, `d`.
/// Modifiers lowercase, joined to the key by `-`; the key a single
/// printable character or one of [`KeySuggestion::NAMED`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KeySuggestion {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

impl KeySuggestion {
    pub const MODIFIERS: &'static [&'static str] = &["ctrl", "alt", "shift"];
    pub const NAMED: &'static [&'static str] = &[
        "enter",
        "esc",
        "tab",
        "space",
        "backspace",
        "delete",
        "insert",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
        "f1",
        "f2",
        "f3",
        "f4",
        "f5",
        "f6",
        "f7",
        "f8",
        "f9",
        "f10",
        "f11",
        "f12",
    ];

    /// `None` for anything that is not a chord in this spelling. A `-` is a
    /// key of its own only last (`ctrl--`).
    pub fn parse(text: &str) -> Option<Self> {
        let mut chord = Self {
            ctrl: false,
            alt: false,
            shift: false,
            key: String::new(),
        };
        let mut rest = text;
        while let Some((modifier, after)) =
            rest.split_once('-').filter(|(_, after)| !after.is_empty())
        {
            let flag = match modifier {
                "ctrl" => &mut chord.ctrl,
                "alt" => &mut chord.alt,
                "shift" => &mut chord.shift,
                _ => return None,
            };
            if std::mem::replace(flag, true) {
                return None;
            }
            rest = after;
        }
        let single = {
            let mut chars = rest.chars();
            matches!((chars.next(), chars.next()), (Some(ch), None) if ch.is_ascii_graphic())
        };
        if !single && !Self::NAMED.contains(&rest) {
            return None;
        }
        chord.key = rest.to_owned();
        Some(chord)
    }

    /// A single character with no modifier: what a stray keystroke presses.
    pub fn is_bare(&self) -> bool {
        !(self.ctrl || self.alt || self.shift) && self.key.chars().count() == 1
    }
}

impl std::fmt::Display for KeySuggestion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (on, name) in [
            (self.ctrl, "ctrl"),
            (self.alt, "alt"),
            (self.shift, "shift"),
        ] {
            if on {
                write!(f, "{name}-")?;
            }
        }
        f.write_str(&self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_is_one_of_the_three_this_build_enforces() {
        for grant in Grant::ALL {
            assert_eq!(Grant::parse(grant.name()), Some(*grant));
        }
        for later in ["net", "exec:gh", "write:checkout", "open_path", "Clipboard"] {
            assert_eq!(Grant::parse(later), None, "{later}");
        }
    }

    #[test]
    fn a_suggested_key_reads_modifiers_then_one_key() {
        let chord = KeySuggestion::parse("ctrl-shift-r").unwrap();
        assert!(chord.ctrl && chord.shift && !chord.alt);
        assert_eq!(chord.key, "r");
        assert_eq!(chord.to_string(), "ctrl-shift-r");
        assert_eq!(KeySuggestion::parse("ctrl--").unwrap().key, "-");
        assert_eq!(KeySuggestion::parse("f5").unwrap().key, "f5");
        for bad in [
            "",
            "ctrl-",
            "ctrl-ctrl-r",
            "super-r",
            "ctrl+r",
            "ctrl-xy",
            "Ctrl-r",
        ] {
            assert_eq!(KeySuggestion::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_an_unmodified_character_is_bare() {
        for bare in ["d", "D", "1", "/"] {
            assert!(KeySuggestion::parse(bare).unwrap().is_bare(), "{bare}");
        }
        for chord in ["ctrl-d", "shift-d", "enter", "f2"] {
            assert!(!KeySuggestion::parse(chord).unwrap().is_bare(), "{chord}");
        }
    }

    #[test]
    fn a_source_says_which_file_answers_it() {
        let fixture = SourceDecl::new("board", SourceKind::Fixture("board.json".into()));
        assert_eq!(fixture.fixture(), Some("board.json"));
        assert_eq!(fixture.script(), None);
        let script = SourceDecl::new("board", SourceKind::Script("board.rhai".into()));
        assert_eq!(script.script(), Some("board.rhai"));
        assert_eq!(script.fixture(), None);
    }

    #[test]
    fn an_action_that_copies_or_opens_a_path_needs_its_grant() {
        assert_eq!(
            Does::Clipboard("{hash}".into()).needs(),
            Some(Grant::Clipboard)
        );
        assert_eq!(
            Does::OpenPath("{path}".into()).needs(),
            Some(Grant::OpenPath)
        );
        assert_eq!(Does::Refresh(vec!["board".into()]).needs(), None);
    }
}
