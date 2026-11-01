//! What the review asks Git: the commit the branch is compared against,
//! what differs from it, one file's diff, and the totals the door says.
//!
//! Every read here runs in a job, off the drawing thread. Each passes
//! `--end-of-options` before anything the checkout supplied (a target's
//! name), so a branch named like an option is read as a name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::diff::{self, DiffCell};
use crate::Host;
use crate::shared::git_status::{
    ChangedFile, FileStatus, STATUS_ARGS, parse_numstat, parse_porcelain_status,
};
use crate::view::Picture;

/// What the branch is compared against.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Base {
    /// The merge base of `HEAD` and the target, or `None` when the review
    /// can only show what is not committed yet.
    pub(crate) commit: Option<String>,
    /// Why it can only show that, said where the diff is.
    pub(crate) reason: Option<String>,
    /// The tips the merge base was taken from: while both stand, it is
    /// still the merge base, and asking again is a waste.
    pub(crate) tips: Option<(String, String)>,
}

impl Base {
    fn uncommitted(reason: impl Into<String>) -> Self {
        Self {
            commit: None,
            reason: Some(reason.into()),
            tips: None,
        }
    }
}

/// The commit `root`'s branch is compared against: the merge base of
/// `HEAD` with the target as last fetched (`refs/uze/sync/<target>`), or
/// the local target when it was never fetched. `known` is the base last
/// found, reused while neither tip moved.
pub(crate) fn base(host: &dyn Host, root: &Path, target: Option<&str>, known: &Base) -> Base {
    let Some(target) = target.filter(|target| !target.is_empty()) else {
        return Base::uncommitted("its target is not known yet");
    };
    let branch = rev(host, root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    if branch.as_deref() == Some(target) {
        return Base::uncommitted(format!("the branch is its own target, {target}"));
    }
    let fetched = format!("refs/uze/sync/{target}");
    let target_ref = match commit_of(host, root, &fetched) {
        Some(_) => fetched,
        None => target.to_owned(),
    };
    let (Some(head), Some(tip)) = (
        commit_of(host, root, "HEAD"),
        commit_of(host, root, &target_ref),
    ) else {
        return Base::uncommitted(format!("{target} cannot be read here"));
    };
    let tips = Some((head.clone(), tip.clone()));
    if known.tips == tips && known.commit.is_some() {
        return known.clone();
    }
    match rev(host, root, &["merge-base", "--end-of-options", &head, &tip]) {
        Some(commit) => Base {
            commit: Some(commit),
            reason: None,
            tips,
        },
        None => Base::uncommitted(format!("the branch shares no history with {target}")),
    }
}

fn commit_of(host: &dyn Host, root: &Path, name: &str) -> Option<String> {
    rev(
        host,
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{name}^{{commit}}"),
        ],
    )
}

fn rev(host: &dyn Host, root: &Path, args: &[&str]) -> Option<String> {
    host.git(root, args, &[])
        .ok()
        .map(|output| output.trim().to_owned())
        .filter(|output| !output.is_empty())
}

/// What differs, file by file, and which of those files have work not yet
/// committed: the ones a discard can act on.
#[derive(Clone, Debug, Default)]
pub(crate) struct Listing {
    pub(crate) files: Vec<ChangedFile>,
    pub(crate) uncommitted: BTreeSet<PathBuf>,
}

/// Every file that differs from `base` in the working tree, untracked ones
/// included, or only the uncommitted ones when `base` is `None`.
pub(crate) fn listing(host: &dyn Host, root: &Path, base: Option<&str>) -> Result<Listing, String> {
    let status = host.git(root, STATUS_ARGS, &[])?;
    let uncommitted_files = parse_porcelain_status(&status, root);
    let uncommitted: BTreeSet<PathBuf> = uncommitted_files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let Some(base) = base else {
        return Ok(Listing {
            files: uncommitted_files,
            uncommitted,
        });
    };
    let named = host.git(
        root,
        &["diff", "--name-status", "-z", "--no-renames", base, "--"],
        &[],
    )?;
    let mut files = parse_name_status(&named, root);
    files.extend(
        uncommitted_files
            .into_iter()
            .filter(|file| file.status == FileStatus::Untracked),
    );
    files.sort_by_cached_key(|file| {
        (
            file.path.parent().map(Path::to_path_buf),
            file.path
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase()),
        )
    });
    files.dedup_by(|a, b| a.path == b.path);
    Ok(Listing { files, uncommitted })
}

/// `git diff --name-status -z`: a status letter, then the path, each
/// NUL-terminated.
fn parse_name_status(output: &str, root: &Path) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut fields = output.split('\0');
    while let Some(code) = fields.next() {
        let Some(path) = fields.next() else {
            break;
        };
        if code.is_empty() || path.is_empty() {
            continue;
        }
        let status = match code.chars().next() {
            Some('A') => FileStatus::Added,
            Some('D') => FileStatus::Deleted,
            _ => FileStatus::Modified,
        };
        files.push(ChangedFile {
            status,
            path: root.join(path),
            renamed_from: None,
        });
    }
    files
}

/// One file's diff, read for the review.
pub(crate) struct FileDiff {
    pub(crate) path: PathBuf,
    pub(crate) cells: Vec<DiffCell>,
    /// One number for what Git answered: a viewed mark holds for this.
    pub(crate) fingerprint: u64,
    pub(crate) binary: bool,
    pub(crate) picture: Option<Picture>,
}

/// `path`'s diff against `base` (or `HEAD`, for what is uncommitted), an
/// untracked file against nothing.
pub(crate) fn file_diff(
    host: &dyn Host,
    root: &Path,
    base: Option<&str>,
    path: &Path,
    status: FileStatus,
) -> Result<FileDiff, String> {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let output = if status == FileStatus::Untracked {
        // `--no-index` exits 1 for "the two differ", which against
        // `/dev/null` is every time.
        host.git(
            root,
            &["diff", "--no-index", "--", "/dev/null", &relative],
            &[1],
        )?
    } else {
        // Literal: a name with `*`, `?` or `[` in it is otherwise a pattern
        // that matches its neighbours too.
        let pathspec = format!(":(literal){relative}");
        let against = base.unwrap_or("HEAD");
        host.git(root, &["diff", against, "--", &pathspec], &[])
            .or_else(|_| host.git(root, &["diff", "--cached", "--", &pathspec], &[]))?
    };
    let theme = host.syntax_theme();
    let binary = diff::is_binary(&output);
    let picture = (binary && crate::shared::picture::is_picture(path))
        .then(|| host.read_bytes(path).ok())
        .flatten()
        .and_then(|bytes| crate::shared::picture::decode(path, &bytes));
    Ok(FileDiff {
        path: path.to_path_buf(),
        fingerprint: fingerprint(&output),
        cells: diff::read(&output, path, &theme),
        binary,
        picture,
    })
}

/// One number standing for a diff as Git wrote it.
pub(crate) fn fingerprint(output: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    output.hash(&mut hasher);
    hasher.finish()
}

/// What the door counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Totals {
    pub(crate) additions: u32,
    pub(crate) deletions: u32,
    pub(crate) files: usize,
}

/// The lines the branch adds and removes against `base` (or `HEAD`),
/// untracked files counted as added in full, as the review shows them.
pub(crate) fn totals(host: &dyn Host, root: &Path, base: Option<&str>) -> Option<Totals> {
    // The diff before the status: a diff writes what it learned back to
    // the index, so the status after it is the cheap one.
    let against = base.unwrap_or("HEAD");
    let numstat = host
        .git(root, &["diff", "--numstat", against, "--"], &[])
        .or_else(|_| host.git(root, &["diff", "--numstat", "--cached"], &[]))
        .ok()?;
    let listing = listing(host, root, base).ok()?;
    let (additions, deletions) = parse_numstat(&numstat);
    let mut totals = Totals {
        additions,
        deletions,
        files: listing.files.len(),
    };
    for file in listing
        .files
        .iter()
        .filter(|file| file.status == FileStatus::Untracked)
    {
        totals.additions = totals
            .additions
            .saturating_add(host.count_lines(&file.path));
    }
    Some(totals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::DiskHost;

    /// A branch with one commit of its own on `main`, which upstream has
    /// already taken: fetched into `refs/uze/sync/main` while the local
    /// `main` stayed behind.
    #[test]
    fn work_already_landed_upstream_is_not_listed_as_the_branchs_own() {
        let repository = uze_testkit::git::Repository::new("review-stale-target");
        let main = repository.branch();
        repository.git(&["checkout", "-b", "fix/naming"]);
        let landed = repository.commit_file("landed.rs", "fn landed() {}\n");
        let root = repository.root();

        let behind = base(&DiskHost, root, Some(&main), &Base::default());
        let listed = listing(&DiskHost, root, behind.commit.as_deref()).unwrap();
        assert_eq!(
            listed.files.len(),
            1,
            "against the local {main}, the commit is the branch's"
        );

        repository.git(&["update-ref", &format!("refs/uze/sync/{main}"), &landed]);
        let fetched = base(&DiskHost, root, Some(&main), &Base::default());
        let listed = listing(&DiskHost, root, fetched.commit.as_deref()).unwrap();
        assert!(
            listed.files.is_empty(),
            "against the fetched {main}, it already landed"
        );
    }

    /// A sibling agent's checkout of the same repository holds a change;
    /// the review of this checkout lists nothing of it.
    #[test]
    fn a_review_reads_its_own_checkout_and_never_a_siblings() {
        let repository = uze_testkit::git::Repository::new("review-one-checkout");
        let sibling = repository
            .root()
            .with_file_name("review-one-checkout-sibling");
        repository.git(&[
            "worktree",
            "add",
            "-b",
            "sibling",
            &sibling.display().to_string(),
        ]);
        std::fs::write(sibling.join("theirs.rs"), "fn theirs() {}\n").unwrap();
        let listed = listing(&DiskHost, repository.root(), None).unwrap();
        assert!(listed.files.is_empty(), "{:?}", listed.files);
        let theirs = listing(&DiskHost, &sibling, None).unwrap();
        assert_eq!(theirs.files.len(), 1);
    }

    #[test]
    fn a_branch_that_is_its_own_target_shows_what_is_uncommitted_and_says_why() {
        let repository = uze_testkit::git::Repository::new("review-own-target");
        let main = repository.branch();
        let found = base(&DiskHost, repository.root(), Some(&main), &Base::default());
        assert_eq!(found.commit, None);
        assert!(found.reason.unwrap().contains("its own target"));
    }

    #[test]
    fn a_name_status_listing_reads_each_letter_and_path() {
        let root = Path::new("/repo");
        let files = parse_name_status("M\0src/a.rs\0A\0new.rs\0D\0gone.rs\0", root);
        let read: Vec<(PathBuf, FileStatus)> = files
            .into_iter()
            .map(|file| (file.path, file.status))
            .collect();
        assert_eq!(
            read,
            [
                (root.join("src/a.rs"), FileStatus::Modified),
                (root.join("new.rs"), FileStatus::Added),
                (root.join("gone.rs"), FileStatus::Deleted),
            ]
        );
    }
}
