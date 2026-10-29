//! What the workspace shows about one space when it is selected: the
//! project's status, its plugins, its memory and the work its agents left,
//! composed from the read models `uze status` and the plugin list already
//! answer with, so the screen and the command can never disagree.

use std::path::{Path, PathBuf};

use uze_core::{Result, UzeError};
use uze_workspace::worktree::isolated_checkout;

use super::{
    CommandsAwaitingApproval, DeclarationView, PreservedWork, StepNotSpelledHere, Workspace,
};
use crate::application::{
    EnvironmentDrift, Freshness, PluginSummary, ProjectLockStatus, ProjectStatus,
    memory::{Entry, MemoryReport, MemoryScope, UnreadableFact},
};

/// The overview of the directory an agent works in. A directory that is
/// gone is an answer the screen shows, never a failure; one outside every
/// project still has its memory and its settings.
#[derive(Clone, Debug)]
pub enum SpaceOverview {
    DirectoryGone { directory: PathBuf },
    Project(Box<ProjectSpace>),
}

/// What the overview reads of a directory, each part on its own: only the
/// parts `uze status` answers need a project around it.
#[derive(Clone, Debug)]
pub struct ProjectSpace {
    /// The directory read: the project's root, or the directory itself
    /// outside every project.
    pub root: PathBuf,
    /// Exactly what `uze status` answers here, or `None` outside every
    /// project, where it has nothing to answer.
    pub status: Option<ProjectStatus>,
    pub plugins: SpacePlugins,
    pub memory: SpaceMemory,
    /// The `workspace:` declaration key by key, or why `agents.yaml` could
    /// not be read for it.
    pub workspace: std::result::Result<DeclarationView, String>,
    /// Every agent recorded for this project, oldest first. Which of them
    /// a live agent is still in front of is the client's to subtract.
    pub preserved: Vec<PreservedWork>,
}

impl ProjectSpace {
    /// The commands waiting for the operator's approval: none outside a
    /// project, which declares none.
    pub fn commands_awaiting_approval(&self) -> Option<&CommandsAwaitingApproval> {
        self.status.as_ref()?.commands_awaiting_approval.as_ref()
    }

    /// The declared steps with no spelling for this platform.
    pub fn steps_not_spelled_here(&self) -> &[StepNotSpelledHere] {
        self.status
            .as_ref()
            .map_or(&[], |status| status.steps_not_spelled_here.as_slice())
    }
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

/// The files whose change the overview of a project shows: what it declares,
/// what it locked and what it tells every agent.
const DECLARING_FILES: [&str; 3] = ["agents.yaml", "agents.lock", "AGENTS.md"];

impl Workspace<'_> {
    /// A stamp of what `root` declares, read from the file system alone
    /// (each of [`DECLARING_FILES`]' size and modification time, or its
    /// absence), so a client can poll it cheaply and read the overview
    /// again only when it moved. At `trace`: it is asked every second
    /// while an overview is open.
    #[tracing::instrument(name = "workspace.declaration_stamp", level = "trace", skip_all, fields(root = %root.display()))]
    pub fn declaration_stamp(&self, root: &Path) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for name in DECLARING_FILES {
            let metadata = std::fs::metadata(root.join(name)).ok();
            metadata
                .as_ref()
                .map(std::fs::Metadata::len)
                .hash(&mut hasher);
            metadata
                .and_then(|metadata| metadata.modified().ok())
                .hash(&mut hasher);
        }
        hasher.finish()
    }

    /// The overview of the project at `root`. Reads only what
    /// `uze status` reads, plus the installed plugins' index and UZE's
    /// own agent records, and asks Git only whether each declared link is
    /// ignored: no network.
    #[tracing::instrument(name = "workspace.space_overview", skip_all, fields(root = %root.display()), err)]
    pub fn space_overview(&self, root: &Path) -> Result<SpaceOverview> {
        if !root.is_dir() {
            return Ok(SpaceOverview::DirectoryGone {
                directory: root.to_path_buf(),
            });
        }
        let status = match self.0.health().project_status(root) {
            Ok(status) => Some(status),
            Err(UzeError::NoProject { .. }) => None,
            Err(error) => return Err(error),
        };
        // Outside a project the directory itself is what its settings
        // declare and its harnesses remember: writing a setting there is
        // what makes it one.
        let root = status
            .as_ref()
            .map_or_else(|| root.to_path_buf(), |status| status.report.root.clone());
        let plugins = match &status {
            Some(status) => space_plugins(status, &self.0.plugins().list()?),
            None => SpacePlugins::default(),
        };
        let memory = match &status {
            Some(status) => space_memory(&status.report.memory),
            None => space_memory(&self.0.memory().report_at(&root)),
        };
        let primary = isolated_checkout(&root)
            .map_or_else(|| root.clone(), |checkout| checkout.primary.to_path_buf());
        let preserved = self
            .preserved_work()
            .into_iter()
            .filter(|work| work.project == primary)
            .collect();
        Ok(SpaceOverview::Project(Box::new(ProjectSpace {
            plugins,
            memory,
            workspace: self
                .declaration_view(&root)
                .map_err(|error| error.to_string()),
            preserved,
            status,
            root,
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

fn space_memory(memory: &MemoryReport) -> SpaceMemory {
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
