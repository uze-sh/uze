//! What Git says about a checkout: its branch, its tip, whether it is dirty, how far it is from its upstream and its target.

use super::*;

/// Renames a branch, under the repository's write lock.
///
/// Taken in the primary the way every other write is, so a rename cannot
/// interleave with a slot acquisition creating the very branch it renames.
/// Git moves the ref and every checkout on it follows, so the agent's own
/// working directory needs nothing done to it.
pub fn rename_branch(primary: &Path, from: &str, to: &str) -> crate::Result<()> {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        git(primary, &["branch", "--move", "--", from, to])
            .map(|_| ())
            .map_err(|reason| crate::UzeError::TaskNaming(reason.to_string()))
    })
    .map_err(|reason| crate::UzeError::TaskNaming(reason.to_string()))?
}

/// The branch checked out in `root`, or `None` for a detached `HEAD`.
/// `symbolic-ref` rather than `rev-parse --abbrev-ref`: it still names the
/// branch when it has no commit yet, which is the case that must be told
/// apart from "no branch at all".
pub fn current_branch(root: &Path) -> Option<String> {
    let branch = crate::git::read(root, &["symbolic-ref", "--short", "--quiet", "HEAD"])
        .ok()?
        .successful()
        .ok()?;
    let branch = branch.trim();
    (!branch.is_empty()).then(|| branch.to_owned())
}

/// The commit `reference` resolves to in `root`.
pub fn tip_of(root: &Path, reference: &str) -> String {
    crate::git::read(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            reference,
        ],
    )
    .ok()
    .and_then(|output| output.successful().ok())
    .map(|stdout| stdout.trim().to_owned())
    .unwrap_or_default()
}

/// Uncommitted changes, tracked or untracked-but-not-ignored. What rebasing,
/// joining and delivering ask: they need a tree with nothing at all in it.
pub fn is_dirty(root: &Path) -> bool {
    crate::git::read(root, &["status", "--porcelain"])
        .ok()
        .and_then(|output| output.successful().ok())
        .is_none_or(|status| !status.trim().is_empty())
}

/// Whether `root` holds uncommitted changes that are somebody's work — the
/// question a checkout is parked or freed by. Content UZE derives and can
/// produce again is not: a lock that only gained or lost entries, which is
/// all `install` does to it, and an instruction file that changed only
/// inside the regions UZE manages. Left alone, those are what a slot
/// collects just by having UZE run in it, and each parked the slot for
/// good. A question Git could not answer is taken as yes.
pub fn holds_uncommitted_work(root: &Path) -> bool {
    let Some(status) = crate::git::read(root, &["status", "--porcelain=v1", "-z"])
        .ok()
        .and_then(|output| output.successful().ok())
    else {
        return true;
    };
    status
        .split('\0')
        .filter(|record| !record.is_empty())
        .any(|record| match record.split_at_checked(3) {
            // A rename or a copy is an edit somebody made, whatever it names.
            Some((code, _)) if code.starts_with(['R', 'C']) => true,
            Some((_, path)) if path == crate::project_lock::LOCK_FILE_NAME => {
                !lock_moves_no_pin(root)
            }
            Some((_, path)) if path == crate::project_context::AGENTS_MD_FILE_NAME => {
                !instructions_changed_only_in_regions(root)
            }
            _ => true,
        })
}

/// Whether every plugin the committed lock and the working one both carry
/// keeps its revision and its digest: only entries were added or dropped.
/// A moved pin is what `update` exists to write, and that is work.
pub(super) fn lock_moves_no_pin(root: &Path) -> bool {
    use crate::project_lock::{LOCK_FILE_NAME, ProjectLock, parse_lock_str};

    let path = root.join(LOCK_FILE_NAME);
    let committed = match committed_text(root, LOCK_FILE_NAME) {
        Committed::Absent => ProjectLock::default(),
        Committed::Text(text) => match parse_lock_str(&text, &path) {
            Ok(lock) => lock,
            Err(_) => return false,
        },
        Committed::Unknown => return false,
    };
    let working = match crate::project_lock::load_lock(root) {
        Ok(lock) => lock.unwrap_or_default(),
        Err(_) => return false,
    };
    let revision = |lock: &ProjectLock, marketplace: &str| {
        lock.marketplaces
            .get(marketplace)
            .map(|entry| entry.revision.clone())
    };
    working.plugins.iter().all(|(name, now)| {
        committed.plugins.get(name).is_none_or(|before| {
            before.integrity == now.integrity
                && revision(&committed, &before.marketplace) == revision(&working, &now.marketplace)
        })
    })
}

pub(super) fn instructions_changed_only_in_regions(root: &Path) -> bool {
    use crate::project_context::AGENTS_MD_FILE_NAME;

    let committed = match committed_text(root, AGENTS_MD_FILE_NAME) {
        Committed::Absent => String::new(),
        Committed::Text(text) => text,
        Committed::Unknown => return false,
    };
    let working = match fs::read(root.join(AGENTS_MD_FILE_NAME)) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => return false,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return false,
    };
    crate::text_region::same_outside_managed_regions(&committed, &working)
}

pub(super) enum Committed {
    Absent,
    Text(String),
    Unknown,
}

/// A top-level file as `HEAD` has it.
pub(super) fn committed_text(root: &Path, name: &str) -> Committed {
    let Some(listed) = crate::git::read(root, &["ls-tree", "--name-only", "HEAD", "--", name])
        .ok()
        .and_then(|output| output.successful().ok())
    else {
        return Committed::Unknown;
    };
    if listed.trim().is_empty() {
        return Committed::Absent;
    }
    crate::git::read(root, &["show", &format!("HEAD:{name}")])
        .ok()
        .and_then(|output| output.successful().ok())
        .map_or(Committed::Unknown, Committed::Text)
}

/// How far the branch checked out in `root` and its upstream have moved
/// apart: what a pull would bring in and what a push would send out.
/// `None` when there is nothing to measure against — a detached `HEAD`,
/// or a branch with no upstream configured.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpstreamDivergence {
    /// Commits the upstream has that the branch lacks.
    pub behind: usize,
    /// Commits the branch has that the upstream lacks.
    pub ahead: usize,
}

pub fn upstream_divergence(root: &Path) -> Option<UpstreamDivergence> {
    let counts = crate::git::read(
        root,
        &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
    )
    .ok()?
    .successful()
    .ok()?;
    let (behind, ahead) = counts.trim().split_once('\t')?;
    Some(UpstreamDivergence {
        behind: behind.parse().ok()?,
        ahead: ahead.parse().ok()?,
    })
}

/// How many commits `branch` has that `target` lacks, or `None` when Git
/// could not answer — either ref missing, most commonly a declared
/// `workspace.target` this clone does not have.
///
/// The distinction is the whole of it: an unanswerable question is not
/// "nothing ahead". Every predicate that authorizes a removal asks this
/// one, so a count that fell back to zero read as "this branch is fully in
/// the target" for *every* branch in the repository.
pub fn commits_ahead_checked(root: &Path, target: &str, branch: &str) -> Option<usize> {
    crate::git::read(
        root,
        &[
            "rev-list",
            "--count",
            "--end-of-options",
            &format!("{target}..{branch}"),
            "--",
        ],
    )
    .ok()
    .and_then(|output| output.successful().ok())
    .and_then(|count| count.trim().parse().ok())
}

/// How many commits `branch` has that `target` lacks, zero when Git could
/// not answer. For displaying a count; anything deciding whether work may
/// be destroyed asks [`commits_ahead_checked`].
pub fn commits_ahead(root: &Path, target: &str, branch: &str) -> usize {
    commits_ahead_checked(root, target, branch).unwrap_or(0)
}
