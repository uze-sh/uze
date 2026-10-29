//! What the workspace shows about one space when it is selected: the
//! project's status, its plugins, its memory and the work its agents left,
//! composed from the read models `uze status` and the plugin list already
//! answer with, so the screen and the command can never disagree.

use std::path::{Path, PathBuf};

use uze_core::{Result, project_root::repository_root_of};
use uze_workspace::worktree::isolated_checkout;

use super::{DeclarationView, PreservedWork, Workspace};
use crate::application::{
    EnvironmentDrift, Freshness, PluginSummary, ProjectLockStatus, ProjectStatus,
    memory::{Entry, MemoryReport, MemoryScope, UnreadableFact},
};

/// The overview of the directory an agent works in. A directory that is
/// gone is an answer the screen shows, never a failure.
#[derive(Clone, Debug)]
pub enum SpaceOverview {
    DirectoryGone { directory: PathBuf },
    Directory(Box<DirectorySpace>),
}

/// What reaches an agent wherever it works: its memory, and the project
/// around it when it works inside a Git repository.
#[derive(Clone, Debug)]
pub struct DirectorySpace {
    /// The directory read: the project's root inside a repository, the
    /// agent's own directory outside every one.
    pub root: PathBuf,
    pub memory: SpaceMemory,
    /// `None` outside every Git repository, where nothing is declared,
    /// locked or checked out for the agent; why the project could not be
    /// read, when it could not.
    pub project: Option<std::result::Result<ProjectSpace, String>>,
}

/// The project a Git repository is, as `uze status` and the workspace see it.
#[derive(Clone, Debug)]
pub struct ProjectSpace {
    /// Exactly what `uze status` answers here.
    pub status: ProjectStatus,
    pub plugins: SpacePlugins,
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

    /// The overview of the directory at `root`. Reads only what
    /// `uze status` reads, plus the installed plugins' index and UZE's
    /// own agent records, and asks Git only whether each declared link is
    /// ignored: no network. Outside a Git repository it reads the memory
    /// alone.
    #[tracing::instrument(name = "workspace.space_overview", skip_all, fields(root = %root.display()), err)]
    pub fn space_overview(&self, root: &Path) -> Result<SpaceOverview> {
        if !root.is_dir() {
            return Ok(SpaceOverview::DirectoryGone {
                directory: root.to_path_buf(),
            });
        }
        if repository_root_of(root).is_none() {
            return Ok(SpaceOverview::Directory(Box::new(DirectorySpace {
                memory: space_memory(&self.0.memory().report_at(root)),
                root: root.to_path_buf(),
                project: None,
            })));
        }
        let space = match self.project_space(root) {
            Ok(project) => DirectorySpace {
                root: project.status.report.root.clone(),
                memory: space_memory(&project.status.report.memory),
                project: Some(Ok(project)),
            },
            Err(error) => DirectorySpace {
                memory: space_memory(&self.0.memory().report_at(root)),
                root: root.to_path_buf(),
                project: Some(Err(error.to_string())),
            },
        };
        Ok(SpaceOverview::Directory(Box::new(space)))
    }

    /// The project the repository at `root` is.
    fn project_space(&self, root: &Path) -> Result<ProjectSpace> {
        let status = self.0.health().project_status(root)?;
        let project_root = status.report.root.clone();
        let primary = isolated_checkout(&project_root).map_or_else(
            || project_root.clone(),
            |checkout| checkout.primary.to_path_buf(),
        );
        Ok(ProjectSpace {
            plugins: space_plugins(&status, &self.0.plugins().list()?),
            workspace: self
                .declaration_view(&project_root)
                .map_err(|error| error.to_string()),
            preserved: self
                .preserved_work()
                .into_iter()
                .filter(|work| work.project == primary)
                .collect(),
            status,
        })
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
