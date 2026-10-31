use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

/// The periods sessions are counted over, in days, in the order the
/// Overview offers them.
pub const PERIODS: [u32; 3] = [30, 90, 7];

/// The name every harness lists uze's own rules engine hook by, so the
/// one resource it is reads as one row whatever file each harness keeps
/// it in.
pub const UZE_RULES: &str = "uze-rules";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Skill,
    Agent,
    Mcp,
    Hook,
    Rule,
}

impl ResourceKind {
    pub const ALL: [ResourceKind; 5] =
        [Self::Skill, Self::Agent, Self::Mcp, Self::Hook, Self::Rule];
}

/// Who put a resource where the harness found it, as far as the harness
/// can tell. Whether uze did is not the harness's to say: that comes from
/// uze's receipts (see [`Claim`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Brought by a plugin the harness installed.
    Plugin { name: String },
    /// Shipped with the harness itself.
    BuiltIn,
    /// Placed by hand in one of the harness's own locations.
    ByHand,
}

/// How a receipt of this harness's integration would name a resource
/// that is not a file of its own: a config entry, or something only the
/// integration can interpret. A file is claimed by its path, which the
/// core reads from the [`Definition`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Claim {
    /// The `entry_name` of a vendor-config or hook-config receipt.
    Entry(String),
    /// The `kind` and `selector` of an integration-owned receipt.
    Owned { kind: String, selector: String },
}

/// Where a resource is defined, and so what it costs in context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Definition {
    /// A Markdown file with frontmatter: its `name` and `description` are
    /// listed in every session, the rest loads when it is used.
    Markdown(PathBuf),
    /// A file loaded whole in every session (an always-on instruction file).
    Whole(PathBuf),
    /// A file in a format only the integration reads: it hands over the text
    /// listed every session and the text loaded when used.
    Listed {
        file: PathBuf,
        per_session: String,
        when_used: Option<String>,
    },
    /// An entry inside a configuration file (an MCP server, a hook): it adds
    /// nothing the core can measure in this version.
    Entry(PathBuf),
}

impl Definition {
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Markdown(path) | Self::Whole(path) | Self::Entry(path) => path,
            Self::Listed { file, .. } => file,
        }
    }
}

/// What a resource runs, so a fact on disk can say it cannot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Runs {
    /// A command line as the harness hands it to a shell (a hook).
    Shell(String),
    /// A program started directly (a local MCP server's `command`).
    Program(String),
    /// A server reached over the network; nothing on disk can judge it.
    Url(String),
}

/// One thing a harness reads at machine level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundResource {
    pub kind: ResourceKind,
    /// The name the harness knows it by (`code:polish` for a plugin's).
    pub name: String,
    pub definition: Definition,
    pub source: Source,
    pub claim: Option<Claim>,
    pub runs: Option<Runs>,
    /// A one-line description, for a resource whose file carries none (a
    /// hook's event, an MCP server's transport).
    pub note: Option<String>,
    /// The harness's own command that removes it, shown, never run.
    pub removal: Option<String>,
    /// What the harness itself proves is wrong with it.
    pub problem: Option<String>,
}

impl FoundResource {
    pub fn new(kind: ResourceKind, name: impl Into<String>, definition: Definition) -> Self {
        Self {
            kind,
            name: name.into(),
            definition,
            source: Source::ByHand,
            claim: None,
            runs: None,
            note: None,
            removal: None,
            problem: None,
        }
    }

    pub fn from_plugin(mut self, plugin: impl Into<String>) -> Self {
        self.source = Source::Plugin {
            name: plugin.into(),
        };
        self
    }

    pub fn built_in(mut self) -> Self {
        self.source = Source::BuiltIn;
        self
    }

    pub fn claimed(mut self, claim: Claim) -> Self {
        self.claim = Some(claim);
        self
    }

    pub fn running(mut self, runs: Runs) -> Self {
        self.runs = Some(runs);
        self
    }

    pub fn noted(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn removed_by(mut self, command: impl Into<String>) -> Self {
        self.removal = Some(command.into());
        self
    }

    pub fn with_problem(mut self, problem: impl Into<String>) -> Self {
        self.problem = Some(problem.into());
        self
    }
}

/// Sessions with activity in each of [`PERIODS`], subagents excluded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionCounts {
    pub days_7: u64,
    pub days_30: u64,
    pub days_90: u64,
}

impl SessionCounts {
    /// Counts sessions by their last activity.
    pub fn from_activity(times: impl IntoIterator<Item = SystemTime>, now: SystemTime) -> Self {
        let mut counts = Self::default();
        for time in times {
            let age = now.duration_since(time).unwrap_or(Duration::ZERO);
            let days = age.as_secs() / 86_400;
            if days < 7 {
                counts.days_7 += 1;
            }
            if days < 30 {
                counts.days_30 += 1;
            }
            if days < 90 {
                counts.days_90 += 1;
            }
        }
        counts
    }

    pub fn within(&self, days: u32) -> u64 {
        match days {
            7 => self.days_7,
            90 => self.days_90,
            _ => self.days_30,
        }
    }
}

/// One harness's answer. `None` is "this harness cannot say".
#[derive(Clone, Debug)]
pub struct HarnessInventory {
    pub id: String,
    pub name: String,
    pub found: Option<Vec<FoundResource>>,
    pub sessions: Option<SessionCounts>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HarnessReport {
    pub id: String,
    pub name: String,
    /// Whether the harness could list what it carries.
    pub inventory_known: bool,
    pub sessions: Option<SessionCounts>,
}

/// Who manages a resource, as the Overview shows it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum Origin {
    /// A uze receipt owns it. `package` is the receipt's owner; uze's own
    /// machine setup is `setup`.
    Uze {
        package: String,
    },
    Plugin {
        name: String,
    },
    BuiltIn,
    You,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Location {
    pub harness: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status {
    /// A fact on disk proves it cannot work; `reason` is one line.
    Broken {
        reason: String,
    },
    /// The same kind and name elsewhere with different content.
    Duplicate {
        others: Vec<Location>,
    },
    Ok,
}

impl Status {
    /// Broken first, then duplicates, then the rest.
    pub fn rank(&self) -> u8 {
        match self {
            Self::Broken { .. } => 0,
            Self::Duplicate { .. } => 1,
            Self::Ok => 2,
        }
    }
}

/// One row of the Overview.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OverviewResource {
    /// Stable across reinstalls: never a path or a version.
    pub id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub origin: Origin,
    /// Where each harness reads it, one per harness.
    pub locations: Vec<Location>,
    pub description: Option<String>,
    /// Estimated tokens added to every session; `None` when not measured.
    pub per_session: Option<u32>,
    /// Estimated tokens loaded when used; `None` when it adds nothing more.
    pub when_used: Option<u32>,
    pub status: Status,
    /// Whether a fact on disk was checked at all: a definition read, or a
    /// command looked for. A server reached by URL is listed, never judged,
    /// so its `ok` says nothing was found against it rather than that it
    /// works.
    #[serde(default)]
    pub checked: bool,
    pub removal: Option<String>,
}

impl OverviewResource {
    pub fn reaches(&self, harness: &str) -> bool {
        self.locations
            .iter()
            .any(|location| location.harness == harness)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MachineOverview {
    pub harnesses: Vec<HarnessReport>,
    pub resources: Vec<OverviewResource>,
}
