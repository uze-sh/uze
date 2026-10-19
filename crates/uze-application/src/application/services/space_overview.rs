//! What the workspace shows about one space when it is selected: the
//! project's status, its plugins, its memory and the work its agents left,
//! composed from the read models `uze status` and the plugin list already
//! answer with, so the screen and the command can never disagree.

use std::path::{Path, PathBuf};

use uze_core::{Result, UzeError};
use uze_workspace::worktree::isolated_checkout;

use super::{DeclarationView, PreservedWork, Workspace};
use crate::application::{
    EnvironmentDrift, Freshness, PluginSummary, ProjectLockStatus, ProjectStatus,
    memory::{Entry, MemoryScope, UnreadableFact},
};

/// One space's overview. A root that is gone and a directory that is no
/// project are answers the screen shows, never failures.
#[derive(Clone, Debug)]
pub enum SpaceOverview {
    RootGone { root: PathBuf },
    NotAProject { root: PathBuf },
    Project(Box<ProjectSpace>),
}

/// A project's overview.
#[derive(Clone, Debug)]
pub struct ProjectSpace {
    /// Exactly what `uze status` answers here.
    pub status: ProjectStatus,
    pub plugins: SpacePlugins,
    pub memory: SpaceMemory,
    /// The `workspace:` declaration key by key, or why `agents.yaml` could
    /// not be read for it.
    pub workspace: std::result::Result<DeclarationView, String>,
    /// Every agent recorded for this project, oldest first. Which of them
    /// a live agent is still in front of is the client's to subtract.
    pub preserved: Vec<PreservedWork>,
}

#[derive(Clone, Debug, Default)]
pub struct SpacePlugins {
    /// Why `agents.lock` could not be read, when it could not.
    pub lock_problem: Option<String>,
    /// The lock's plugins, then any other installed plugin whose
    /// instructions this project's `AGENTS.md` carries: a contribution is
    /// only ever made from the Store, so those are installed.
    pub plugins: Vec<SpacePlugin>,
    /// What `agents.yaml` asks for that the lock, the Store or the
    /// projection has not caught up to.
    pub drift: EnvironmentDrift,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpacePlugin {
    pub plugin: String,
    /// The installed package it answers to, `<plugin>@<marketplace>`.
    pub package: String,
    /// Recorded in `agents.lock`.
    pub locked: bool,
    pub installed: bool,
    /// Installed at a revision other than the lock's.
    pub differs_from_lock: bool,
    /// The commit installed, when it came from one.
    pub commit: Option<String>,
    /// `None` when it is not installed.
    pub freshness: Option<Freshness>,
}

#[derive(Clone, Debug, Default)]
pub struct SpaceMemory {
    /// The project's facts and its harnesses' entries, as an index sees them.
    pub project: Vec<Entry>,
    /// The facts the machine keeps for every project, which reach this
    /// project's agents as much as its own do.
    pub machine: Vec<Entry>,
    /// Machine facts agents proposed, waiting for the operator.
    pub proposals: usize,
    pub unreadable: Vec<UnreadableFact>,
}

impl Workspace<'_> {
    /// The overview of the space rooted at `root`. Reads only what
    /// `uze status` reads, plus the installed plugins' index and UZE's
    /// own agent records, and asks Git only whether each declared link is
    /// ignored: no network.
    #[tracing::instrument(name = "workspace.space_overview", skip_all, fields(root = %root.display()), err)]
    pub fn space_overview(&self, root: &Path) -> Result<SpaceOverview> {
        if !root.is_dir() {
            return Ok(SpaceOverview::RootGone {
                root: root.to_path_buf(),
            });
        }
        let status = match self.0.health().project_status(root) {
            Ok(status) => status,
            Err(UzeError::NoProject { .. }) => {
                return Ok(SpaceOverview::NotAProject {
                    root: root.to_path_buf(),
                });
            }
            Err(error) => return Err(error),
        };
        let installed = self.0.plugins().list()?;
        let primary = isolated_checkout(&status.report.root).map_or_else(
            || status.report.root.clone(),
            |checkout| checkout.primary.to_path_buf(),
        );
        let preserved = self
            .preserved_work()
            .into_iter()
            .filter(|work| work.project == primary)
            .collect();
        Ok(SpaceOverview::Project(Box::new(ProjectSpace {
            plugins: space_plugins(&status, &installed),
            memory: space_memory(&status),
            workspace: self
                .declaration_view(&status.report.root)
                .map_err(|error| error.to_string()),
            preserved,
            status,
        })))
    }
}

fn space_plugins(status: &ProjectStatus, installed: &[PluginSummary]) -> SpacePlugins {
    let report = &status.report;
    let summary = |package: &str| installed.iter().find(|summary| summary.id == package);
    let (lock_problem, locked) = match &report.project_lock {
        ProjectLockStatus::Absent => (None, Vec::new()),
        ProjectLockStatus::Malformed { reason } => (Some(reason.clone()), Vec::new()),
        ProjectLockStatus::Present { plugins } => (None, plugins.clone()),
    };
    let plugins: Vec<SpacePlugin> = locked
        .into_iter()
        .map(|health| {
            let summary = summary(&health.package);
            SpacePlugin {
                commit: summary.and_then(|summary| summary.commit.clone()),
                freshness: summary.map(|summary| summary.freshness.clone()),
                plugin: health.plugin,
                package: health.package,
                locked: true,
                installed: health.installed,
                differs_from_lock: health.differs_from_lock,
            }
        })
        .collect();
    SpacePlugins {
        lock_problem,
        plugins,
        drift: report.drift.clone(),
    }
}

fn space_memory(status: &ProjectStatus) -> SpaceMemory {
    let memory = &status.report.memory;
    SpaceMemory {
        project: memory
            .entries
            .iter()
            .filter(|entry| entry.scope == MemoryScope::Project)
            .cloned()
            .collect(),
        machine: memory
            .entries
            .iter()
            .filter(|entry| entry.scope == MemoryScope::Machine)
            .cloned()
            .collect(),
        proposals: memory.proposals.len(),
        unreadable: memory.unreadable.clone(),
    }
}
