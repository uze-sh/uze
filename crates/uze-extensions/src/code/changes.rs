//! What a checkout currently has that its last commit does not: the
//! compact summary the tab strip badges, and the changed-file list the
//! overlay navigates.
//!
//! Its own module because "what changed" is a question with one answer
//! and several readers — the badge asks it on a timer and the overlay
//! asks it on every refresh — so the parsing of Git's porcelain has to
//! be one thing rather than the same shape written twice.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};

use super::diff::{self, DiffCell};
use crate::{
    Host,
    view::{Role, ScrollDirection},
};

/// What the checkout has that its last commit does not, and the diff of
/// whichever file the surface is on.
///
/// One of the two halves [`super::CodeView`] holds. It answers about a
/// path and knows nothing about the other half — see that type for why
/// the two stay apart.
#[derive(Default)]
pub(super) struct Changes {
    /// In the order the navigator lists them — see
    /// [`parse_porcelain_status`].
    pub(super) files: Vec<ChangedFile>,
    pub(super) diff: Vec<DiffCell>,
    /// Git answered for the selected file that it is binary, so `diff` is
    /// empty because there are no lines to compare, not because nothing
    /// changed.
    pub(super) diff_binary: bool,
    /// Set when the selection moved and cleared when a read catches up.
    ///
    /// Reading and highlighting a diff is the one thing here whose cost
    /// has no bound, and an arrow key may not pay for it on the thread
    /// that draws. So selecting records only *what* is selected; the host
    /// re-reads, and until it answers this says the diff on screen is not
    /// the one being asked for.
    pub(super) diff_pending: bool,
    /// Set instead of populating `files`/`diff` when the checkout isn't a
    /// git repository, `git` isn't on `PATH`, or a `git diff` fails —
    /// shown in place of the changed-file list rather than refusing to
    /// open at all.
    pub(super) error: Option<String>,
    pub(super) refreshed_at: Option<Instant>,
    /// Each file's place in `files`, by path — what marks a changed file
    /// in the other list, asked once per row per frame.
    pub(super) positions: OnceLock<HashMap<PathBuf, usize>>,
    /// How long the read that produced this took, for the pacing of the
    /// next one (see [`super::PACE`]).
    pub(super) took: std::time::Duration,
    /// What the diff on screen was read from — the raw output and the
    /// palette together, as one number.
    ///
    /// Carried so the next read can tell that it found the same thing.
    /// A diff is re-read on a timer, because the file behind it is under
    /// an agent's hands; colouring it again when not one byte of it moved
    /// is the same picture drawn twice, and for a large diff it is the
    /// most expensive thing this surface does.
    pub(super) diff_digest: u64,
    /// The read found the diff exactly as it already was, so it carries
    /// no cells — the ones on screen are the answer.
    pub(super) diff_unchanged: bool,
}

impl Changes {
    /// Everything `git status` says, plus the diff of `selected`.
    ///
    /// Takes no `&self`: this is the slow half of the surface — a
    /// `status`, a `diff`, and the highlighting of that diff — and an
    /// associated function can run wherever the host puts it. The host
    /// reads on a thread and installs the answer when it lands, which is
    /// why nothing here may borrow the view being refreshed.
    ///
    /// It answers with the changes *only*. The whole view used to be
    /// rebuilt this way, which was safe while nothing in it was authored
    /// by the viewer; the same struct now holds a buffer, so a refresh
    /// that could reach it would be a refresh that eats what was typed.
    pub(super) fn read(host: &dyn Host, root: &Path, selected: Option<&Path>, shown: u64) -> Self {
        let status = match host.git(root, STATUS_ARGS, &[]) {
            Ok(output) => output,
            Err(message) => return Self::failed(message),
        };
        let mut changes = Self {
            files: parse_porcelain_status(&status, root),
            refreshed_at: Some(Instant::now()),
            ..Self::default()
        };
        changes.load_diff(host, root, selected, shown);
        changes
    }

    /// A read that could not answer, and why — drawn where the diff would
    /// be, while the files half carries on.
    pub(super) fn failed(message: String) -> Self {
        Self {
            error: Some(message),
            refreshed_at: Some(Instant::now()),
            ..Self::default()
        }
    }

    /// Where `path` sits in the changed-file list, if it changed at all.
    pub(super) fn position_of(&self, path: Option<&Path>) -> Option<usize> {
        let path = path?;
        self.positions
            .get_or_init(|| {
                self.files
                    .iter()
                    .enumerate()
                    .map(|(index, file)| (file.path.clone(), index))
                    .collect()
            })
            .get(path)
            .copied()
    }

    /// Takes over what `previous` already worked out about the list, when
    /// the list is the same one — which, re-read every second or two, it
    /// nearly always is.
    pub(super) fn inherit(&mut self, previous: &mut Changes) {
        if self.files != previous.files {
            return;
        }
        self.positions = std::mem::take(&mut previous.positions);
    }

    fn load_diff(&mut self, host: &dyn Host, root: &Path, selected: Option<&Path>, shown: u64) {
        let Some(file) = self
            .position_of(selected)
            .and_then(|index| self.files.get(index))
        else {
            self.diff = Vec::new();
            self.diff_binary = false;
            return;
        };
        let read = read_diff(host, root, &file.path.clone(), file.status, shown);
        self.install_diff(read);
    }

    /// Puts a diff read in place, whichever read it came from.
    pub(super) fn install_diff(&mut self, read: DiffRead) {
        match read.outcome {
            Err(message) => self.error = Some(message),
            Ok(None) => {
                self.diff_digest = read.digest;
                self.diff_unchanged = true;
            }
            Ok(Some(cells)) => {
                self.diff_digest = read.digest;
                self.diff_unchanged = false;
                self.diff = cells;
                self.diff_binary = read.binary;
            }
        }
    }

    /// One number standing for a diff drawn in a palette: what a re-read
    /// compares against to know it found the same picture.
    fn digest_of(output: &str, theme: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        output.hash(&mut hasher);
        theme.hash(&mut hasher);
        // Zero doubles as "nothing is on screen yet" at the one call site
        // that compares these, so a diff that genuinely hashes to zero is
        // coloured again every read — the cheaper of the two mistakes.
        hasher.finish()
    }

    /// The next changed file from `from`, in `direction`.
    pub(super) fn neighbour(&self, from: usize, direction: ScrollDirection) -> Option<usize> {
        match direction {
            ScrollDirection::Down => from.checked_add(1).filter(|next| *next < self.files.len()),
            ScrollDirection::Up => from.checked_sub(1),
        }
    }
}

/// One file's diff, read on its own: what moving the selection costs,
/// with no `status` of the whole checkout in front of it.
pub(super) struct DiffRead {
    pub(super) path: PathBuf,
    pub(super) digest: u64,
    pub(super) binary: bool,
    /// `Ok(None)` when the read found the diff already on screen
    /// (`shown`), so nothing was coloured again.
    pub(super) outcome: Result<Option<Vec<DiffCell>>, String>,
}

pub(super) fn read_diff(
    host: &dyn Host,
    root: &Path,
    path: &Path,
    status: FileStatus,
    shown: u64,
) -> DiffRead {
    let relative = path.to_string_lossy();
    let raw = if status == FileStatus::Untracked {
        // `--no-index` exits 1 for "the two differ", which against
        // `/dev/null` is every time.
        host.git(
            root,
            &["diff", "--no-index", "--", "/dev/null", &relative],
            &[1],
        )
    } else {
        // Literal, or a name with `*`, `?` or `[` in it is a pattern that
        // matches its neighbours too, and their diffs arrive under its name.
        let pathspec = format!(":(literal){relative}");
        host.git(root, &["diff", "HEAD", "--", &pathspec], &[])
    };
    let output = match raw {
        Ok(output) => output,
        Err(message) => {
            return DiffRead {
                path: path.to_path_buf(),
                digest: 0,
                binary: false,
                outcome: Err(message),
            };
        }
    };
    let theme = host.syntax_theme();
    let digest = Changes::digest_of(&output, &theme);
    // Nothing moved, and the palette is the one it was drawn in.
    let outcome = match digest == shown && shown != 0 {
        true => Ok(None),
        false => Ok(Some(diff::read(&output, path, &theme))),
    };
    DiffRead {
        path: path.to_path_buf(),
        digest,
        binary: diff::is_binary(&output),
        outcome,
    }
}

/// A compact summary for the workspace tab strip. It is deliberately
/// separate from [`Changes`]: the strip needs only a cheap indicator,
/// while opening the surface can afford to load and highlight a full
/// diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangeSummary {
    pub additions: u32,
    pub deletions: u32,
}

/// Returns a summary only when `cwd` resolves to a git repository with
/// changes. `None` covers a non-repository, a missing/unusable `git`, and a
/// clean worktree alike, which lets the caller omit its badge entirely.
pub fn change_summary(host: &dyn Host, cwd: &Path) -> Option<ChangeSummary> {
    let root = host.repository_root(cwd).ok()?;
    // The diff before the status, and the order is the whole point. When
    // files were touched without changing — a formatter, a build, a
    // `touch` — the index no longer vouches for them, and whichever read
    // comes first re-hashes the checkout: seconds, on a large one. A
    // `diff` writes what it learned back to the index and a `status`
    // read this way does not, so the diff first is one re-hash, and the
    // status after it is the cheap one it always was.
    //
    // One diff against HEAD, not the staged and unstaged ones added
    // together: a line staged and then edited again appears in both, and
    // the sum counts it twice. A repository with no commit yet has no HEAD
    // to diff against and answers from the index alone.
    let numstat = host
        .git(&root, &["diff", "--numstat", "HEAD"], &[])
        .or_else(|_| host.git(&root, &["diff", "--numstat", "--cached"], &[]))
        .ok()?;
    let status = host.git(&root, STATUS_ARGS, &[]).ok()?;
    let files = parse_porcelain_status(&status, &root);
    if files.is_empty() {
        return None;
    }
    let (additions, deletions) = parse_numstat(&numstat);
    let mut summary = ChangeSummary {
        additions,
        deletions,
    };
    for file in files
        .iter()
        .filter(|file| file.status == FileStatus::Untracked)
    {
        // `git diff` leaves untracked files out, but the overlay shows
        // them against `/dev/null`: counting their lines keeps the badge
        // and the overlay agreeing that they are changes.
        summary.additions = summary
            .additions
            .saturating_add(host.count_lines(&file.path));
    }
    Some(summary)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl FileStatus {
    pub(super) fn glyph(self) -> &'static str {
        match self {
            FileStatus::Modified => "M",
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "U",
        }
    }

    pub(super) fn role(self) -> Role {
        match self {
            FileStatus::Modified => Role::Warning,
            FileStatus::Added | FileStatus::Untracked => Role::Success,
            FileStatus::Deleted => Role::Danger,
            FileStatus::Renamed => Role::Info,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ChangedFile {
    pub(super) status: FileStatus,
    /// Absolute — resolved against the repository root (`CodeView::root`),
    /// never the tab's `cwd` directly. `git status` reports paths relative
    /// to the repository root regardless of `-C`, which may differ from a
    /// tab whose `cwd` is a subdirectory; resolving to an absolute path
    /// once here means nothing downstream has to re-derive that.
    pub(super) path: PathBuf,
    /// Where a rename came from — the other path throwing it away puts
    /// back. `None` for everything else.
    pub(super) renamed_from: Option<PathBuf>,
}

/// Every changed path, untracked ones listed file by file — the one status
/// both the badge and the overlay read. NUL-separated, because it is the
/// only form that gives a path back verbatim: without `-z` Git quotes and
/// escapes unusual names, and a name containing ` -> ` reads as a rename.
pub(super) const STATUS_ARGS: &[&str] =
    &["status", "--porcelain=v1", "-z", "--untracked-files=all"];

/// Totals Git's tab-separated `--numstat` output. Binary entries use `-`
/// counts and intentionally contribute zero: there is no meaningful line
/// delta to show in the compact badge.
pub(super) fn parse_numstat(output: &str) -> (u32, u32) {
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
pub(super) fn parse_porcelain_status(output: &str, root: &Path) -> Vec<ChangedFile> {
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

#[cfg(test)]
mod repository_tests {
    use super::*;

    use crate::code::tests::RepositoryHost as TestHost;

    /// Drives real `git` in a scratch repository — proves the actual
    /// `git status --porcelain=v1 --untracked-files=all`/`git diff HEAD`/
    /// `git diff --no-index` output this module depends on parses the way
    /// the fixture-string tests above assume, not just those fixtures.
    #[test]
    fn open_reads_a_real_repositorys_staged_unstaged_and_untracked_changes() {
        let repository = uze_testkit::git::Repository::new("git-diff-test");
        let root = repository.root().to_path_buf();
        repository.commit_file("tracked.rs", "fn one() {}\n");

        assert_eq!(change_summary(&TestHost, &root), None);

        std::fs::write(root.join("tracked.rs"), "fn one() {}\nfn two() {}\n").unwrap();
        std::fs::write(root.join("staged.rs"), "fn staged() {}\n").unwrap();
        repository.git(&["add", "staged.rs"]);
        std::fs::write(root.join("new.rs"), "fn brand_new() {}\n").unwrap();

        let changes = Changes::read(&TestHost, &root, None, 0);
        assert!(
            changes.error.is_none(),
            "unexpected error: {:?}",
            changes.error
        );
        assert_eq!(
            changes.files.len(),
            3,
            "expected 3 changed files: {:?}",
            changes
                .files
                .iter()
                .map(|f| (&f.path, f.status))
                .collect::<Vec<_>>()
        );
        let statuses: Vec<FileStatus> = changes.files.iter().map(|f| f.status).collect();
        assert!(statuses.contains(&FileStatus::Modified));
        assert!(statuses.contains(&FileStatus::Added));
        assert!(statuses.contains(&FileStatus::Untracked));
        assert_eq!(
            change_summary(&TestHost, &root),
            Some(ChangeSummary {
                additions: 3,
                deletions: 0,
            })
        );
        // Read with a file named, the diff of that file comes with it.
        let first = changes.files[0].path.clone();
        let changes = Changes::read(&TestHost, &root, Some(&first), 0);
        assert!(
            !changes.diff.is_empty(),
            "expected a non-empty diff for the file that was asked about"
        );

        std::fs::write(root.join("written_later.rs"), "fn later() {}\n").unwrap();
        let changes = Changes::read(&TestHost, &root, Some(&first), 0);
        assert!(
            changes
                .files
                .iter()
                .any(|file| file.path == root.join("written_later.rs")),
            "a re-read must pick up a change made while the viewer is open"
        );
        assert_eq!(
            changes.position_of(Some(&first)),
            Some(0),
            "and the file that was asked about is still where it was"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
    /// A name Git would read as a pattern is still one file: the diff of
    /// `[ab].rs` is not also the diff of `a.rs`.
    #[test]
    fn a_diff_is_of_the_file_named_even_when_the_name_is_a_pattern() {
        let repository = uze_testkit::git::Repository::new("git-diff-literal");
        let root = repository.root().to_path_buf();
        repository.commit_file("[ab].rs", "fn bracketed() {}\n");
        repository.commit_file("a.rs", "fn a() {}\n");
        std::fs::write(root.join("[ab].rs"), "fn bracketed() {}\nfn more() {}\n").unwrap();
        std::fs::write(root.join("a.rs"), "fn a() {}\nfn neighbour() {}\n").unwrap();

        let read = read_diff(
            &TestHost,
            &root,
            &root.join("[ab].rs"),
            FileStatus::Modified,
            0,
        );
        let Ok(Some(cells)) = read.outcome else {
            panic!("the diff is read");
        };
        let text: String = cells
            .iter()
            .flat_map(|cell| cell.spans.iter().map(|(_, piece)| piece.as_str()))
            .collect();
        assert!(text.contains("more"), "{text}");
        assert!(!text.contains("neighbour"), "{text}");

        let _ = std::fs::remove_dir_all(&root);
    }
    /// The badge counts the change against HEAD, which is what the viewer
    /// sees on screen — not the staged diff plus the unstaged one. A line
    /// staged and then edited again appears in both of those, and adding
    /// them reported the work twice.
    #[test]
    fn a_line_staged_and_then_edited_again_counts_once() {
        let repository = uze_testkit::git::Repository::new("git-badge-double-count");
        let root = repository.root().to_path_buf();
        repository.commit_file("f.rs", "a\nb\nc\n");

        std::fs::write(root.join("f.rs"), "a\nB\nc\n").unwrap();
        repository.git(&["add", "f.rs"]);
        std::fs::write(root.join("f.rs"), "a\nBB\nc\n").unwrap();

        assert_eq!(
            change_summary(&TestHost, &root),
            Some(ChangeSummary {
                additions: 1,
                deletions: 1,
            }),
            "one line differs from HEAD, however many times it was touched \
             on the way there"
        );
    }
    /// A repository whose first commit has not happened has no HEAD to
    /// diff against; the badge still has to answer for what is staged.
    #[test]
    fn a_repository_with_no_commit_yet_still_reports_what_is_staged() {
        let repository = uze_testkit::git::Repository::new("git-badge-unborn");
        let root = repository.root().to_path_buf();
        std::fs::write(root.join("first.rs"), "fn first() {}\n").unwrap();
        repository.git(&["add", "first.rs"]);

        assert_eq!(
            change_summary(&TestHost, &root),
            Some(ChangeSummary {
                additions: 1,
                deletions: 0,
            })
        );
    }
    /// The scoping rule this view is built on. `git worktree list` answers
    /// repository-wide from anywhere inside the repository, so a view opened
    /// in an agent's isolated checkout would otherwise show the primary's
    /// changes and every sibling agent's alongside its own — including
    /// checkouts whose agent is long gone. Scoping is by checkout, not by
    /// the isolation layout, so it holds for any worktree, however created.
    #[test]
    fn discovers_main_and_configured_linked_worktrees() {
        let repository = uze_testkit::git::Repository::new("worktree-test");
        let root = repository.root().to_path_buf();
        // Ignored, as UZE excludes it, so the primary's own status is not
        // dominated by the checkouts hanging off it.
        repository.commit_file(".gitignore", ".worktrees/\n");
        // Mirrors where UZE isolates agents. Spelled out rather than taken
        // from the domain constant: this crate does not depend on the
        // domain, and the scoping under test is by checkout rather than by
        // that layout — it holds for any worktree, however created.
        let linked = root.join(".worktrees").join("feature");
        std::fs::create_dir_all(linked.parent().unwrap()).unwrap();
        repository.git(&[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "feature",
            linked.to_str().unwrap(),
        ]);
        std::fs::write(root.join("primary-only.rs"), "fn primary() {}\n").unwrap();
        std::fs::write(linked.join("agent-only.rs"), "fn agent() {}\n").unwrap();

        let agent_root = TestHost.repository_root(&linked).expect("a checkout");
        let from_agent = Changes::read(&TestHost, &agent_root, None, 0);
        assert!(from_agent.error.is_none(), "{:?}", from_agent.error);
        assert_eq!(
            crate::shared::checkout::branch_of(&TestHost, &agent_root),
            "feature"
        );
        assert_eq!(
            from_agent
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            vec![linked.join("agent-only.rs")],
            "an isolated agent sees its own checkout and nothing else"
        );

        let primary_root = TestHost.repository_root(&root).expect("a checkout");
        let from_primary = Changes::read(&TestHost, &primary_root, None, 0);
        assert!(from_primary.error.is_none(), "{:?}", from_primary.error);
        assert_eq!(
            from_primary
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            vec![root.join("primary-only.rs")],
            "and the seat sees the seat, not the agents hanging off it"
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
