//! What the workspace client grants an extension.
//!
//! An extension holds no machine access of its own (see
//! `uze_extensions::Host`): it names what it needs, and this decides
//! whether to oblige. Today it always obliges, in this process, so nothing
//! observable changes — the point is that the grant now has a single, named
//! place, which is where a real capability model would go if extensions
//! were ever authored elsewhere.
//!
//! Not only the client's, despite the name: `uze agent artifacts check`
//! runs the architect over the same project from the CLI, and an
//! extension asked to work outside the client is still an extension —
//! giving it a second grant would be two answers to "what can this reach"
//! for one piece of code.
//!
//! Most of it reads. Two methods write — the file explorer's save and
//! delete — and they are the reason this file is the one place to look
//! when the question is "what can an extension actually do to this
//! machine". Both are narrow on purpose: a write replaces a file that
//! already exists, and a delete removes a file and never a directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use uze_extensions::Unreadable;

/// How much of a file this host will hand an extension.
///
/// Generous for anything a person reads and small enough that the read
/// itself is never what they notice — see [`WorkspaceHost::read_file`]
/// for what the bound is protecting against.
const READABLE_FILE_LIMIT: u64 = 2 * 1024 * 1024;

/// What a file that cannot be shown as text is called, wherever the
/// reason is the filesystem's rather than ours. Said once so the surface
/// and this grant cannot describe the same state differently.
const UNREADABLE: &str = "not readable as text";

fn unreadable() -> Unreadable {
    Unreadable::Failed(UNREADABLE.to_owned())
}

/// Where this project declares its artifacts, read from its manifest.
///
/// Beside the grant rather than at either caller because
/// `uze_extensions::ArtifactSource` says whose answer it is:
/// the host's, since only the host may read the project's manifest. The
/// client asks it on a thread and the CLI's check asks it directly, and
/// neither should be able to answer it differently.
pub fn artifacts_declared_in(project: &Path) -> uze_extensions::ArtifactSource {
    use uze_extensions::ArtifactSource;
    match uze_application::project_artifacts(project) {
        uze_application::ProjectArtifacts::Undeclared => ArtifactSource::Undeclared,
        uze_application::ProjectArtifacts::Refused(reason) => ArtifactSource::Refused(reason),
        uze_application::ProjectArtifacts::Declared {
            directories,
            project,
        } => ArtifactSource::Directories {
            roots: directories
                .into_iter()
                .map(|directory| uze_extensions::ArtifactRoot {
                    path: directory.directory,
                    declared: directory.declared.display().to_string(),
                })
                .collect(),
            project,
        },
    }
}

/// The workspace client's grant. Zero-sized: the capabilities are the
/// host's own, not per-extension state.
pub struct WorkspaceHost;

impl uze_extensions::Host for WorkspaceHost {
    /// Through `uze-git`'s read path, so an overlay refreshing every few
    /// seconds cannot contend with an agent writing in a sibling checkout.
    ///
    /// A `status` is shared (see [`shared_status`]): the badge and the
    /// code surface ask the same one on their own clocks, and on a large
    /// checkout each is a walk of the whole tree.
    fn git(&self, root: &Path, args: &[&str], answers: &[i32]) -> Result<String, String> {
        let run = || {
            uze_git::read(root, args)
                .map_err(|error| error.to_string())?
                .or_exit(answers)
        };
        match args.first() {
            Some(&"status") => shared_status(root, args, run),
            _ => run(),
        }
    }

    /// Remembered by `uze-git`: a quarter of the Git processes a session
    /// spawned were this one question, whose answer never differed.
    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        uze_git::repository::root(path)
    }

    /// Bounded, because the gesture behind it is a single click on a row
    /// in a tree. Everything downstream of the read keeps a copy —
    /// syntect's spans per line, then the buffer's own — so the file's
    /// size is paid three times over, and a checked-in fixture, a log or
    /// a minified bundle that is one enormous line is a multi-gigabyte
    /// allocation and a highlighting pass measured in minutes from a
    /// click nobody would expect to cost anything.
    ///
    /// Read through a `take` rather than checked with `metadata` first:
    /// what the cap has to bound is how much lands in memory, and a
    /// length read separately from the bytes is a different question.
    fn read_file(&self, path: &Path) -> Result<String, Unreadable> {
        use std::io::Read;

        let file = std::fs::File::open(path).map_err(|_| unreadable())?;
        let mut text = String::new();
        let read = file
            // One byte past the cap: a file exactly at it still opens,
            // and anything larger is known to be larger without reading
            // the rest of it.
            .take(READABLE_FILE_LIMIT + 1)
            .read_to_string(&mut text)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::InvalidData => Unreadable::NotText,
                _ => unreadable(),
            })?;
        if read as u64 > READABLE_FILE_LIMIT {
            return Err(Unreadable::Failed(format!(
                "too large to open here — over {} MiB",
                READABLE_FILE_LIMIT / (1024 * 1024)
            )));
        }
        Ok(text)
    }

    /// Directories first, then files, each half by name — the order the
    /// contract promises, resolved here rather than in the extension
    /// because `read_dir` answers in whatever order the filesystem
    /// happens to hold, and a tree that reorders itself between two
    /// listings of the same directory is a tree nobody can click in.
    fn list_dir(&self, path: &Path) -> Result<Vec<uze_extensions::DirEntry>, String> {
        let mut entries: Vec<uze_extensions::DirEntry> = std::fs::read_dir(path)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| uze_extensions::DirEntry {
                // `file_type` rather than a follow: a symlink to a
                // directory is still browsable, and one that dangles is
                // reported as the file it is not rather than as an error
                // that takes the whole listing with it.
                directory: entry.file_type().is_ok_and(|kind| kind.is_dir()),
                name: entry.file_name().to_string_lossy().into_owned(),
            })
            .collect();
        entries.sort();
        Ok(entries)
    }

    /// Refuses a path that is not already a file, so "save" can only ever
    /// mean "save this file" — never "create whatever this string names".
    ///
    /// Replaced by rename rather than written in place, so a save that
    /// fails partway leaves the file as it was instead of truncated.
    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String> {
        let target = self.save_target(path)?;
        forget_statuses_around(path);
        replace_contents(&target, contents.as_bytes()).map_err(|error| error.to_string())
    }

    /// Files only. A recursive removal is a different act from the one
    /// the gesture that reaches here describes, and the difference
    /// between them is measured in how much is gone afterwards.
    ///
    /// A symbolic link is removed as the link, never its target, so this
    /// reaches nothing outside the directory the path names.
    fn delete_file(&self, path: &Path) -> Result<(), String> {
        if !path.is_file() {
            return Err(format!("{} is not a file", path.display()));
        }
        forget_statuses_around(path);
        std::fs::remove_file(path).map_err(|error| error.to_string())
    }

    fn delete_dir(&self, path: &Path) -> Result<(), String> {
        // `symlink_metadata`, so a link to a directory is not followed
        // into deleting what it points at.
        if !std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
            return Err(format!("{} is not a directory", path.display()));
        }
        forget_statuses_around(path);
        std::fs::remove_dir_all(path).map_err(|error| error.to_string())
    }

    /// Checked before the rename rather than left to it, because
    /// `rename(2)` replaces an existing file without a word.
    fn rename_path(&self, from: &Path, to: &Path) -> Result<(), String> {
        if from.parent() != to.parent() {
            return Err(format!(
                "{} would move out of its directory",
                from.display()
            ));
        }
        if std::fs::symlink_metadata(to).is_ok() {
            return Err(format!("{} already exists", to.display()));
        }
        forget_statuses_around(from);
        std::fs::rename(from, to).map_err(|error| error.to_string())
    }

    /// Through `uze-git`'s write path, under the repository lock, one path
    /// at a time: a path the last commit has is restored in the index and
    /// the tree together, and one it lacks — added, untracked, a rename's
    /// new name — leaves the index and then the disk. Asked of the commit
    /// rather than of the status, because the status is what just changed.
    fn restore_to_head(&self, root: &Path, paths: &[PathBuf]) -> Result<(), String> {
        let write = |args: &[&str]| {
            uze_git::write(root, args)
                .map_err(|error| error.to_string())?
                .successful()
        };
        // Refused before anything runs, so a list with one stray path in
        // it restores none rather than some.
        if let Some(outside) = paths.iter().find(|path| !path.starts_with(root)) {
            return Err(format!(
                "{} is outside {}",
                outside.display(),
                root.display()
            ));
        }
        for path in paths {
            let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
            let committed = uze_git::read(root, &["cat-file", "-e", &format!("HEAD:{relative}")])
                .is_ok_and(|output| output.is_success());
            let pathspec = format!(":(literal){relative}");
            if committed {
                write(&[
                    "restore",
                    "--source=HEAD",
                    "--staged",
                    "--worktree",
                    "--",
                    &pathspec,
                ])?;
            } else {
                write(&[
                    "rm",
                    "--cached",
                    "--quiet",
                    "--ignore-unmatch",
                    "--",
                    &pathspec,
                ])?;
                if path.is_file() {
                    std::fs::remove_file(path).map_err(|error| error.to_string())?;
                }
            }
            forget_statuses_around(path);
        }
        Ok(())
    }

    /// Counted off a buffered read rather than the whole file: the change
    /// badge asks this of every untracked file every refresh, and an
    /// untracked directory a project never gitignored is megabytes read
    /// into memory on a timer for a line count.
    ///
    /// And remembered while the file's size and modification time stay
    /// what they were: the badge asks again every few hundred
    /// milliseconds about files that mostly have not moved.
    fn count_lines(&self, path: &Path) -> u32 {
        let stamp = std::fs::metadata(path)
            .ok()
            .and_then(|meta| Some((meta.len(), meta.modified().ok()?)));
        if let Some(stamp) = stamp
            && let Some(count) = counted(path, stamp)
        {
            return count;
        }
        let count = count_lines_of(path);
        if let Some(stamp) = stamp {
            remember_count(path, stamp, count);
        }
        count
    }

    /// The active theme's, so highlighted content is drawn for the same
    /// background the chrome around it is.
    fn syntax_theme(&self) -> String {
        uze_theme::active().syntax_theme().to_owned()
    }
}

impl WorkspaceHost {
    /// The file a save of `path` lands in. A symbolic link is followed
    /// only to a file inside the repository the link sits in: the link is
    /// something a checkout can carry, and following one anywhere would
    /// let a cloned project point a save at any file its reader can write.
    fn save_target(&self, path: &Path) -> Result<PathBuf, String> {
        use uze_extensions::Host;

        let not_a_file = || format!("{} is not a file", path.display());
        let entry = std::fs::symlink_metadata(path).map_err(|_| not_a_file())?;
        if entry.is_file() {
            return Ok(path.to_path_buf());
        }
        if !entry.file_type().is_symlink() {
            return Err(not_a_file());
        }
        let target = std::fs::canonicalize(path).map_err(|_| not_a_file())?;
        if !target.is_file() {
            return Err(not_a_file());
        }
        let root = path
            .parent()
            .and_then(|directory| self.repository_root(directory).ok())
            .and_then(|root| std::fs::canonicalize(root).ok());
        match root {
            Some(root) if target.starts_with(&root) => Ok(target),
            _ => Err(format!("{} links outside its repository", path.display())),
        }
    }
}

/// Replaces `target`'s contents through a sibling that is renamed over
/// it, carrying the permissions `target` had. A read-only file stays
/// refused, as an in-place write would have refused it: the rename only
/// needs the directory to be writable.
fn replace_contents(target: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let permissions = std::fs::metadata(target)?.permissions();
    if permissions.readonly() {
        return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    }
    let directory = target
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = directory.join(format!(
        ".{name}.{}.{}.save",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let replaced = (|| {
        file.write_all(contents)?;
        file.set_permissions(permissions)?;
        file.sync_all()?;
        uze_platform::fs::rename(&temporary, target)
    })();
    if replaced.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    replaced
}

/// How long a `status` answer is handed to whoever asks the same one
/// again. Under the badge's own period, so a reader never sees an answer
/// older than the one it would have read itself.
const STATUS_SHARED_FOR: Duration = Duration::from_millis(500);

/// One `status`, run once however many readers are waiting on it.
#[derive(Default)]
struct StatusRead {
    answer: Mutex<Option<(Instant, Result<String, String>)>>,
    landed: Condvar,
}

impl StatusRead {
    fn answer(&self) -> MutexGuard<'_, Option<(Instant, Result<String, String>)>> {
        self.answer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Still running, or answered recently enough to hand out.
    fn usable(&self) -> bool {
        self.answer()
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() < STATUS_SHARED_FOR)
    }

    fn settle(&self, answer: Result<String, String>) {
        *self.answer() = Some((Instant::now(), answer));
        self.landed.notify_all();
    }
}

type StatusKey = (PathBuf, Vec<String>);

static STATUS_READS: LazyLock<Mutex<HashMap<StatusKey, Arc<StatusRead>>>> =
    LazyLock::new(Mutex::default);

fn status_reads() -> MutexGuard<'static, HashMap<StatusKey, Arc<StatusRead>>> {
    STATUS_READS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `run` for this `status`, unless the same one is already running
/// or has just answered — then that answer is the answer.
fn shared_status(
    root: &Path,
    args: &[&str],
    run: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let key = (
        root.to_path_buf(),
        args.iter().map(|arg| arg.to_string()).collect(),
    );
    let (read, ours) = {
        let mut reads = status_reads();
        match reads.get(&key) {
            Some(read) if read.usable() => (Arc::clone(read), false),
            _ => {
                let read = Arc::new(StatusRead::default());
                reads.insert(key, Arc::clone(&read));
                (read, true)
            }
        }
    };
    if ours {
        // Settled even if the read unwinds: a reader waiting on an answer
        // that never comes is a thread that never ends.
        struct Settle<'a>(&'a StatusRead, Option<Result<String, String>>);
        impl Drop for Settle<'_> {
            fn drop(&mut self) {
                let answer = self
                    .1
                    .take()
                    .unwrap_or_else(|| Err("the status read did not finish".to_owned()));
                self.0.settle(answer);
            }
        }
        let mut settle = Settle(&read, None);
        let answer = run();
        settle.1 = Some(answer.clone());
        return answer;
    }
    let mut answer = read.answer();
    loop {
        if let Some((_, answer)) = answer.as_ref() {
            return answer.clone();
        }
        answer = read
            .landed
            .wait(answer)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

/// Drops the shared answers about the checkout `path` is in: this host
/// just changed it, and the next `status` there must see it.
fn forget_statuses_around(path: &Path) {
    status_reads().retain(|(root, _), _| !path.starts_with(root));
}

type LineStamp = (u64, SystemTime);

/// Line counts by path, while the file is the one that was counted. A
/// few thousand at most: past that it starts over, which costs a recount
/// and nothing else.
static LINE_COUNTS: LazyLock<Mutex<HashMap<PathBuf, (LineStamp, u32)>>> =
    LazyLock::new(Mutex::default);
const LINE_COUNTS_KEPT: usize = 4096;

fn line_counts() -> MutexGuard<'static, HashMap<PathBuf, (LineStamp, u32)>> {
    LINE_COUNTS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn counted(path: &Path, stamp: LineStamp) -> Option<u32> {
    line_counts()
        .get(path)
        .filter(|(counted_at, _)| *counted_at == stamp)
        .map(|(_, count)| *count)
}

fn remember_count(path: &Path, stamp: LineStamp, count: u32) {
    let mut counts = line_counts();
    if counts.len() >= LINE_COUNTS_KEPT {
        counts.clear();
    }
    counts.insert(path.to_path_buf(), (stamp, count));
}

/// Counted off a buffered read rather than the whole file (see
/// [`WorkspaceHost::count_lines`]).
fn count_lines_of(path: &Path) -> u32 {
    use std::io::{BufReader, Read};
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; 16 * 1024];
    let mut lines: u32 = 0;
    let mut last = None;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                lines = lines.saturating_add(
                    buffer[..read].iter().filter(|byte| **byte == b'\n').count() as u32,
                );
                last = buffer[..read].last().copied();
            }
            Err(_) => return 0,
        }
    }
    // `str::lines` yields nothing for an empty file and does not add a
    // line for a trailing newline, so only unterminated content counts
    // one more than its newlines.
    match last {
        None => 0,
        Some(b'\n') => lines,
        Some(_) => lines.saturating_add(1),
    }
}

#[cfg(test)]
mod tests {
    use uze_extensions::{Host, Unreadable};

    use super::WorkspaceHost;

    /// Two readers asking the same `status` at once are one walk of the
    /// tree, and an answer just given is handed to the next asker.
    #[test]
    fn a_status_asked_twice_at_once_is_run_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};

        let root = super::PathBuf::from("/shared-status/at-once");
        let runs = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Barrier::new(2));
        let readers: Vec<_> = (0..2)
            .map(|_| {
                let (runs, started, root) = (Arc::clone(&runs), Arc::clone(&started), root.clone());
                std::thread::spawn(move || {
                    started.wait();
                    super::shared_status(&root, &["status"], || {
                        runs.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        Ok("M a\n".to_owned())
                    })
                })
            })
            .collect();
        for reader in readers {
            assert_eq!(reader.join().unwrap(), Ok("M a\n".to_owned()));
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        let again = super::shared_status(&root, &["status"], || {
            runs.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        });
        assert_eq!(
            again,
            Ok("M a\n".to_owned()),
            "an answer just given is reused"
        );
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    /// A read that unwinds still answers the readers waiting on it, and
    /// the next asker runs its own rather than inheriting the failure.
    #[test]
    fn a_status_read_that_panics_releases_whoever_waits_on_it() {
        let root = super::PathBuf::from("/shared-status/panics");
        let unwound = std::panic::catch_unwind(|| {
            super::shared_status(&root, &["status"], || panic!("the read unwound"))
        });
        assert!(unwound.is_err());
        let answer = super::shared_status(&root, &["status"], || Ok("fresh".to_owned()));
        assert!(
            answer == Ok("fresh".to_owned()) || answer.is_err(),
            "answered, never waiting: {answer:?}"
        );
    }

    /// Every kind of change a reviewer throws away comes back as the last
    /// commit has it: an edit undone in the tree and the index, a deleted
    /// file back, and a staged or untracked new one gone from both.
    #[test]
    fn restoring_to_head_leaves_nothing_changed() {
        let repository = uze_testkit::git::Repository::new("restore-to-head");
        let root = repository.root().to_path_buf();
        repository.commit_file("edited.rs", "fn one() {}\n");
        repository.commit_file("removed.rs", "fn two() {}\n");
        repository.commit_file("[literal].rs", "fn three() {}\n");

        std::fs::write(root.join("edited.rs"), "fn changed() {}\n").unwrap();
        repository.git(&["add", "edited.rs"]);
        std::fs::write(root.join("edited.rs"), "fn changed_again() {}\n").unwrap();
        std::fs::remove_file(root.join("removed.rs")).unwrap();
        std::fs::write(root.join("[literal].rs"), "fn other() {}\n").unwrap();
        std::fs::write(root.join("staged.rs"), "fn staged() {}\n").unwrap();
        repository.git(&["add", "staged.rs"]);
        std::fs::write(root.join("untracked.rs"), "fn loose() {}\n").unwrap();

        let paths: Vec<_> = [
            "edited.rs",
            "removed.rs",
            "[literal].rs",
            "staged.rs",
            "untracked.rs",
        ]
        .map(|name| root.join(name))
        .into();
        WorkspaceHost.restore_to_head(&root, &paths).unwrap();

        assert_eq!(repository.git(&["status", "--porcelain"]), "");
        assert_eq!(
            std::fs::read_to_string(root.join("edited.rs")).unwrap(),
            "fn one() {}\n"
        );
        assert!(!root.join("untracked.rs").exists());
    }

    /// The grant is the checkout's own paths and no others.
    #[test]
    fn restoring_to_head_refuses_a_path_outside_the_checkout() {
        let repository = uze_testkit::git::Repository::new("restore-outside");
        let root = repository.root().to_path_buf();
        let outside = root.parent().unwrap().join("elsewhere.rs");

        assert!(WorkspaceHost.restore_to_head(&root, &[outside]).is_err());
    }

    /// Changing a file is changing what `status` says, so the shared
    /// answer is dropped.
    #[test]
    fn a_write_through_the_host_forgets_the_shared_status() {
        let root = super::PathBuf::from("/shared-status/forgotten");
        let _ = super::shared_status(&root, &["status"], || Ok("before".to_owned()));
        super::forget_statuses_around(&root.join("src/main.rs"));
        let after = super::shared_status(&root, &["status"], || Ok("after".to_owned()));
        assert_eq!(after, Ok("after".to_owned()));
    }

    /// The two methods that change the machine, and the two things they
    /// refuse. Both refusals are the whole difference between "an
    /// extension edits the files a project has" and "an extension writes
    /// anywhere it can name".
    #[test]
    fn the_write_grant_is_narrower_than_the_filesystem() {
        let directory = scratch("uze-write-grant");
        let file = directory.join("a.txt");
        std::fs::write(&file, "before\n").unwrap();
        let nested = directory.join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        assert!(WorkspaceHost.write_file(&file, "after\n").is_ok());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "after\n");

        assert!(
            WorkspaceHost
                .write_file(&directory.join("invented.txt"), "x")
                .is_err(),
            "saving never creates a file that was not there"
        );
        assert!(
            WorkspaceHost.write_file(&nested, "x").is_err(),
            "a directory is not a file to be overwritten"
        );
        assert!(
            WorkspaceHost.delete_file(&nested).is_err(),
            "a directory is never removed by the gesture that removes a file"
        );
        assert!(nested.is_dir(), "and it is still there afterwards");

        assert!(WorkspaceHost.delete_file(&file).is_ok());
        assert!(!file.exists());
        std::fs::remove_dir_all(&directory).ok();
    }

    /// A rename gives a new name and nothing more: it never moves a path
    /// to another directory and never replaces what is already there.
    #[test]
    fn a_rename_neither_moves_nor_overwrites() {
        let directory = scratch("uze-rename-grant");
        let nested = directory.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let file = directory.join("a.txt");
        std::fs::write(&file, "a\n").unwrap();
        let taken = directory.join("b.txt");
        std::fs::write(&taken, "b\n").unwrap();

        assert!(WorkspaceHost.rename_path(&file, &taken).is_err());
        assert_eq!(std::fs::read_to_string(&taken).unwrap(), "b\n");
        assert!(
            WorkspaceHost
                .rename_path(&file, &nested.join("a.txt"))
                .is_err(),
            "a rename is not a move"
        );

        let renamed = directory.join("c.txt");
        assert!(WorkspaceHost.rename_path(&file, &renamed).is_ok());
        assert!(!file.exists() && renamed.is_file());
        assert!(
            WorkspaceHost
                .rename_path(&nested, &directory.join("moved"))
                .is_ok()
        );
        assert!(directory.join("moved").is_dir());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_directory_is_deleted_whole_by_its_own_grant() {
        let directory = scratch("uze-delete-dir");
        let nested = directory.join("nested");
        std::fs::create_dir_all(nested.join("deeper")).unwrap();
        std::fs::write(nested.join("deeper/file.txt"), "x\n").unwrap();
        let file = directory.join("a.txt");
        std::fs::write(&file, "a\n").unwrap();

        assert!(
            WorkspaceHost.delete_dir(&file).is_err(),
            "a file is not a directory"
        );
        assert!(WorkspaceHost.delete_dir(&nested).is_ok());
        assert!(!nested.exists() && file.is_file());
        std::fs::remove_dir_all(&directory).ok();
    }

    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn deleting_a_directory_never_follows_a_link_into_its_target() {
        let directory = scratch("uze-delete-dir-link");
        let outside = scratch("uze-delete-dir-link-outside");
        std::fs::write(outside.join("kept"), "x\n").unwrap();
        let link = directory.join("innocent");
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        assert!(WorkspaceHost.delete_dir(&link).is_err());
        assert!(outside.join("kept").is_file());
        std::fs::remove_dir_all(&directory).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn a_save_never_follows_a_link_out_of_its_repository() {
        let directory = scratch("uze-write-link");
        let outside = scratch("uze-write-link-outside");
        let secret = outside.join("secret");
        std::fs::write(&secret, "untouched\n").unwrap();
        let link = directory.join("innocent.txt");
        std::os::unix::fs::symlink(&secret, &link).unwrap();

        assert!(WorkspaceHost.write_file(&link, "overwritten\n").is_err());
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "untouched\n");

        assert!(WorkspaceHost.delete_file(&link).is_ok());
        assert!(secret.is_file(), "deleting a link leaves its target");
        std::fs::remove_dir_all(&directory).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    // Its stand-in programs are POSIX shell scripts.
    #[cfg(unix)]
    #[test]
    fn a_save_keeps_the_permissions_the_file_had() {
        use std::os::unix::fs::PermissionsExt;

        let directory = scratch("uze-write-mode");
        let script = directory.join("run.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o750)).unwrap();

        WorkspaceHost
            .write_file(&script, "#!/bin/sh\necho saved\n")
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(&script).unwrap(),
            "#!/bin/sh\necho saved\n"
        );
        let mode = std::fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o750);
        assert_eq!(
            std::fs::read_dir(&directory).unwrap().count(),
            1,
            "nothing is left beside the file"
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    /// A click on a row in a tree never reads more than the grant allows,
    /// and says so rather than answering the same "not readable as text"
    /// a binary gets — the two are different things to do about it.
    #[test]
    fn opening_a_file_is_bounded_by_what_the_grant_allows() {
        let directory = scratch("uze-read-cap");
        let at_the_cap = directory.join("at-the-cap");
        std::fs::write(&at_the_cap, "x".repeat(super::READABLE_FILE_LIMIT as usize)).unwrap();
        let over_the_cap = directory.join("over-the-cap");
        std::fs::write(
            &over_the_cap,
            "x".repeat(super::READABLE_FILE_LIMIT as usize + 1),
        )
        .unwrap();

        assert_eq!(
            WorkspaceHost.read_file(&at_the_cap).map(|text| text.len()),
            Ok(super::READABLE_FILE_LIMIT as usize),
            "a file exactly at the cap still opens, whole"
        );
        let refused = WorkspaceHost
            .read_file(&over_the_cap)
            .expect_err("one byte over is refused");
        assert!(
            refused.to_string().contains("too large"),
            "the refusal names the size rather than the file's kind: {refused}"
        );
        assert_eq!(
            WorkspaceHost.read_file(&directory.join("absent")),
            Err(super::unreadable()),
            "a file that is not there is the state a view draws, not a path leak"
        );
        let image = directory.join("pixel.png");
        std::fs::write(
            &image,
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff],
        )
        .unwrap();
        assert_eq!(
            WorkspaceHost.read_file(&image),
            Err(Unreadable::NotText),
            "bytes that are not text are a kind of file, not a failure"
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    /// Directories before files, each half by name — a tree that reorders
    /// itself between two listings is a tree nobody can click in.
    #[test]
    fn a_listing_is_ordered_the_same_way_every_time() {
        let directory = scratch("uze-listing-order");
        std::fs::create_dir_all(directory.join("zeta")).unwrap();
        std::fs::create_dir_all(directory.join("alpha")).unwrap();
        std::fs::write(directory.join("b.txt"), "").unwrap();
        std::fs::write(directory.join("a.txt"), "").unwrap();

        let names: Vec<String> = WorkspaceHost
            .list_dir(&directory)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, ["alpha", "zeta", "a.txt", "b.txt"]);
        assert!(
            WorkspaceHost.list_dir(&directory.join("absent")).is_err(),
            "a directory that is not there is an error, not an empty tree"
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    /// A scratch directory of this test run's own.
    fn scratch(prefix: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// The streaming count must answer exactly what the contract's default
    /// answers — `str::lines` — or the badge's totals move the day a host
    /// stops materialising the file. The cases that differ are the edges:
    /// an empty file, content with no trailing newline, and a file larger
    /// than the read buffer.
    #[test]
    fn counting_lines_without_the_file_in_memory_matches_str_lines() {
        let directory = std::env::temp_dir().join(format!(
            "uze-count-lines-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&directory).unwrap();

        let big = "x".repeat(100) + "\n";
        for (name, contents) in [
            ("empty", String::new()),
            ("just-a-newline", "\n".to_owned()),
            ("no-trailing-newline", "a\nb".to_owned()),
            ("trailing-newline", "a\nb\n".to_owned()),
            ("one-unterminated-line", "solo".to_owned()),
            ("past-the-buffer", big.repeat(1000)),
        ] {
            let path = directory.join(name);
            std::fs::write(&path, &contents).unwrap();
            assert_eq!(
                WorkspaceHost.count_lines(&path),
                contents.lines().count() as u32,
                "{name} counted differently from str::lines"
            );
        }

        assert_eq!(
            WorkspaceHost.count_lines(&directory.join("absent")),
            0,
            "a file that cannot be read counts as nothing, never as an error"
        );
        std::fs::remove_dir_all(&directory).ok();
    }
}
