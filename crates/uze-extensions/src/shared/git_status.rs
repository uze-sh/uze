//! What `git status` and `git diff --numstat` say, read once for every
//! surface that asks: the review lists what changed, the code surface's map
//! marks what is uncommitted, and its history counts what a commit did.
//!
//! Here rather than with any one of them since the changes left the code
//! surface for the review, and the map and the history stayed behind still
//! reading the same output.

use std::path::{Path, PathBuf};

use crate::view::Role;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl FileStatus {
    pub(crate) fn glyph(self) -> &'static str {
        match self {
            FileStatus::Modified => "M",
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "U",
        }
    }

    pub(crate) fn role(self) -> Role {
        match self {
            FileStatus::Modified => Role::Warning,
            FileStatus::Added | FileStatus::Untracked => Role::Success,
            FileStatus::Deleted => Role::Danger,
            FileStatus::Renamed => Role::Info,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChangedFile {
    pub(crate) status: FileStatus,
    /// Absolute — resolved against the repository root (`CodeView::root`),
    /// never the tab's `cwd` directly. `git status` reports paths relative
    /// to the repository root regardless of `-C`, which may differ from a
    /// tab whose `cwd` is a subdirectory; resolving to an absolute path
    /// once here means nothing downstream has to re-derive that.
    pub(crate) path: PathBuf,
    /// Where a rename came from — the other path throwing it away puts
    /// back. `None` for everything else.
    pub(crate) renamed_from: Option<PathBuf>,
}

/// Every changed path, untracked ones listed file by file — the one status
/// both the badge and the overlay read. NUL-separated, because it is the
/// only form that gives a path back verbatim: without `-z` Git quotes and
/// escapes unusual names, and a name containing ` -> ` reads as a rename.
pub(crate) const STATUS_ARGS: &[&str] =
    &["status", "--porcelain=v1", "-z", "--untracked-files=all"];

/// Totals Git's tab-separated `--numstat` output. Binary entries use `-`
/// counts and intentionally contribute zero: there is no meaningful line
/// delta to show in the compact badge.
pub(crate) fn parse_numstat(output: &str) -> (u32, u32) {
    output.lines().fold((0, 0), |(additions, deletions), line| {
        let mut fields = line.split('\t');
        let addition = fields.next().and_then(|value| value.parse::<u32>().ok());
        let deletion = fields.next().and_then(|value| value.parse::<u32>().ok());
        match (addition, deletion) {
            (Some(addition), Some(deletion)) => (
                additions.saturating_add(addition),
                deletions.saturating_add(deletion),
            ),
            _ => (additions, deletions),
        }
    })
}

/// Parses the output of [`STATUS_ARGS`]. Resolves each reported path
/// (always repository-root-relative, regardless of `-C` — see
/// `ChangedFile::path`'s doc comment) against `root` so every
/// `ChangedFile` carries an absolute path.
///
/// Listed grouped by the directory each file sits in and by name within
/// it, the way a flat changes list reads: a file's neighbours are the
/// files beside it on disk, rather than whatever order Git's index
/// happens to keep.
pub(crate) fn parse_porcelain_status(output: &str, root: &Path) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut records = output.split('\0');
    while let Some(record) = records.next() {
        let Some((code, relative)) = record
            .split_at_checked(3)
            .map(|(state, path)| (&state[..2], path))
        else {
            continue;
        };
        // A rename or copy is two records, where it went and then where it
        // came from — and only the first is where the change lives now.
        let source = match code.contains(['R', 'C']) {
            true => records.next(),
            false => None,
        };
        if relative.is_empty() {
            continue;
        }
        let status = if code == "??" {
            FileStatus::Untracked
        } else if code.contains(['R', 'C']) {
            FileStatus::Renamed
        } else if code.contains('A') {
            FileStatus::Added
        } else if code.contains('D') {
            FileStatus::Deleted
        } else {
            FileStatus::Modified
        };
        files.push(ChangedFile {
            status,
            path: root.join(relative),
            // A copy leaves its source where it was.
            renamed_from: source
                .filter(|_| code.contains('R'))
                .map(|source| root.join(source)),
        });
    }
    files.sort_by_cached_key(|file| {
        (
            file.path.parent().map(Path::to_path_buf),
            file.path
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase()),
        )
    });
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ordinary_status_codes() {
        let root = Path::new("/repo");
        let output = " M modified.rs\0A  added.rs\0 D deleted.rs\0?? untracked.rs\0";
        let files = parse_porcelain_status(output, root);
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].status, FileStatus::Added);
        assert_eq!(files[1].status, FileStatus::Deleted);
        assert_eq!(files[2].status, FileStatus::Modified);
        assert_eq!(files[2].path, root.join("modified.rs"));
        assert_eq!(files[3].status, FileStatus::Untracked);
        assert_eq!(files[3].path, root.join("untracked.rs"));
    }
    #[test]
    fn lists_files_by_directory_then_by_name_whatever_order_git_kept() {
        let root = Path::new("/repo");
        let output = " M src/ui/b.rs\0 M src/Z.rs\0 M src/a.rs\0 M top.rs\0";
        let names: Vec<PathBuf> = parse_porcelain_status(output, root)
            .into_iter()
            .map(|file| file.path)
            .collect();
        assert_eq!(
            names,
            ["top.rs", "src/a.rs", "src/Z.rs", "src/ui/b.rs"].map(|path| root.join(path))
        );
    }
    /// `x` deleted and `x/y` added in its place are two rows, each
    /// reachable, whichever order Git named them in.
    #[test]
    fn a_path_that_is_a_file_and_a_directory_keeps_both_rows() {
        let root = Path::new("/repo");
        let files = parse_porcelain_status("?? x/y\0 D x\0", root);
        let listed: Vec<(PathBuf, FileStatus)> = files
            .into_iter()
            .map(|file| (file.path, file.status))
            .collect();
        assert_eq!(
            listed,
            [
                (root.join("x"), FileStatus::Deleted),
                (root.join("x/y"), FileStatus::Untracked),
            ]
        );
    }
    #[test]
    fn parses_a_rename_using_the_destination_path() {
        let root = Path::new("/repo");
        let files = parse_porcelain_status("R  new-name.rs\0old-name.rs\0", root);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].path, root.join("new-name.rs"));
    }
    #[test]
    fn ignores_an_empty_status() {
        assert!(parse_porcelain_status("", Path::new("/repo")).is_empty());
    }
    /// Each of these is a name the line-oriented form mangles: quoted and
    /// escaped, split at an arrow, or trimmed.
    #[test]
    fn a_name_is_taken_verbatim_however_unusual() {
        let root = Path::new("/repo");
        let output = "?? say \"hi\".rs\0 M a -> b.rs\0?? \u{e9}t\u{e9}.rs\0 M  leading.rs\0";
        let paths: Vec<PathBuf> = parse_porcelain_status(output, root)
            .into_iter()
            .map(|file| file.path)
            .collect();
        assert_eq!(
            paths,
            [
                root.join(" leading.rs"),
                root.join("a -> b.rs"),
                root.join("say \"hi\".rs"),
                root.join("\u{e9}t\u{e9}.rs"),
            ]
        );
    }
    #[test]
    fn totals_that_would_overflow_stay_at_the_largest_count() {
        assert_eq!(
            parse_numstat("4294967295\t1\ta.rs\n1\t0\tb.rs\n"),
            (u32::MAX, 1)
        );
    }
    #[test]
    fn totals_text_numstat_and_ignores_binary_entries() {
        assert_eq!(parse_numstat("2\t1\tsrc/lib.rs\n-\t-\timage.png\n"), (2, 1));
    }
}
