//! What portable project context exists here, and where its root is.
//!
//! The root is `project_root::resolve_project_root`'s answer — the rule
//! every project-scoped command resolves with — and the two portable
//! context resources, `AGENTS.md` and `.agents/`, are observed
//! independently at that root. Neither gates the other: a project with only
//! `.agents/skills/` still has context to deliver.

use crate::path::Canonical as _;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use crate::{Result, UzeError, harness_runtime::project_id_for, home::UzeHome};

/// The portable project-context resources, as they exist on disk right
/// now. Purely observational: resolving never creates or moves anything.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectContext {
    /// The resolved project root — `resolve_project_root`'s answer, which
    /// falls back to `cwd` itself when nothing marks a root.
    pub root: PathBuf,
    /// The shared instruction baseline, when the project has one.
    pub agents_md: Option<PathBuf>,
    /// The portable resource directory, when the project has one.
    pub agents_directory: Option<PathBuf>,
}

/// A resource kind `.agents/` carries. Named here rather than at each call
/// site so a harness projecting the directory and a status view describing
/// it can never drift apart on which subdirectories count. The kinds are
/// answered apart because harnesses read them apart: one that reads
/// `.agents/skills` may read no `.agents/agents`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AgentsDirectoryResource {
    Skills,
    Agents,
}

impl AgentsDirectoryResource {
    pub const ALL: [Self; 2] = [Self::Skills, Self::Agents];

    /// The subdirectory of `.agents/` holding this kind.
    pub fn directory_name(self) -> &'static str {
        match self {
            Self::Skills => "skills",
            Self::Agents => "agents",
        }
    }
}

pub const AGENTS_MD_FILE_NAME: &str = "AGENTS.md";
pub const AGENTS_DIRECTORY_NAME: &str = ".agents";

impl ProjectContext {
    /// Whether this project has anything portable to deliver at all. The
    /// activation condition for any delivery strategy: a project with only
    /// `.agents/skills/` and no `AGENTS.md` is still a project with
    /// context, and vice versa.
    pub fn has_any(&self) -> bool {
        self.agents_md.is_some() || self.agents_directory.is_some()
    }

    /// `.agents/<resource>/`, when the project carries that kind.
    pub fn resource_directory(&self, resource: AgentsDirectoryResource) -> Option<PathBuf> {
        self.agents_directory
            .as_ref()
            .map(|directory| directory.join(resource.directory_name()))
            .filter(|path| path.is_dir())
    }
}

/// Resolves the project context containing `cwd`. Infallible by design:
/// an unresolvable or nonexistent `cwd` yields a context rooted at `cwd`
/// with no resources, never an error — every caller (a status popup, a
/// shim about to exec a harness) needs an answer it can render or ignore,
/// not a failure mode.
pub fn resolve(cwd: &Path) -> ProjectContext {
    // Absence is an answer, not a failure: a directory that is no project
    // yields a context rooted at `cwd` with no resources, which every
    // caller can render or ignore.
    let root = crate::project_root::resolve_project_root(cwd)
        .ok()
        .flatten()
        .unwrap_or_else(|| cwd.canonical().unwrap_or_else(|_| cwd.to_path_buf()));
    let agents_md = root.join(AGENTS_MD_FILE_NAME);
    let agents_directory = root.join(AGENTS_DIRECTORY_NAME);
    ProjectContext {
        agents_md: agents_md.is_file().then_some(agents_md),
        agents_directory: agents_directory.is_dir().then_some(agents_directory),
        root,
    }
}

/// How long a writer waits for the other owner of `AGENTS.md` to finish.
/// A rewrite of one Markdown file takes milliseconds; a holder past this is
/// a stuck process, which is reported rather than waited on forever.
const AGENTS_MD_WAIT: Duration = Duration::from_secs(10);
const AGENTS_MD_RETRY: Duration = Duration::from_millis(10);

/// Held while a project's `AGENTS.md` is rewritten. The file has two owners,
/// the package manager (package and authoring regions, the bridges read from
/// it) and the workspace (its policy region), and a rewrite is a whole-file
/// read-modify-write: without this, two of them interleaving would each keep
/// its own region and drop the other's until the next run.
pub struct AgentsMdGuard {
    _file: File,
}

impl AgentsMdGuard {
    pub fn acquire(home: &UzeHome, project_root: &Path) -> Result<Self> {
        let path = home.agents_md_lock_path(&project_id_for(project_root));
        let parent = path.parent().expect("UZE state paths have a parent");
        fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
        // Readable or writable, not append-only: Windows refuses to lock a
        // handle that may only append.
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(UzeError::write(&path))?;
        let started = Instant::now();
        while let Err(error) = crate::persistence::try_lock_exclusive(&file) {
            if error.kind() != std::io::ErrorKind::WouldBlock {
                return Err(UzeError::Write {
                    path,
                    source: error,
                });
            }
            if started.elapsed() >= AGENTS_MD_WAIT {
                return Err(UzeError::MutationInProgress { path, pid: None });
            }
            thread::sleep(AGENTS_MD_RETRY);
        }
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two owners of one `AGENTS.md` never rewrite it at once: the second
    /// waits for the first to let go.
    #[test]
    fn a_second_owner_waits_for_the_first() {
        use std::sync::mpsc;

        let home = UzeHome::at(uze_testkit::temp::scratch("agents-md-guard-home"));
        let project = uze_testkit::temp::scratch("agents-md-guard-project");
        let first = AgentsMdGuard::acquire(&home, &project).unwrap();

        let (sender, receiver) = mpsc::channel();
        let (home_for, project_for) = (home.clone(), project.clone());
        let waiter = std::thread::spawn(move || {
            let _second = AgentsMdGuard::acquire(&home_for, &project_for).unwrap();
            sender.send(()).unwrap();
        });
        assert!(
            receiver.recv_timeout(Duration::from_millis(200)).is_err(),
            "the second owner took the file while the first held it"
        );
        drop(first);
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the second owner takes it once the first lets go");
        waiter.join().unwrap();
    }
    use std::fs;

    #[test]
    fn agents_directory_alone_is_still_project_context() {
        // The regression this module exists for: the runtime projection
        // used to abort before looking at `.agents/` whenever `AGENTS.md`
        // was absent, so a project delivering only Skills delivered
        // nothing and the UI reported the harness as "not supported".
        let root = uze_testkit::temp::scratch("agents-dir-only");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join(".agents/skills")).unwrap();
        let context = resolve(&root);
        assert!(context.agents_md.is_none());
        assert!(context.agents_directory.is_some());
        assert!(context.has_any());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_nested_directory_resolves_to_the_project_root_not_itself() {
        let root = uze_testkit::temp::scratch("nested");
        fs::write(root.join(AGENTS_MD_FILE_NAME), "x\n").unwrap();
        let nested = root.join("crates/deep");
        fs::create_dir_all(&nested).unwrap();
        let context = resolve(&nested);
        assert_eq!(context.root, root.canonical().unwrap());
        assert!(context.agents_md.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_git_repository_without_portable_context_has_none() {
        let root = uze_testkit::temp::scratch("bare-git");
        fs::create_dir_all(root.join(".git")).unwrap();
        let context = resolve(&root);
        assert_eq!(context.root, root.canonical().unwrap());
        assert!(!context.has_any());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_nonexistent_directory_resolves_to_itself_with_no_context() {
        let root = uze_testkit::temp::scratch("absent");
        let missing = root.join("nowhere");
        let context = resolve(&missing);
        assert!(!context.has_any());
        assert_eq!(context.root, missing);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_subdirectory_of_a_repository_resolves_to_the_repository() {
        let root = uze_testkit::temp::scratch("repo-subdir");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(AGENTS_MD_FILE_NAME), "x\n").unwrap();
        fs::create_dir_all(root.join(".agents/skills")).unwrap();
        let nested = root.join("crates/deep/src");
        fs::create_dir_all(&nested).unwrap();
        let context = resolve(&nested);
        assert_eq!(context.root, root.canonical().unwrap());
        assert!(context.agents_md.is_some());
        assert!(context.agents_directory.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_nested_repository_is_its_own_project_and_inherits_nothing() {
        // A checkout inside another checkout is a project boundary: it must
        // resolve to itself and see none of the outer repository's context,
        // otherwise the same vendored tree carries different instructions
        // depending on where it happens to be nested.
        let outer = uze_testkit::temp::scratch("nested-repo");
        fs::create_dir_all(outer.join(".git")).unwrap();
        fs::write(outer.join(AGENTS_MD_FILE_NAME), "# outer\n").unwrap();
        let inner = outer.join("vendor");
        fs::create_dir_all(inner.join(".git")).unwrap();
        let context = resolve(&inner);
        assert_eq!(context.root, inner.canonical().unwrap());
        assert!(context.agents_md.is_none());
        assert!(!context.has_any());
        let _ = fs::remove_dir_all(&outer);
    }
}
