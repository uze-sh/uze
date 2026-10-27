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

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// The frame sheets its `Sprite` nodes play.
    pub sprites: Vec<SpriteDecl>,
}

/// A sheet of equal frames and the states that play them
/// (`extension-media-and-resident`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpriteDecl {
    pub id: String,
    /// A PNG inside the extension.
    pub sheet: String,
    /// One frame's size, in pixels.
    pub frame: (u32, u32),
    /// Frames in a row of the sheet.
    pub columns: u32,
    pub states: Vec<SpriteState>,
    /// The state played when the bound one is none of these.
    pub fallback: String,
}

/// One animation of a sprite: frame indices into the sheet, read left to
/// right and top to bottom.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpriteState {
    pub name: String,
    pub frames: Vec<u32>,
    /// Frames a second, 1 to 30.
    pub fps: u32,
    /// Whether it starts over at its end, or holds its last frame.
    pub looped: bool,
}

impl Manifest {
    pub fn sprite(&self, id: &str) -> Option<&SpriteDecl> {
        self.sprites.iter().find(|sprite| sprite.id == id)
    }

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

    /// The hosts `fetch` may reach, as `wants` grants them.
    pub fn hosts(&self) -> impl Iterator<Item = &str> {
        self.wants.iter().filter_map(Grant::host)
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecDecl {
    pub name: String,
    pub reads: Vec<String>,
}

/// Where a surface is drawn. The host owns its geometry.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BandDecl {
    pub rows: u8,
    pub place: BandPlace,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreferenceDecl {
    pub name: String,
    /// What the Extensions screen calls it.
    pub label: String,
    pub kind: PreferenceKind,
    /// What it holds until the operator says otherwise.
    pub default: serde_json::Value,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PreferenceKind {
    Text,
    /// Kept in the operating system's keychain, never shown again, never
    /// read by `{pref.*}` nor handed to a script: only `fetch`'s `secret`
    /// option places it, in a request's header.
    Secret,
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
    pub const NAMES: &'static [&'static str] = &["text", "number", "toggle", "choice", "secret"];
}

/// A secret's value as it may be kept: one line of printable text, no
/// longer than a token a forge hands out, so it fits in one header.
pub fn is_secret_value(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

impl PreferenceDecl {
    /// Whether its value is a secret, kept out of every answer.
    pub fn is_secret(&self) -> bool {
        self.kind == PreferenceKind::Secret
    }

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
            PreferenceKind::Secret => value.as_str().is_some_and(is_secret_value),
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

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// A path inside the checkout the surface needs, as written
    /// (`applies-when=".planning"`): while the checkout it is about has
    /// none there, the surface is neither offered nor drawn.
    #[serde(default)]
    pub applies_when: Option<String>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// A path inside the checkout the source needs: while the checkout it
    /// would be asked about has none there, it is not asked.
    #[serde(default)]
    pub applies_when: Option<String>,
}

/// How events that ask a source again reach it.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
            applies_when: None,
        }
    }

    /// The JSON file the source is fed by, when a fixture answers it.
    pub fn fixture(&self) -> Option<&str> {
        match &self.kind {
            SourceKind::Fixture(file) => Some(file),
            SourceKind::Script(_)
            | SourceKind::Run(_)
            | SourceKind::Events(_)
            | SourceKind::Agents
            | SourceKind::Resident(_) => None,
        }
    }

    /// The Rhai file that answers the source, when a script does.
    pub fn script(&self) -> Option<&str> {
        match &self.kind {
            SourceKind::Script(file) => Some(file),
            SourceKind::Fixture(_)
            | SourceKind::Run(_)
            | SourceKind::Events(_)
            | SourceKind::Agents
            | SourceKind::Resident(_) => None,
        }
    }

    /// The Rhai file kept alive for the source, when it is resident.
    pub fn resident(&self) -> Option<&str> {
        match &self.kind {
            SourceKind::Resident(file) => Some(file),
            _ => None,
        }
    }
}

/// Who answers a source. Every file is relative to the extension's
/// directory.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// The host's own: every agent of the workspace's open spaces, under
    /// `agent:observe`.
    Agents,
    /// A Rhai file kept alive while a surface showing it is open, under
    /// `resident`: `start`, `event`, `input`, `render`, `stop`.
    Resident(String),
}

impl SourceKind {
    /// How a manifest spells each kind.
    pub const NAMES: &'static [&'static str] =
        &["script", "run", "fixture", "events", "agents", "resident"];
}

/// The most events an `events` source keeps.
pub const EVENTS_MAX: u16 = 200;

/// The longest a program may run, and what it runs for when its
/// declaration says nothing.
pub const PROGRAM_TIME_LIMIT: Duration = Duration::from_secs(10);

/// A program as a manifest declares it: an argv whose first element is a
/// path inside the extension or a name an `exec:` grant covers, each later
/// element a whole argument in which `{field}` is substituted.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventName {
    /// A file beneath one of these paths, relative to the checkout,
    /// changed.
    PathsChanged { paths: Vec<String> },
    /// The checkout's branch, head or working tree changed.
    CheckoutChanged,
    /// An agent of the workspace did something, as it reported or as the
    /// workspace saw it paint. Needs `agent:observe`.
    Agent(AgentEvent),
}

/// What an agent may be seen doing (`agent-bridge`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum AgentEvent {
    SessionStarted,
    TurnStarted,
    TurnFinished,
    ToolStarted,
    ToolFinished,
    ApprovalAsked,
    ApprovalResolved,
}

impl AgentEvent {
    pub const ALL: &'static [AgentEvent] = &[
        Self::SessionStarted,
        Self::TurnStarted,
        Self::TurnFinished,
        Self::ToolStarted,
        Self::ToolFinished,
        Self::ApprovalAsked,
        Self::ApprovalResolved,
    ];

    /// As a manifest's `refresh-on` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::SessionStarted => "agent.session.started",
            Self::TurnStarted => "agent.turn.started",
            Self::TurnFinished => "agent.turn.finished",
            Self::ToolStarted => "agent.tool.started",
            Self::ToolFinished => "agent.tool.finished",
            Self::ApprovalAsked => "agent.approval.asked",
            Self::ApprovalResolved => "agent.approval.resolved",
        }
    }

    /// `name` as a manifest or a report spells it: with or without the
    /// `agent.` an agent's own report leaves out.
    pub fn parse(name: &str) -> Option<Self> {
        let full = if name.starts_with("agent.") {
            name.to_owned()
        } else {
            format!("agent.{name}")
        };
        Self::ALL.iter().copied().find(|event| event.name() == full)
    }
}

impl EventName {
    pub const PATHS_CHANGED: &'static str = "paths.changed";
    pub const CHECKOUT_CHANGED: &'static str = "checkout.changed";
    pub const NAMES: &'static [&'static str] = &[Self::PATHS_CHANGED, Self::CHECKOUT_CHANGED];

    pub fn name(&self) -> &'static str {
        match self {
            Self::PathsChanged { .. } => Self::PATHS_CHANGED,
            Self::CheckoutChanged => Self::CHECKOUT_CHANGED,
            Self::Agent(event) => event.name(),
        }
    }
}

/// Who an agent action is aimed at.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentTarget {
    /// The agent the surface stands for: a band's pane's, or the agent the
    /// selected item names.
    Own,
    /// The agent whose id this `{field}` template resolves to, from the
    /// `agents` source.
    Named(String),
}

/// What an agent prompt types.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PromptText {
    /// Text, with `{field}` references resolved against the item when the
    /// operator presses it; fixed when unattended.
    Text(String),
    /// A skill the agent's harness knows, spelled by its integration, with
    /// its arguments.
    Skill { skill: String, args: Vec<String> },
}

/// An unattended prompt: what event sends it, and the closed set its one
/// argument is taken from, when it has one.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AutoPrompt {
    pub on: AgentEvent,
    /// The values `{arg}` may take; empty for a prompt with none.
    pub values: Vec<String>,
}

/// Something an extension offers the operator to do on one of its
/// surfaces.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// The host-held values it reads beside its item, as written:
    /// `ui.<id>`, `form.<id>` or `form.*` (D24). Only these reach it.
    #[serde(default)]
    pub with: Vec<String>,
    /// What a `run` or a `script` does with its answer once it arrives: a
    /// prompt typed into an agent, shown first, on the gesture that ran it
    /// and never unattended (D16, D24).
    #[serde(default)]
    pub then: Option<Then>,
}

impl ActionDecl {
    /// An action that does `does`, with nothing asked first, read beside
    /// its item or done after.
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        on: impl Into<String>,
        does: Does,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            on: on.into(),
            key: None,
            confirm: None,
            does,
            with: Vec::new(),
            then: None,
        }
    }

    /// Whether it reads the UI value `ui.<id>`.
    pub fn reads_ui(&self, id: &str) -> bool {
        self.with
            .iter()
            .any(|read| read.strip_prefix("ui.") == Some(id))
    }

    /// Whether it reads the form field `form.<id>`.
    pub fn reads_form(&self, id: &str) -> bool {
        self.with
            .iter()
            .any(|read| read == "form.*" || read.strip_prefix("form.") == Some(id))
    }

    /// Every grant it cannot run without: its own, and its second step's.
    pub fn needs(&self) -> Vec<Grant> {
        self.does
            .needs()
            .into_iter()
            .chain(self.then.as_ref().map(|_| Grant::AgentPrompt))
            .collect()
    }
}

/// The second step of a two-step action: a prompt whose `{answer.*}`
/// references read what the first step answered.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Then {
    pub prompt: PromptText,
    pub target: AgentTarget,
}

/// What an action does when it runs.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Types a prompt into an agent, shown first; never queued.
    AgentPrompt {
        prompt: PromptText,
        target: AgentTarget,
        /// Sent on an agent event rather than a gesture.
        auto: Option<AutoPrompt>,
    },
    /// Starts an agent's harness on a new conversation; always confirmed.
    AgentRestart { target: AgentTarget },
    /// Puts an agent's pane in front.
    FocusAgent { target: AgentTarget },
}

impl Does {
    /// How a manifest spells each, as the node inside an `action`.
    pub const NAMES: &'static [&'static str] = &[
        "script",
        "refresh",
        "clipboard",
        "open-path",
        "open",
        "run",
        "agent-prompt",
        "agent-restart",
        "focus-agent",
    ];

    /// The grant it cannot run without, when it needs one of its own.
    pub fn needs(&self) -> Option<Grant> {
        match self {
            Self::Clipboard(_) => Some(Grant::Clipboard),
            Self::OpenPath(_) => Some(Grant::OpenPath),
            Self::AgentPrompt { auto: Some(_), .. } => Some(Grant::AgentPromptAuto),
            Self::AgentPrompt { .. } => Some(Grant::AgentPrompt),
            Self::AgentRestart { .. } => Some(Grant::AgentRestart),
            Self::FocusAgent { .. } => Some(Grant::AgentObserve),
            Self::Script(_) | Self::Refresh(_) | Self::Open(_) | Self::Run(_) => None,
        }
    }
}

/// What an extension may reach beyond drawing. Only the grants this build
/// enforces exist here; the decoder names the change that brings any
/// other.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Seeing the workspace's agents and what they do.
    AgentObserve,
    /// Typing a prompt into an agent, on the operator's gesture.
    AgentPrompt,
    /// Typing a fixed prompt into an agent on its events, unattended.
    AgentPromptAuto,
    /// Starting an agent's harness on a new conversation.
    AgentRestart,
    /// Keeping a script alive while its surface is open, reading raw keys
    /// when focused.
    Resident,
    /// Running the program of this name, at the path pinned when the
    /// extension was enabled.
    Exec(String),
    /// Requesting `https://` URLs of this host, and no other, through
    /// `fetch`.
    Net(String),
}

impl Grant {
    /// The grants named by a fixed word; `exec:<name>` and `net:<host>` are
    /// the families.
    pub const FIXED: &'static [Grant] = &[
        Self::ReadCheckout,
        Self::Clipboard,
        Self::OpenPath,
        Self::Store,
        Self::StoreGlobal,
        Self::Notify,
        Self::AgentObserve,
        Self::AgentPrompt,
        Self::AgentPromptAuto,
        Self::AgentRestart,
        Self::Resident,
    ];
    pub const EXEC: &'static str = "exec:";
    pub const NET: &'static str = "net:";

    pub fn parse(name: &str) -> Option<Self> {
        if let Some(program) = name.strip_prefix(Self::EXEC) {
            return is_program_name(program).then(|| Self::Exec(program.to_owned()));
        }
        if let Some(host) = name.strip_prefix(Self::NET) {
            return is_host_name(host).then(|| Self::Net(host.to_owned()));
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
            Self::AgentObserve => "agent:observe",
            Self::AgentPrompt => "agent:prompt",
            Self::AgentPromptAuto => "agent:prompt:auto",
            Self::AgentRestart => "agent:restart",
            Self::Resident => "resident",
            Self::Exec(program) => return format!("{}{program}", Self::EXEC).into(),
            Self::Net(host) => return format!("{}{host}", Self::NET).into(),
        })
    }

    /// The program an `exec:` grant covers.
    pub fn program(&self) -> Option<&str> {
        match self {
            Self::Exec(program) => Some(program),
            _ => None,
        }
    }

    /// The host a `net:` grant covers.
    pub fn host(&self) -> Option<&str> {
        match self {
            Self::Net(host) => Some(host),
            _ => None,
        }
    }
}

/// A host as `net:` spells it: a DNS name in lower case, its labels of
/// letters, digits and inner hyphens, with no scheme, port, path or
/// wildcard, so the grant read at consent is the one host reached.
pub fn is_host_name(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
        assert_eq!(
            Grant::parse("net:api.github.com"),
            Some(Grant::Net("api.github.com".into()))
        );
        assert_eq!(
            Grant::Net("api.github.com".into()).name(),
            "net:api.github.com"
        );
        for later in [
            "net",
            "net:",
            "net:https://api.github.com",
            "net:api.github.com/repos",
            "net:api.github.com:443",
            "net:*.github.com",
            "net:API.github.com",
            "net:-x.com",
            "net:a..b",
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
