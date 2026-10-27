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
    /// What a granted program reads beyond the host's own paths, by name.
    pub exec: Vec<ExecDecl>,
    /// The settings the operator edits, in the order written.
    pub preferences: Vec<PreferenceDecl>,
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

    pub fn wants(&self, grant: &Grant) -> bool {
        self.wants.contains(grant)
    }

    /// The programs a bare name may run, as `wants` grants them.
    pub fn programs(&self) -> impl Iterator<Item = &str> {
        self.wants.iter().filter_map(Grant::program)
    }

    /// The paths `program` reads that its `exec` declaration names.
    pub fn reads_of(&self, program: &str) -> &[String] {
        self.exec
            .iter()
            .find(|decl| decl.name == program)
            .map_or(&[], |decl| decl.reads.as_slice())
    }

    pub fn preference(&self, name: &str) -> Option<&PreferenceDecl> {
        self.preferences
            .iter()
            .find(|preference| preference.name == name)
    }

    /// Whether the extension keeps a store, and where.
    pub fn store(&self) -> Option<StoreScope> {
        if self.wants.contains(&Grant::StoreGlobal) {
            Some(StoreScope::Global)
        } else if self.wants.contains(&Grant::Store) {
            Some(StoreScope::Project)
        } else {
            None
        }
    }
}

/// Where an extension's store is kept.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StoreScope {
    /// One per project the extension is enabled in.
    Project,
    /// One for the machine, consented to on its own.
    Global,
}

/// The paths a granted program reads that the host would not otherwise
/// let it: a device, a driver's directory. Listed on the consent screen.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecDecl {
    pub name: String,
    pub reads: Vec<String>,
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
    /// One row of the sidebar's header.
    Status,
    /// Rows reserved along the top or bottom of each agent's pane.
    Band,
    /// A modal card over the frame, opened by a gesture.
    Overlay,
}

impl Slot {
    pub const NAMES: &'static [&'static str] =
        &["view", "section", "drawer", "status", "band", "overlay"];

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "view" => Some(Self::View),
            "section" => Some(Self::Section),
            "drawer" => Some(Self::Drawer),
            "status" => Some(Self::Status),
            "band" => Some(Self::Band),
            "overlay" => Some(Self::Overlay),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Section => "section",
            Self::Drawer => "drawer",
            Self::Status => "status",
            Self::Band => "band",
            Self::Overlay => "overlay",
        }
    }
}

/// The most rows a band may take from an agent's pane.
pub const BAND_ROWS_MAX: u8 = 6;

/// Where a band stands on its pane, and how many rows it takes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BandDecl {
    pub rows: u8,
    pub place: BandPlace,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum BandPlace {
    #[default]
    Top,
    Bottom,
}

impl BandPlace {
    pub const NAMES: &'static [&'static str] = &["top", "bottom"];

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// A setting an extension declares and the operator edits.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreferenceDecl {
    pub name: String,
    /// What the Extensions screen calls it.
    pub label: String,
    pub kind: PreferenceKind,
    /// What it holds until the operator says otherwise.
    pub default: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PreferenceKind {
    Text,
    /// Bounds are whole numbers; a value between them may have a fraction.
    Number {
        min: Option<i64>,
        max: Option<i64>,
    },
    Toggle,
    Choice {
        values: Vec<String>,
    },
}

impl PreferenceKind {
    pub const NAMES: &'static [&'static str] = &["text", "number", "toggle", "choice"];
}

impl PreferenceDecl {
    /// `value` when it is one this preference may hold, else its default.
    pub fn admit(&self, value: Option<&serde_json::Value>) -> serde_json::Value {
        value
            .filter(|value| self.holds(value))
            .cloned()
            .unwrap_or_else(|| self.default.clone())
    }

    /// Whether `value` is one this preference may hold.
    pub fn holds(&self, value: &serde_json::Value) -> bool {
        match &self.kind {
            PreferenceKind::Text => value.is_string(),
            PreferenceKind::Number { min, max } => value.as_f64().is_some_and(|number| {
                min.is_none_or(|min| number >= min as f64)
                    && max.is_none_or(|max| number <= max as f64)
            }),
            PreferenceKind::Toggle => value.is_boolean(),
            PreferenceKind::Choice { values } => value
                .as_str()
                .is_some_and(|chosen| values.iter().any(|value| value == chosen)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SurfaceDecl {
    pub id: String,
    pub slot: Slot,
    /// The view file, relative to the extension's directory.
    pub view: String,
    /// A band's rows and place; `None` for every other slot.
    pub band: Option<BandDecl>,
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
    /// How a program's standard output is read.
    pub output: Output,
    /// Whether the extension's store is part of what it is asked with.
    pub with_store: bool,
    /// Whether the extension's preferences are part of what it is asked
    /// with.
    pub with_preferences: bool,
    /// How events that ask it again are handed over.
    pub deliver: Deliver,
}

/// How events that ask a source again reach it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Deliver {
    /// Coalesced in a short window: it is asked once, with the last.
    #[default]
    Latest,
    /// Asked once per event, in order, a bounded number per window.
    Each,
}

impl Deliver {
    pub const NAMES: &'static [&'static str] = &["latest", "each"];
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
            output: Output::Json,
            with_store: false,
            with_preferences: false,
            deliver: Deliver::Latest,
        }
    }

    /// The JSON file the source is fed by, when a fixture answers it.
    pub fn fixture(&self) -> Option<&str> {
        match &self.kind {
            SourceKind::Fixture(file) => Some(file),
            SourceKind::Script(_) | SourceKind::Run(_) | SourceKind::Events(_) => None,
        }
    }

    /// The Rhai file that answers the source, when a script does.
    pub fn script(&self) -> Option<&str> {
        match &self.kind {
            SourceKind::Script(file) => Some(file),
            SourceKind::Fixture(_) | SourceKind::Run(_) | SourceKind::Events(_) => None,
        }
    }
}

/// Who answers a source. Every file is relative to the extension's
/// directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceKind {
    /// A Rhai file whose `answer(input, context)` returns the envelope.
    Script(String),
    /// A program the host runs, confined where the machine can.
    Run(Program),
    /// A JSON file whose contents are the source's data.
    Fixture(String),
    /// The host's own: the last this many events the extension may see.
    Events(u16),
}

impl SourceKind {
    /// How a manifest spells each kind.
    pub const NAMES: &'static [&'static str] = &["script", "run", "fixture", "events"];
}

/// The most events an `events` source keeps.
pub const EVENTS_MAX: u16 = 200;

/// The longest a program may run, and what it runs for when its
/// declaration says nothing.
pub const PROGRAM_TIME_LIMIT: Duration = Duration::from_secs(10);

/// A program as a manifest declares it: an argv whose first element is a
/// path inside the extension or a name an `exec:` grant covers, each later
/// element a whole argument in which `{field}` is substituted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub argv: Vec<String>,
    /// Lower than [`PROGRAM_TIME_LIMIT`] when the manifest says so.
    pub timeout: Option<Duration>,
}

impl Program {
    pub fn new(argv: Vec<String>) -> Self {
        Self {
            argv,
            timeout: None,
        }
    }

    pub fn time_limit(&self) -> Duration {
        self.timeout
            .map_or(PROGRAM_TIME_LIMIT, |limit| limit.min(PROGRAM_TIME_LIMIT))
    }

    /// The program's first element: `./bin/x` or `nvidia-smi`.
    pub fn program(&self) -> &str {
        self.argv.first().map_or("", String::as_str)
    }

    /// Whether it names a file inside the extension rather than a program
    /// an `exec:` grant covers.
    pub fn is_packaged(&self) -> bool {
        self.program().contains('/')
    }
}

/// How a program's standard output becomes a source's data.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Output {
    /// One JSON value, or an envelope around one.
    #[default]
    Json,
    /// A tree of catalog nodes the host validates and draws.
    View,
    /// One item per line, its fields named in order and split at
    /// `separator`, a tab unless the declaration names another, each
    /// trimmed of the spaces around it.
    Lines {
        fields: Vec<String>,
        separator: char,
    },
}

impl Output {
    pub const NAMES: &'static [&'static str] = &["json", "lines", "view"];
    pub const TAB: char = '\t';
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
    /// Runs a program, confined where the machine can, and applies the
    /// envelope it answers.
    Run(Program),
}

impl Does {
    /// How a manifest spells each, as the node inside an `action`.
    pub const NAMES: &'static [&'static str] =
        &["script", "refresh", "clipboard", "open-path", "open", "run"];

    /// The grant it cannot run without, when it needs one of its own.
    pub fn needs(&self) -> Option<Grant> {
        match self {
            Self::Clipboard(_) => Some(Grant::Clipboard),
            Self::OpenPath(_) => Some(Grant::OpenPath),
            Self::Script(_) | Self::Refresh(_) | Self::Open(_) | Self::Run(_) => None,
        }
    }
}

/// What an extension may reach beyond drawing. Only the grants this build
/// enforces exist here; the decoder names the change that brings any
/// other.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum Grant {
    /// Reading files beneath the checkout the surface is on.
    ReadCheckout,
    /// Writing text to the clipboard, on a gesture.
    Clipboard,
    /// Opening a file inside the checkout, on a gesture.
    OpenPath,
    /// Keeping a store per project.
    Store,
    /// Keeping one store for the machine, across projects.
    StoreGlobal,
    /// Notifying the operator and sounding a cue, rate-limited.
    Notify,
    /// Running the program of this name, at the path pinned when the
    /// extension was enabled.
    Exec(String),
}

impl Grant {
    /// The grants named by a fixed word; `exec:<name>` is the one family.
    pub const FIXED: &'static [Grant] = &[
        Self::ReadCheckout,
        Self::Clipboard,
        Self::OpenPath,
        Self::Store,
        Self::StoreGlobal,
        Self::Notify,
    ];
    pub const EXEC: &'static str = "exec:";

    pub fn parse(name: &str) -> Option<Self> {
        if let Some(program) = name.strip_prefix(Self::EXEC) {
            return is_program_name(program).then(|| Self::Exec(program.to_owned()));
        }
        Self::FIXED
            .iter()
            .find(|grant| grant.name() == name)
            .cloned()
    }

    pub fn name(&self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(match self {
            Self::ReadCheckout => "read:checkout",
            Self::Clipboard => "clipboard",
            Self::OpenPath => "open-path",
            Self::Store => "store",
            Self::StoreGlobal => "store:global",
            Self::Notify => "notify",
            Self::Exec(program) => return format!("{}{program}", Self::EXEC).into(),
        })
    }

    /// The program an `exec:` grant covers.
    pub fn program(&self) -> Option<&str> {
        match self {
            Self::Exec(program) => Some(program),
            _ => None,
        }
    }
}

/// A program's name as `exec:` spells it: a file name a search path can
/// hold, never a path.
pub fn is_program_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+'))
}

impl std::fmt::Display for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
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
    fn a_grant_is_one_this_build_enforces() {
        for grant in Grant::FIXED {
            assert_eq!(Grant::parse(&grant.name()), Some(grant.clone()));
        }
        assert_eq!(Grant::parse("exec:gh"), Some(Grant::Exec("gh".into())));
        assert_eq!(Grant::Exec("nvidia-smi".into()).name(), "exec:nvidia-smi");
        for later in [
            "net",
            "write:checkout",
            "open_path",
            "Clipboard",
            "exec:",
            "exec:../gh",
            "exec:/usr/bin/gh",
            "exec:-x",
            "exec:a b",
        ] {
            assert_eq!(Grant::parse(later), None, "{later}");
        }
    }

    #[test]
    fn a_program_runs_for_at_most_the_host_limit() {
        let mut program = Program::new(vec!["nvidia-smi".into()]);
        assert_eq!(program.time_limit(), PROGRAM_TIME_LIMIT);
        assert!(!program.is_packaged());
        program.timeout = Some(Duration::from_secs(60));
        assert_eq!(program.time_limit(), PROGRAM_TIME_LIMIT);
        program.timeout = Some(Duration::from_secs(2));
        assert_eq!(program.time_limit(), Duration::from_secs(2));
        assert!(Program::new(vec!["./bin/meter".into()]).is_packaged());
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
