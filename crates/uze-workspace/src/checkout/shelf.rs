//! An agent's uncommitted work, kept in Git rather than in its checkout.
//!
//! A shelf is a commit under [`SHELF_REFS`]`<task>`, written with plumbing
//! from a copy of the checkout's index: its tree is everything the
//! repository would report as a change, its first parent the checkout's
//! `HEAD`, and its second parent the task's earlier shelf when there was
//! one, so no shelf is ever dropped by the next. Nothing about it touches
//! the task's branch, the checkout's own index or the repository's stash,
//! which every worktree and the operator share.
//!
//! Once the shelf exists the checkout holds nothing of anybody's, and goes
//! back to the pool; that is what keeps the number of checkouts at the
//! number of agents working at once.

use std::{collections::HashMap, fs};

use super::*;
use crate::worktree::{SHELF_REFS, SHELF_TRAILER_BRANCH, SHELF_TRAILER_LABEL, SHELF_TRAILER_TASK};

/// The ref a task's shelf lives under.
pub fn shelf_ref(task: &str) -> String {
    format!("{SHELF_REFS}{task}")
}

/// What shelving a checkout produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Shelving {
    /// Nothing in the checkout is anybody's work.
    Nothing,
    /// The work is in this commit, under the task's shelf ref.
    Kept(String),
}

/// Why a checkout's work could not be shelved: the checkout is pinned with
/// it, and the operator is told.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Unshelvable {
    /// Work a shelf would keep only the commit id of, with the objects
    /// behind it left in the checkout.
    Uncapturable(String),
    /// Git refused, or the shelf could not be written.
    Failed(String),
}

impl fmt::Display for Unshelvable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uncapturable(what) => write!(formatter, "{what} cannot be kept on a shelf"),
            Self::Failed(reason) => write!(formatter, "its work could not be shelved: {reason}"),
        }
    }
}

/// A shelf as the repository holds it, described by its own commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Shelf {
    pub task: String,
    pub commit: String,
    pub label: String,
    pub branch: String,
}

/// Records the work in `slot` as `task`'s shelf. `Nothing` when the tree is
/// what `HEAD` has and `HEAD` is on a branch: content UZE derives does not
/// count, ignored files are never taken.
pub fn shelve(slot: &Path, task: &str, label: &str, branch: &str) -> Result<Shelving, Unshelvable> {
    if let Some(what) = uncapturable_work(slot) {
        return Err(Unshelvable::Uncapturable(what));
    }
    let failed = |reason: String| Unshelvable::Failed(reason);
    let tree = work_tree(slot)?;
    let head = tip_of(slot, "HEAD");
    if head.is_empty() {
        return Err(failed("the checkout has no commit".into()));
    }
    let head_tree = tip_of(slot, "HEAD^{tree}");
    if tree == head_tree && !holds_unbranched_commits(slot) {
        return Ok(Shelving::Nothing);
    }
    let reference = shelf_ref(task);
    let earlier = tip_of(slot, &reference);
    let message = format!(
        "unfinished: {label}\n\n{SHELF_TRAILER_TASK}: {task}\n{SHELF_TRAILER_BRANCH}: {branch}\n{SHELF_TRAILER_LABEL}: {label}\n"
    );
    let mut commit_tree = vec![
        "commit-tree",
        "--no-gpg-sign",
        tree.as_str(),
        "-p",
        head.as_str(),
    ];
    if !earlier.is_empty() {
        commit_tree.extend(["-p", earlier.as_str()]);
    }
    commit_tree.extend(["-m", message.as_str()]);
    let commit = plumbing(slot, &commit_tree, &[]).map_err(failed)?;
    swap_ref(slot, &reference, &commit, &earlier).map_err(failed)?;
    Ok(Shelving::Kept(commit))
}

/// The tree of everything in `slot` that is somebody's work: what its
/// index and working tree hold over `HEAD`, untracked files the repository
/// does not ignore included, content UZE derives left as `HEAD` has it.
pub fn tree_of_work(slot: &Path) -> Result<String, String> {
    work_tree(slot).map_err(|refusal| refusal.to_string())
}

fn work_tree(slot: &Path) -> Result<String, Unshelvable> {
    let failed = |reason: String| Unshelvable::Failed(reason);
    let git_dir =
        git_dir(slot).ok_or_else(|| failed("its Git directory could not be read".to_owned()))?;
    let index = TemporaryIndex::copied_from(&git_dir).map_err(failed)?;
    let env = [("GIT_INDEX_FILE", index.as_str())];
    plumbing(slot, &["add", "--all", "--", ":/"], &env).map_err(failed)?;
    for derived in derived_paths(slot) {
        plumbing(slot, &["reset", "--quiet", "HEAD", "--", derived], &env).map_err(failed)?;
    }
    if let Some(nested) = nested_repository(slot, &env).map_err(failed)? {
        return Err(Unshelvable::Uncapturable(format!("repository `{nested}`")));
    }
    plumbing(slot, &["write-tree"], &env).map_err(failed)
}

/// A repository `add` swept in as a bare commit id: a gitlink the index
/// holds and `HEAD` does not. Read from what was added rather than from
/// `status`, which names only the outermost untracked directory and so
/// never sees a repository nested further down inside it.
fn nested_repository(slot: &Path, env: &[(&str, &str)]) -> Result<Option<String>, String> {
    let listing = plumbing(
        slot,
        &["diff-index", "--cached", "-z", "--no-renames", "HEAD"],
        env,
    )?;
    let mut fields = listing.split('\0');
    while let (Some(meta), Some(path)) = (fields.next(), fields.next()) {
        let mut modes = meta.trim_start_matches(':').split(' ');
        let (old, new) = (modes.next(), modes.next());
        if new == Some(GITLINK_MODE) && old != Some(GITLINK_MODE) {
            return Ok(Some(path.to_owned()));
        }
    }
    Ok(None)
}

const GITLINK_MODE: &str = "160000";

/// What putting a shelf back produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Restoring {
    Restored,
    /// The shelf does not apply where the branch stands now; nothing in
    /// the checkout changed. The paths both sides changed.
    Conflict(Vec<PathBuf>),
}

/// Puts `shelf` back in `slot` as uncommitted changes, all unstaged, on
/// whatever is checked out there. When the checkout descends from nothing
/// the shelf lacks, the shelf's tree is restored whole; otherwise what the
/// shelf changed since the two parted is applied as a patch, all or
/// nothing.
pub fn restore(slot: &Path, shelf: &str) -> Result<Restoring, String> {
    let head = tip_of(slot, "HEAD");
    let base = read(slot, &["merge-base", "--end-of-options", &head, shelf])?;
    if base == head {
        write(
            slot,
            &[
                "restore",
                &format!("--source={shelf}"),
                "--worktree",
                "--",
                ":/",
            ],
        )?;
        return Ok(Restoring::Restored);
    }
    let patch = read_untrimmed(
        slot,
        &[
            "diff-tree",
            "-p",
            "--binary",
            "--full-index",
            "--no-renames",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            &base,
            shelf,
        ],
    )?;
    if patch.trim().is_empty() {
        return Ok(Restoring::Restored);
    }
    let applied = crate::git::write_with_stdin(slot, &["apply", "--whitespace=nowarn"], &patch)?;
    if applied.is_success() {
        return Ok(Restoring::Restored);
    }
    let theirs = changed_paths(slot, &base, &head)?;
    let conflicting = changed_paths(slot, &base, shelf)?
        .into_iter()
        .filter(|path| theirs.contains(path))
        .map(PathBuf::from)
        .collect();
    Ok(Restoring::Conflict(conflicting))
}

/// Removes `task`'s shelf, but only while it is still `expected`: a shelf
/// written since is somebody's newer work.
pub fn drop_shelf(root: &Path, task: &str, expected: &str) -> Result<(), String> {
    write(root, &["update-ref", "-d", &shelf_ref(task), expected]).map(|_| ())
}

/// Takes back the shelf `written` for `task`, leaving the ref where it
/// stood before: at `earlier`, or absent. Compare-and-swap, like every move
/// of the ref.
pub fn unshelve(
    root: &Path,
    task: &str,
    written: &str,
    earlier: Option<&str>,
) -> Result<(), String> {
    match earlier {
        Some(earlier) => swap_ref(root, &shelf_ref(task), earlier, written),
        None => drop_shelf(root, task, written),
    }
}

/// Every shelf in the repository, read in one process. Each record ends
/// in two NULs, which neither a ref name nor a commit message holds.
pub fn list(primary: &Path) -> Vec<Shelf> {
    let Ok(listing) = read_untrimmed(
        primary,
        &[
            "for-each-ref",
            "--format=%(refname:lstrip=3)%00%(objectname)%00%(contents)%00%00",
            SHELF_REFS,
        ],
    ) else {
        return Vec::new();
    };
    listing
        .split("\0\0\n")
        .filter_map(|record| {
            let mut fields = record.splitn(3, '\0');
            let task = fields.next()?.trim();
            let commit = fields.next()?.trim();
            let message = fields.next().unwrap_or_default();
            let trailer = |key: &str| {
                message
                    .lines()
                    .rev()
                    .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
                    .map(|value| value.trim().to_owned())
                    .unwrap_or_default()
            };
            (!task.is_empty() && !commit.is_empty()).then(|| Shelf {
                task: task.to_owned(),
                commit: commit.to_owned(),
                label: trailer(SHELF_TRAILER_LABEL),
                branch: trailer(SHELF_TRAILER_BRANCH),
            })
        })
        .collect()
}

/// The commit `task`'s shelf points at, if it has one.
pub fn shelf_of(primary: &Path, task: &str) -> Option<String> {
    let commit = tip_of(primary, &shelf_ref(task));
    (!commit.is_empty()).then_some(commit)
}

/// Whether everything `shelf` holds is already in `target`: the commits
/// it was cut from, and, path by path, every change it made on top of
/// them — deletions and renames each as the two paths they are, a mode as
/// much as the bytes. Never by pathspec, so a file named like a pattern is
/// only ever itself.
pub fn is_in_target(primary: &Path, shelf: &str, target: &str) -> bool {
    let target = tip_of(primary, &format!("{target}^{{commit}}"));
    is_in_commit(primary, shelf, &target)
}

/// [`is_in_target`] against a target already resolved to its commit, as a
/// pass holding the repository's branch tips has it.
///
/// Remembered per pair of commits for the life of the process: neither can
/// change, so neither can the answer, and every collection and evaluation
/// asks it again of every shelf the project keeps.
pub fn is_in_commit(primary: &Path, shelf: &str, target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    let question = (primary.to_path_buf(), shelf.to_owned(), target.to_owned());
    let remembered = IN_TARGET
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&question)
        .copied();
    if let Some(answer) = remembered {
        return answer;
    }
    let Some(answer) = carried_by(primary, shelf, target) else {
        return false;
    };
    let mut remembered = IN_TARGET
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if remembered.len() >= REMEMBERED_INTEGRATIONS {
        remembered.clear();
    }
    remembered.insert(question, answer);
    answer
}

type InTargetQuestion = (PathBuf, String, String);

static IN_TARGET: std::sync::LazyLock<std::sync::Mutex<HashMap<InTargetQuestion, bool>>> =
    std::sync::LazyLock::new(std::sync::Mutex::default);

/// `None` when Git could not answer: only a complete answer is remembered.
fn carried_by(primary: &Path, shelf: &str, target: &str) -> Option<bool> {
    let parent = tip_of(primary, &format!("{shelf}^1"));
    if parent.is_empty() {
        return None;
    }
    if !integrated_commits(primary, target.to_owned(), parent.clone()) {
        return Some(false);
    }
    let changed = changed_paths(primary, &parent, shelf).ok()?;
    if changed.is_empty() {
        return Some(true);
    }
    let differing: std::collections::HashSet<String> = changed_paths(primary, target, shelf)
        .ok()?
        .into_iter()
        .collect();
    Some(changed.iter().all(|path| !differing.contains(path)))
}

/// Whether `slot` holds work a shelf cannot capture: a submodule with
/// changes or commits of its own, or a repository nested in it.
pub fn uncapturable_work(slot: &Path) -> Option<String> {
    let status = read_untrimmed(
        slot,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ],
    )
    .ok()?;
    let mut records = status.split('\0').filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let fields: Vec<&str> = record.splitn(9, ' ').collect();
        match fields.first().copied() {
            Some("1") if fields.get(2).is_some_and(|sub| sub.starts_with('S')) => {
                return Some(format!("submodule `{}`", fields.get(8).unwrap_or(&"")));
            }
            Some("2") => {
                // A rename carries its original path as the next record.
                if fields.get(2).is_some_and(|sub| sub.starts_with('S')) {
                    return Some("a submodule".to_owned());
                }
                records.next();
            }
            Some("?") => {
                let path = record.trim_start_matches("? ");
                if path.ends_with('/') && slot.join(path).join(".git").exists() {
                    return Some(format!("repository `{}`", path.trim_end_matches('/')));
                }
            }
            _ => {}
        }
    }
    None
}

/// The derived files whose change is not work, by the same predicate a
/// checkout is judged free by.
fn derived_paths(slot: &Path) -> Vec<&'static str> {
    let known = |name: &str| {
        slot.join(name).exists() || matches!(committed_text(slot, name), Committed::Text(_))
    };
    let mut derived = Vec::new();
    let lock = crate::project_lock::LOCK_FILE_NAME;
    if known(lock) && lock_moves_no_pin(slot) {
        derived.push(lock);
    }
    let instructions = crate::project_context::AGENTS_MD_FILE_NAME;
    if known(instructions) && instructions_changed_only_in_regions(slot) {
        derived.push(instructions);
    }
    derived
}

fn changed_paths(root: &Path, from: &str, to: &str) -> Result<Vec<String>, String> {
    let listing = read_untrimmed(
        root,
        &[
            "diff-tree",
            "-r",
            "-z",
            "--no-renames",
            "--name-only",
            from,
            to,
        ],
    )?;
    Ok(listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Compare-and-swap: `earlier` empty means the ref must not exist yet.
fn swap_ref(root: &Path, reference: &str, new: &str, earlier: &str) -> Result<(), String> {
    write(
        root,
        &["update-ref", "-m", "uze: shelve", reference, new, earlier],
    )
    .map(|_| ())
}

fn git_dir(slot: &Path) -> Option<PathBuf> {
    uze_git::repository::git_dir(slot).ok()
}

/// A copy of a checkout's index in its own Git directory, removed when
/// dropped. A copy, not a fresh one: it keeps the stat cache, so `add`
/// rehashes only what changed, and sparse and intent-to-add entries, so a
/// path outside the cone never reads as deleted.
struct TemporaryIndex(PathBuf);

impl TemporaryIndex {
    fn copied_from(git_dir: &Path) -> Result<Self, String> {
        let path = git_dir.join(format!("uze-shelf-index-{}", std::process::id()));
        let source = git_dir.join("index");
        if source.exists() {
            fs::copy(&source, &path)
                .map_err(|error| format!("the index could not be copied: {error}"))?;
        }
        Ok(Self(path))
    }

    fn as_str(&self) -> &str {
        self.0.to_str().unwrap_or_default()
    }
}

impl Drop for TemporaryIndex {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// A write that must not depend on the operator's own Git: a fixed
/// identity, since `commit-tree` refuses without one, and no hooks, since a
/// `reference-transaction` hook is theirs to run on their own refs. The
/// hooks directory is one that never exists, inside the checkout's own Git
/// directory.
fn plumbing(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    let no_hooks = git_dir(root)
        .map(|dir| dir.join("uze-no-hooks").to_string_lossy().into_owned())
        .ok_or_else(|| "its Git directory could not be read".to_owned())?;
    let mut all: Vec<(&str, &str)> = vec![
        ("GIT_AUTHOR_NAME", "UZE"),
        ("GIT_AUTHOR_EMAIL", "uze@localhost"),
        ("GIT_COMMITTER_NAME", "UZE"),
        ("GIT_COMMITTER_EMAIL", "uze@localhost"),
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_0", &no_hooks),
    ];
    all.extend_from_slice(env);
    crate::git::write_with_env(root, args, &all)?
        .successful()
        .map(|stdout| stdout.trim().to_owned())
}

fn write(root: &Path, args: &[&str]) -> Result<String, String> {
    plumbing(root, args, &[])
}

fn read(root: &Path, args: &[&str]) -> Result<String, String> {
    read_untrimmed(root, args).map(|stdout| stdout.trim().to_owned())
}

fn read_untrimmed(root: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::read(root, args)?.successful()
}
