//! Where a project begins, seen from any directory inside it.
//!
//! A directory is a project only on evidence, and may be none. The nearest
//! ancestor declaring `agents.yaml` is the project; without one, the
//! nearest repository root is — the first `.git` met ends the walk, so a
//! repository never inherits a manifest or `AGENTS.md` from a directory
//! above it that happens to be a parent on this machine (a dotfiles
//! repository in `$HOME`, a checkout under another checkout), and an
//! `AGENTS.md` inside a repository can no longer shadow the repository
//! root. Only when there is no manifest and no repository is the nearest
//! `AGENTS.md` the project. Standing in a directory proves nothing:
//! outside every marker the answer is absence, and the caller decides what
//! runs machine-only and what refuses.

use crate::path::Canonical as _;
use std::path::{Path, PathBuf};

use crate::{Result, UzeError, manifest::MANIFEST_FILE_NAME, project_context::AGENTS_MD_FILE_NAME};

/// The file or directory marking a Git repository's root.
const GIT_MARKER: &str = ".git";

pub fn resolve_project_root(cwd: &Path) -> Result<Option<PathBuf>> {
    resolve_project_root_trusting(cwd, owned_here)
}

/// Whether this user, or the administrator, owns `marker`: what makes a
/// file or directory outside every repository one to act on.
pub fn owned_here(marker: &Path) -> bool {
    uze_platform::fs::owned_by_user_or_administrator(marker).unwrap_or(false)
}

/// Whether a marker found walking up may be adopted: one inside a
/// repository always, one outside every repository only when `owned` says
/// this user (or the administrator) wrote it.
pub(crate) fn adoptable(marker: &Path, owned: impl Fn(&Path) -> bool) -> bool {
    let directory = marker.parent().unwrap_or(marker);
    directory.ancestors().any(is_repository_root) || owned(marker)
}

/// [`resolve_project_root`], with who may have written a marker found
/// outside every repository decided by `owned`.
///
/// Outside a repository nothing but the file's owner says whose it is: an
/// `agents.yaml` in `/tmp` or a shared parent directory, written by another
/// account, would otherwise make that account's declarations the project of
/// whoever runs UZE below it. Inside a repository the clone is the operator's
/// own act, and the trust question each declared plugin raises is asked
/// there instead.
fn resolve_project_root_trusting(
    cwd: &Path,
    owned: impl Fn(&Path) -> bool,
) -> Result<Option<PathBuf>> {
    if !cwd.exists() {
        return Err(UzeError::MissingPath(cwd.to_path_buf()));
    }
    let adoptable = |marker: &Path| adoptable(marker, &owned);
    let mut nearest_agents_md = None;
    let (_, root) = find_upward(cwd, |dir| {
        let manifest = dir.join(MANIFEST_FILE_NAME);
        if manifest.is_file() && adoptable(&manifest) {
            return Some(dir.to_path_buf());
        }
        let agents_md = dir.join(AGENTS_MD_FILE_NAME);
        if nearest_agents_md.is_none() && agents_md.is_file() && adoptable(&agents_md) {
            nearest_agents_md = Some(dir.to_path_buf());
        }
        // A repository root outranks an `AGENTS.md` remembered below it:
        // the walk stops here, so the remembered one is discarded rather
        // than shadowing the repository.
        is_repository_root(dir).then(|| dir.to_path_buf())
    })?;
    Ok(root.or(nearest_agents_md))
}

/// Walks from the directory `path` names — its parent when `path` is a file
/// — up through every ancestor, nearest first, and returns the canonical
/// starting directory with the first answer `found` gives.
pub(crate) fn find_upward<T>(
    path: &Path,
    found: impl FnMut(&Path) -> Option<T>,
) -> Result<(PathBuf, Option<T>)> {
    let directory = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let start = directory.canonical().map_err(UzeError::read(path))?;
    let answer = start.ancestors().find_map(found);
    Ok((start, answer))
}

pub(crate) fn is_repository_root(directory: &Path) -> bool {
    directory.join(GIT_MARKER).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cwd_with_a_manifest_is_root() {
        let root = uze_testkit::temp::scratch("manifest-root");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(MANIFEST_FILE_NAME), "workspace: {}\n").unwrap();
        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();
        // cwd is sub, which declares nothing; the walk finds the parent's
        // manifest, and the parent is the project root
        let resolved = resolve_project_root(&sub).unwrap();
        assert_eq!(resolved, Some(root.canonical().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_markers_is_no_project() {
        let root = uze_testkit::temp::scratch("no-project");
        fs::create_dir_all(&root).unwrap();
        let resolved = resolve_project_root(&root).unwrap();
        assert!(
            resolved.is_none(),
            "standing in a directory proves nothing: {resolved:?}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_repository_never_inherits_an_agents_md_from_above_it() {
        // A repo with no portable context of its own resolves to itself,
        // even when an ancestor directory carries an AGENTS.md — otherwise
        // the same project resolves differently depending on where it was
        // cloned.
        let outer = uze_testkit::temp::scratch("git-boundary");
        let repo = outer.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(outer.join(AGENTS_MD_FILE_NAME), "# not this one\n").unwrap();
        let sub = repo.join("src");
        fs::create_dir_all(&sub).unwrap();
        let resolved = resolve_project_root(&sub).unwrap();
        assert_eq!(resolved, Some(repo.canonical().unwrap()));
        fs::remove_dir_all(outer).unwrap();
    }

    #[test]
    fn agents_md_inside_a_repository_does_not_shadow_the_root() {
        // The case nothing tested before: an `AGENTS.md` in a subdirectory
        // of a repository loses to the repository root — `repo/docs` is not
        // the project `repo` is.
        let root = uze_testkit::temp::scratch("agents-in-repo");
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(docs.join(AGENTS_MD_FILE_NAME), "# not this one\n").unwrap();
        let resolved = resolve_project_root(&docs).unwrap();
        assert_eq!(resolved, Some(root.canonical().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_worktree_counts_as_a_repository_root() {
        // A worktree carries `.git` as a file, not a directory; the marker
        // test is about existence, and it still outranks a nested
        // `AGENTS.md`.
        let root = uze_testkit::temp::scratch("worktree-marker");
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(root.join(GIT_MARKER), "gitdir: elsewhere\n").unwrap();
        fs::write(docs.join(AGENTS_MD_FILE_NAME), "# not this one\n").unwrap();
        let resolved = resolve_project_root(&docs).unwrap();
        assert_eq!(resolved, Some(root.canonical().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prefers_agents_md_over_git() {
        // Same repository root, no repo above it in play: the AGENTS.md
        // directory *is* the repository root here — the precedence question
        // is only about which marker wins when they sit on different
        // directories, which the two tests above settle.
        let root = uze_testkit::temp::scratch("agents-vs-git");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(AGENTS_MD_FILE_NAME), "# hi\n").unwrap();
        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let resolved = resolve_project_root(&sub).unwrap();
        assert_eq!(resolved, Some(root.canonical().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    /// Outside every repository, a marker another account wrote is not a
    /// project: the walk passes over it as though it were not there.
    #[test]
    fn a_marker_another_account_wrote_outside_a_repository_is_not_a_project() {
        let outer = uze_testkit::temp::scratch("foreign-manifest");
        let sub = outer.join("shared").join("mine");
        fs::create_dir_all(&sub).unwrap();
        let foreign_manifest = outer.join("shared").join(MANIFEST_FILE_NAME);
        fs::write(&foreign_manifest, "workspace: {}\n").unwrap();
        fs::write(outer.join("shared").join(AGENTS_MD_FILE_NAME), "# theirs\n").unwrap();
        let shared = outer.join("shared").canonical().unwrap();
        let foreign = |marker: &Path| marker.parent() != Some(shared.as_path());

        let resolved = resolve_project_root_trusting(&sub, foreign).unwrap();
        assert_eq!(resolved, None, "another account's markers adopted");
        assert_eq!(
            resolve_project_root_trusting(&sub, |_| true).unwrap(),
            Some(outer.join("shared").canonical().unwrap())
        );

        // Inside a repository the same file is the clone's, and adopted.
        fs::create_dir_all(outer.join(".git")).unwrap();
        assert_eq!(
            resolve_project_root_trusting(&sub, foreign).unwrap(),
            Some(outer.join("shared").canonical().unwrap())
        );
        fs::remove_dir_all(outer).unwrap();
    }

    #[test]
    fn a_repository_never_inherits_a_manifest_from_above_it() {
        let outer = uze_testkit::temp::scratch("git-boundary-manifest");
        let repo = outer.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(outer.join(MANIFEST_FILE_NAME), "workspace: {}\n").unwrap();
        let sub = repo.join("src");
        fs::create_dir_all(&sub).unwrap();
        let resolved = resolve_project_root(&sub).unwrap();
        assert_eq!(resolved, Some(repo.canonical().unwrap()));
        fs::remove_dir_all(outer).unwrap();
    }
}
