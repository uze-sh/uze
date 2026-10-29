//! Launch overrides as presentation sees them: what a harness's launch
//! takes, a typed line checked against it, and the sets used lately.
//!
//! The catalog is static data the integration declares, so checking a line
//! against it asks nothing of the machine: an overrides step checks on each
//! keystroke without a thread. The sets used lately are a read of a record,
//! and are asked for like any other.

use std::path::Path;

use uze_core::{
    Result,
    flag_catalog::{FlagCatalog, FlagOwner, FlagSpec, FlagValue},
    integration::IntegrationPort,
};
use uze_workspace::overrides::{self, Entry, Unterminated, Verdict, record};

use super::Workspace;

/// What one harness's launch takes, as an overrides step offers and checks
/// it. Copied with the harness's identity; holds only what its integration
/// declares.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverrideCatalog {
    flags: FlagCatalog,
    managed: &'static [&'static str],
}

impl OverrideCatalog {
    pub(crate) fn of(integration: &dyn IntegrationPort) -> Self {
        Self {
            flags: integration.flag_catalog(),
            managed: integration.managed_environment(),
        }
    }

    /// The flags a step may offer, in the catalog's order: every one but
    /// those UZE composes for the launch itself.
    pub fn flags(&self) -> impl Iterator<Item = OverrideFlag> + '_ {
        self.flags
            .iter()
            .filter(|spec| spec.is_offered())
            .map(OverrideFlag::from)
    }

    /// The offered flag spelled `spelling`, in any of its spellings.
    pub fn flag(&self, spelling: &str) -> Option<OverrideFlag> {
        self.flags().find(|flag| flag.spellings.contains(&spelling))
    }

    /// `line` as typed, read as entries and judged one by one, without the
    /// launch's plan: a key UZE sets only on some launches is a warning
    /// here, decided at the launch. An open quote is the line still being
    /// typed.
    pub fn check(&self, line: &str) -> std::result::Result<Vec<CheckedOverride>, Unterminated> {
        Ok(self.check_words(&overrides::split(line)?))
    }

    /// [`Self::check`] for words already split: a set used before, or the
    /// entries a step holds.
    pub fn check_words(&self, words: &[String]) -> Vec<CheckedOverride> {
        let entries = overrides::entries(words, self.flags);
        let verdicts = overrides::check(&entries, self.flags, self.managed, None);
        entries
            .into_iter()
            .zip(verdicts)
            .map(|(entry, verdict)| CheckedOverride {
                words: entry.words(),
                label: entry.label().to_owned(),
                kind: OverrideKind::of(&entry),
                verdict,
            })
            .collect()
    }
}

/// One flag a harness's launch takes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverrideFlag {
    /// The spelling shown: the long one.
    pub name: &'static str,
    /// Every spelling the harness accepts, long first.
    pub spellings: &'static [&'static str],
    pub value: OverrideValue,
    /// Whether the harness takes it more than once in one launch.
    pub repeatable: bool,
    /// A short description, lowercase.
    pub summary: &'static str,
    pub mark: OverrideMark,
}

impl From<&FlagSpec> for OverrideFlag {
    fn from(spec: &FlagSpec) -> Self {
        Self {
            name: spec.name(),
            spellings: spec.spellings,
            value: match spec.value {
                // Offered as a switch: a value it may take goes attached,
                // which the summary names.
                FlagValue::None | FlagValue::Optional(_) => OverrideValue::Switch,
                FlagValue::Free(hint) => OverrideValue::Free(hint),
                FlagValue::OneOf(values) => OverrideValue::OneOf(values),
            },
            repeatable: spec.repeatable,
            summary: spec.summary,
            mark: match spec.owner {
                FlagOwner::Weakens(what) => OverrideMark::Weakens(what),
                FlagOwner::Degrades(what) => OverrideMark::Degrades(what),
                FlagOwner::Operator | FlagOwner::Uze(_) | FlagOwner::Keyed { .. } => {
                    OverrideMark::Plain
                }
            },
        }
    }
}

/// What follows a flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverrideValue {
    /// Nothing: the flag is a switch.
    Switch,
    /// Any value; the hint names what it is (`model`, `dir`).
    Free(&'static str),
    /// One of these, which the step offers after the flag and a space.
    OneOf(&'static [&'static str]),
}

/// What a flag is marked with wherever it is shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverrideMark {
    Plain,
    /// It weakens the harness's own safety, giving up what it names.
    Weakens(&'static str),
    /// It stops something UZE delivers from loading, naming what.
    Degrades(&'static str),
}

/// One entry of a checked line, and what the launch does with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedOverride {
    /// The words the entry is, as typed: what reaches the harness.
    pub words: Vec<String>,
    /// What may be said of it: a variable's name or a flag's spelling,
    /// never a value.
    pub label: String,
    pub kind: OverrideKind,
    pub verdict: Verdict,
}

/// What an entry of a typed line is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverrideKind {
    /// `NAME=value`, set in the harness's environment.
    Variable,
    /// A flag, with the value it took.
    Flag,
    /// A word no flag takes, which the harness would read as a prompt.
    Positional,
    /// A literal `--`.
    EndOfOptions,
}

impl OverrideKind {
    fn of(entry: &Entry) -> Self {
        match entry {
            Entry::Variable { .. } => Self::Variable,
            Entry::Flag { .. } => Self::Flag,
            Entry::Positional(_) => Self::Positional,
            Entry::EndOfOptions => Self::EndOfOptions,
        }
    }
}

impl Workspace<'_> {
    /// What `harness`'s launch takes, or `None` for a harness UZE does not
    /// know.
    #[tracing::instrument(name = "workspace.override_catalog", skip_all, fields(harness))]
    pub fn override_catalog(&self, harness: &str) -> Option<OverrideCatalog> {
        self.integration(harness).map(OverrideCatalog::of)
    }

    /// The sets of overrides used lately on `harness`: those last used in
    /// `project_root` first, then the others, each most recent first. A
    /// read of a record: off the thread that draws.
    #[tracing::instrument(name = "workspace.recent_overrides", skip_all, fields(harness, project_root = %project_root.display()))]
    pub fn recent_overrides(&self, harness: &str, project_root: &Path) -> Vec<record::RecentSet> {
        record::recent(&self.0.home, harness, project_root)
    }

    /// Forgets the set `words` on `harness`, in every project, and says
    /// whether there was one.
    #[tracing::instrument(
        name = "workspace.forget_recent_overrides",
        skip_all,
        fields(harness),
        err
    )]
    pub fn forget_recent_overrides(&self, harness: &str, words: &[String]) -> Result<bool> {
        record::forget_recent(&self.0.home, harness, words)
    }

    /// The overrides `agent` of `project_root` was last launched with on
    /// `harness`, as words; none when it has none.
    #[tracing::instrument(name = "workspace.agent_overrides", skip_all, fields(agent, harness))]
    pub fn agent_overrides(&self, project_root: &Path, agent: &str, harness: &str) -> Vec<String> {
        record::recorded(
            &self.0.home,
            project_root,
            &uze_workspace::task::AgentId::adopted(agent),
            harness,
        )
    }
}
