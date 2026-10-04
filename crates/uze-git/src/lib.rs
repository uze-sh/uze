//! One transport for speaking to the Git binary.
//!
//! Not a domain: nothing here knows what a worktree, a checkout, or a diff
//! view is.
//!
//! # Why a crate rather than a module in `uze-core`
//!
//! Not because Git is peripheral — it is essential — but because of which
//! way the dependencies run. The domain needs it, and so do two callers
//! that may not name the domain: the workspace client's extension host,
//! which is how an extension reaches Git (presentation never names
//! `uze-core`, an enforced rule), and `uze-testkit`, which would form a
//! cycle, since `uze-core` dev-depends on it. A leaf that depends on no
//! other crate of the workspace is the only position all of them can share.
//!
//! What it owns is the part every caller was reinventing — how the process
//! is spawned, what environment it inherits, and what a non-zero exit
//! means.
//!
//! # Why a non-zero exit is not an error
//!
//! Two callers grew two incompatible conventions. `worktree` treated any
//! non-zero exit as failure; the diff view treated `1` as success, because
//! `git diff --no-index` uses it for "there are differences". Both were right for
//! their own command and wrong for the other's, and a third caller would
//! have had to guess again — `git rebase` exits 1 on a conflict, which is a
//! state, and `git rev-parse --verify --quiet` exits 1 for "no such ref",
//! which is an answer.
//!
//! So this layer does not decide. It reports what Git said and lets the
//! caller — which knows the subcommand it asked for — classify it.
//! [`Output::successful`] is there for the common case where non-zero
//! really is a failure.
//!
//! # Reads and writes are separate on purpose
//!
//! [`write`] takes the repository write lock; [`read`] never does. The
//! lock lives here and nowhere else because a lock is worthless if a
//! second module can spawn Git around it, and every call site has already
//! declared which side it is on — so a status view never blocks behind a
//! rebase.
//!
//! # The write lock
//!
//! Linked worktrees share one `.git`: `packed-refs`, branch creation,
//! `worktree add`, `prune`, `fetch`, `rebase` and `merge` are not safe
//! against each other, and the operator may have another UZE or a bare
//! `git` running. The lock is therefore inter-process — a `flock` on
//! `<common dir>/uze-write.lock`, keyed on the repository's common
//! directory so a write from inside any worktree of a repository contends
//! with every other — and it is reentrant on the thread that holds it, so
//! [`locked`] can make several writes one critical section. The kernel
//! releases a `flock` when its holder dies, so a lock left by a crashed
//! process costs nothing to reclaim. A directory that is not a repository
//! has no common directory and no lock, which is how `git init` runs.

use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

mod lock;
pub mod repository;

/// How long a write waits for the lock before giving up. Long enough for a
/// rebase or a fetch in another process, short enough that a hung holder
/// is reported rather than waited on forever.
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a command that talks to a remote may run. A stalled connection
/// otherwise holds the write lock — and every write queued behind it —
/// for as long as the network cares to stay silent; this is generous
/// enough that a large push over a slow link is never the one cut off.
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(600);

/// The subcommands that reach a remote, and with it an SSH that may want a
/// passphrase or a host-key answer.
const NETWORK_SUBCOMMANDS: &[&str] = &["fetch", "pull", "push", "clone", "ls-remote"];

/// Git could not be run at all. A Git that ran and disagreed with the
/// caller is an [`Output`], not this.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpawnError(pub(crate) String);

impl std::fmt::Display for SpawnError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SpawnError {}

/// What Git said.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Output {
    /// `None` when the process was terminated by a signal, which is never
    /// an answer to anything.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn is_success(&self) -> bool {
        self.code == Some(0)
    }

    /// Stdout when Git exited zero, trimmed stderr otherwise — the shape a
    /// caller wants when non-zero genuinely means failure for the
    /// subcommand it ran.
    pub fn successful(self) -> Result<String, String> {
        if self.is_success() {
            Ok(self.stdout)
        } else {
            Err(self.stderr.trim().to_owned())
        }
    }

    /// Stdout when Git exited zero or with one of `answers`. For a
    /// subcommand whose non-zero exit is an answer rather than a failure —
    /// `diff --no-index` reporting differences, `rev-parse --verify
    /// --quiet` reporting a missing ref.
    pub fn or_exit(self, answers: &[i32]) -> Result<String, String> {
        if self.is_success() || self.code.is_some_and(|code| answers.contains(&code)) {
            Ok(self.stdout)
        } else {
            Err(self.stderr.trim().to_owned())
        }
    }
}

/// A path as Git printed it, in this platform's spelling (Git for Windows
/// prints `C:/x/y`).
pub fn native_path(printed: &str) -> std::path::PathBuf {
    uze_platform::path::from_tool_output(printed)
}

/// Runs a Git command that only observes. Never takes the repository write
/// lock, and asks Git not to take its own optional index lock either, so a
/// status view cannot block behind — or interfere with — a write in
/// another checkout of the same repository.
pub fn read(root: &Path, args: &[&str]) -> Result<Output, SpawnError> {
    let mut command = base_command(root, args);
    command.env("GIT_OPTIONAL_LOCKS", "0");
    run(command, args)
}

/// Runs a Git command that changes the repository, under the repository
/// write lock, waiting up to [`DEFAULT_WRITE_TIMEOUT`] for it.
pub fn write(root: &Path, args: &[&str]) -> Result<Output, SpawnError> {
    write_within(root, args, DEFAULT_WRITE_TIMEOUT)
}

/// [`write`] with an explicit bound on how long to wait for the lock. A
/// wait that runs out is a [`SpawnError`] naming the lock, since Git never
/// ran.
pub fn write_within(root: &Path, args: &[&str], timeout: Duration) -> Result<Output, SpawnError> {
    let _held = lock::acquire(root, timeout)?;
    run(base_command(root, args), args)
}

/// [`write`] for a command that reads its input rather than its
/// arguments — `git apply` taking a patch, the one write whose subject is
/// too big to be an argument at all.
///
/// The stdin every other entry point nulls on purpose (a command that
/// stops to ask for a credential never gets an answer) is opened for
/// exactly this, and closed the moment the input is written, so a Git
/// that waits for more input meets the end of it rather than a terminal.
pub fn write_with_stdin(root: &Path, args: &[&str], input: &str) -> Result<Output, SpawnError> {
    use std::io::Write;

    let _held = lock::acquire(root, DEFAULT_WRITE_TIMEOUT)?;
    let mut command = base_command(root, args);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command.spawn().map_err(describe_spawn_failure)?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = input.to_owned();
    // Fed from its own thread while this one drains stdout and stderr: a
    // Git that answers before it has read everything would otherwise fill
    // its output pipe and wait on us while we wait on it. The pipe closes
    // when the thread ends, and a `git apply` reads until end of input.
    let feeder = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let output = child.wait_with_output().map_err(describe_spawn_failure)?;
    feeder
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("the input writer panicked")))
        .map_err(describe_spawn_failure)?;
    Ok(Output {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// [`write`] with variables set for this one command. For a write whose
/// result must not depend on the moment it ran — an object written only to
/// be compared, which a pinned date makes the same object every time.
pub fn write_with_env(
    root: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<Output, SpawnError> {
    let _held = lock::acquire(root, DEFAULT_WRITE_TIMEOUT)?;
    let mut command = base_command(root, args);
    command.envs(env.iter().copied());
    run(command, args)
}

/// Runs `body` with the repository write lock held throughout, so the
/// writes it makes — through [`write`], which re-enters the lock on this
/// thread — form one critical section: a prune, a name check and a
/// `worktree add` that no other process can interleave with.
pub fn locked<R>(
    root: &Path,
    timeout: Duration,
    body: impl FnOnce() -> R,
) -> Result<R, SpawnError> {
    let _held = lock::acquire(root, timeout)?;
    Ok(body())
}

fn base_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    // Git for Windows cannot open a verbatim `\\?\C:\…` root.
    command
        .arg("-C")
        .arg(uze_platform::path::strip_verbatim(root))
        .args(args);
    // A subprocess that stops to ask for a credential never gets an
    // answer: nothing here is attached to a terminal the operator can see.
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.stdin(Stdio::null());
    command
}

/// Whether `args` run one of [`NETWORK_SUBCOMMANDS`], skipping the global
/// options Git accepts before its subcommand.
fn reaches_a_remote(args: &[&str]) -> bool {
    let mut remaining = args.iter();
    while let Some(argument) = remaining.next() {
        match *argument {
            "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace" => {
                remaining.next();
            }
            option if option.starts_with('-') => {}
            subcommand => return NETWORK_SUBCOMMANDS.contains(&subcommand),
        }
    }
    false
}

fn run(command: Command, args: &[&str]) -> Result<Output, SpawnError> {
    if reaches_a_remote(args) {
        run_within(command, NETWORK_TIMEOUT)
    } else {
        run_to_completion(command)
    }
}

fn run_to_completion(mut command: Command) -> Result<Output, SpawnError> {
    let arguments = arguments_of(&command);
    // Debug, not info: this is *how* an operation was carried out, and at
    // the TUI's refresh cadences it is carried out tens of thousands of
    // times an hour. A journal recording each one buries the thing that
    // asked — which is what the journal is for — under its own machinery.
    // The operation's own span still carries what it cost;
    // `UZE_LOG=uze_git=debug` brings every invocation back.
    let span = tracing::debug_span!("git", args = %arguments, exit = tracing::field::Empty);
    let _entered = span.enter();
    let output = command.output().map_err(|error| {
        let failure = describe_spawn_failure(error);
        tracing::warn!(error = %failure.0, "git could not be run");
        failure
    })?;
    span.record("exit", output.status.code().unwrap_or(-1));
    Ok(Output {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// [`run_to_completion`] for a command that may never end on its own: past
/// `limit` Git and everything it started are ended as one tree while Git
/// is still unreaped, and the wait is reported rather than continued.
///
/// The tree has no terminal to ask on. `GIT_TERMINAL_PROMPT` silences Git,
/// not the SSH it runs, and an SSH wanting a passphrase or a host-key
/// answer opens the terminal directly — which, under the workspace client,
/// is the raw-mode terminal the operator is typing into. Without one to
/// open, it fails and says so.
fn run_within(mut command: Command, limit: Duration) -> Result<Output, SpawnError> {
    use std::time::Instant;

    let span =
        tracing::debug_span!("git", args = %arguments_of(&command), exit = tracing::field::Empty);
    let _entered = span.enter();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let (mut child, tree) =
        uze_platform::process::spawn_tree(&mut command, uze_platform::process::Seat::NoTerminal)
            .map_err(describe_spawn_failure)?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    let mut pause = Duration::from_millis(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_millis(100));
            }
            outcome => {
                tree.end();
                let _ = child.wait();
                return Err(match outcome {
                    Err(error) => describe_spawn_failure(error),
                    Ok(_) => SpawnError(format!(
                        "git did not finish within {}s and was stopped",
                        limit.as_secs()
                    )),
                });
            }
        }
    };
    span.record("exit", status.code().unwrap_or(-1));
    Ok(Output {
        code: status.code(),
        stdout: String::from_utf8_lossy(&stdout.join().unwrap_or_default()).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.join().unwrap_or_default()).into_owned(),
    })
}

/// Reads a child's pipe to its end on a thread of its own, so neither
/// pipe can fill while the other is being read.
fn drain(pipe: Option<impl io::Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn arguments_of(command: &Command) -> String {
    command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn describe_spawn_failure(error: io::Error) -> SpawnError {
    SpawnError(if error.kind() == io::ErrorKind::NotFound {
        "git is not installed or not on PATH".to_owned()
    } else {
        format!("could not run git: {error}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, time::Instant};

    /// A span's name and the `exit` it recorded.
    type SpanExit = (String, Option<i64>);

    /// The spans this crate opens, with the `exit` each recorded.
    #[derive(Clone, Default)]
    struct Recorded(std::sync::Arc<std::sync::Mutex<Vec<SpanExit>>>);

    impl<S> tracing_subscriber::Layer<S> for Recorded
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    {
        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            _id: &tracing::span::Id,
            _context: tracing_subscriber::layer::Context<'_, S>,
        ) {
            self.0
                .lock()
                .unwrap()
                .push((attrs.metadata().name().to_owned(), None));
        }

        fn on_record(
            &self,
            _id: &tracing::span::Id,
            values: &tracing::span::Record<'_>,
            _context: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut exit = ExitCode(None);
            values.record(&mut exit);
            if let Some(code) = exit.0
                && let Some(last) = self.0.lock().unwrap().last_mut()
            {
                last.1 = Some(code);
            }
        }
    }

    struct ExitCode(Option<i64>);

    impl tracing::field::Visit for ExitCode {
        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            if field.name() == "exit" {
                self.0 = Some(value);
            }
        }

        fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {}
    }

    #[test]
    fn every_invocation_is_a_span_with_its_exit_code() {
        use tracing_subscriber::layer::SubscriberExt;
        let _env = uze_testkit::env::scope();
        let root = repository("git-span");
        let recorded = Recorded::default();
        let subscriber = tracing_subscriber::registry().with(recorded.clone());
        tracing::subscriber::with_default(subscriber, || {
            read(&root, &["rev-parse", "--verify", "HEAD"]).unwrap();
            read(&root, &["rev-parse", "--verify", "no-such-ref"]).unwrap();
        });
        let spans = recorded.0.lock().unwrap().clone();
        assert_eq!(
            spans,
            vec![("git".to_owned(), Some(0)), ("git".to_owned(), Some(128))],
            "one span per invocation, each with Git's exit code"
        );
    }

    /// Every test here spawns Git, which resolves through the process-global
    /// `PATH` — including the one test that empties it. Reading ambient env
    /// is exactly what `env::scope` serializes, so a test that only reads it
    /// must still take the lock or it races the one that writes.
    /// Hand-rolled rather than `uze_testkit::git::Repository`: that fixture
    /// is built on this crate, so using it here would test the transport
    /// through itself.
    fn repository(label: &str) -> PathBuf {
        let root = uze_testkit::temp::scratch(label);
        for args in [
            vec!["init", "-q", "-b", "main", "."],
            vec!["config", "user.email", "t@example.invalid"],
            vec!["config", "user.name", "t"],
            // The developer's own `commit.gpgsign` would otherwise ask an
            // agent to sign the seed commit, hanging or failing the run.
            vec!["config", "commit.gpgsign", "false"],
        ] {
            write(&root, &args).unwrap().successful().unwrap();
        }
        std::fs::write(root.join("file"), b"seed").unwrap();
        write(&root, &["add", "."]).unwrap().successful().unwrap();
        write(&root, &["commit", "-qm", "seed"])
            .unwrap()
            .successful()
            .unwrap();
        root
    }

    #[test]
    fn a_subcommand_that_reaches_a_remote_is_found_past_the_global_options() {
        assert!(reaches_a_remote(&["fetch", "--quiet", "origin"]));
        assert!(reaches_a_remote(&["-c", "http.proxy=x", "push", "origin"]));
        assert!(reaches_a_remote(&["--no-pager", "-C", "push", "ls-remote"]));
        assert!(!reaches_a_remote(&["-c", "fetch.prune=true", "status"]));
        assert!(!reaches_a_remote(&["log", "push"]));
        assert!(!reaches_a_remote(&[]));
    }

    // Drives POSIX programs (`sh`, `sleep`) as stand-ins.
    #[cfg(unix)]
    #[test]
    fn a_command_past_its_limit_is_stopped_and_reported() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & sleep 30"]);
        let started = Instant::now();

        let outcome = run_within(command, Duration::from_millis(200));

        assert!(
            outcome.is_err_and(|error| error.0.contains("did not finish")),
            "a stalled remote is reported, not waited on"
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_push_to_a_local_remote_still_runs_to_completion() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-network-push");
        let remote = uze_testkit::temp::scratch("git-network-remote");
        write(&remote, &["init", "-q", "--bare", "."])
            .unwrap()
            .successful()
            .unwrap();
        let remote_path = remote.to_str().unwrap();

        write(&root, &["push", "--quiet", remote_path, "main"])
            .unwrap()
            .successful()
            .unwrap();

        assert!(
            read(
                &remote,
                &["rev-parse", "--verify", "--quiet", "refs/heads/main"]
            )
            .unwrap()
            .is_success()
        );
    }

    #[test]
    fn input_larger_than_a_pipe_is_fed_while_the_output_is_read() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-stdin-large");
        let input = "x".repeat(1 << 20);

        let output = write_with_stdin(&root, &["hash-object", "--stdin"], &input).unwrap();

        assert!(output.is_success());
        assert_eq!(output.stdout.trim().len(), 40);
    }

    /// The whole reason this layer exists: the same exit code means
    /// failure for one subcommand and an answer for another, so the
    /// transport reports it instead of deciding.
    #[test]
    fn a_non_zero_exit_is_reported_not_flattened_into_an_error() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-exit-codes");
        std::fs::write(root.join("file"), b"changed").unwrap();

        let diff = read(&root, &["diff", "--quiet"]).unwrap();
        assert_eq!(diff.code, Some(1), "differences, not a failure");
        assert!(diff.clone().successful().is_err());
        assert!(diff.clone().or_exit(&[1]).is_ok());
        assert!(diff.or_exit(&[]).is_err());

        let missing = read(
            &root,
            &["rev-parse", "--verify", "--quiet", "refs/heads/nope"],
        )
        .unwrap();
        assert_eq!(missing.code, Some(1));
        assert!(
            !missing.is_success(),
            "an absent ref must stay distinguishable from a present one"
        );
        assert!(
            read(
                &root,
                &["rev-parse", "--verify", "--quiet", "refs/heads/main"]
            )
            .unwrap()
            .is_success()
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stdout_and_stderr_are_both_kept() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-streams");
        let output = read(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(output.stdout.trim(), "main");
        assert!(output.stderr.is_empty());

        let failed = read(&root, &["cat-file", "-p", "definitely-not-an-object"]).unwrap();
        assert!(!failed.is_success());
        assert!(
            !failed.clone().successful().unwrap_err().is_empty(),
            "a failure keeps Git's own words: {failed:?}"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_git_is_named_rather_than_reported_as_an_io_error() {
        let mut environment = uze_testkit::env::scope();
        environment.set("PATH", uze_testkit::temp::scratch("git-absent"));
        let error = read(Path::new("."), &["status"]).unwrap_err();
        assert!(error.to_string().contains("not installed"), "{error}");
    }

    /// Lost updates are the observable failure a missing lock produces:
    /// every thread reads a counter, pauses, and writes it back, and only
    /// mutual exclusion makes the total come out right.
    #[test]
    fn concurrent_critical_sections_never_interleave() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-lock-sections");
        let counter = root.join("counter");
        std::fs::write(&counter, b"0").unwrap();
        std::thread::scope(|scope| {
            for _ in 0..6 {
                scope.spawn(|| {
                    for _ in 0..5 {
                        locked(&root, DEFAULT_WRITE_TIMEOUT, || {
                            let value: u32 = std::fs::read_to_string(&counter)
                                .unwrap()
                                .trim()
                                .parse()
                                .unwrap();
                            std::thread::sleep(Duration::from_millis(1));
                            std::fs::write(&counter, (value + 1).to_string()).unwrap();
                        })
                        .unwrap();
                    }
                });
            }
        });
        assert_eq!(std::fs::read_to_string(&counter).unwrap().trim(), "30");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The write that motivated the lock: several worktrees created at
    /// once contend on the shared refs, and each must still succeed and be
    /// registered.
    #[test]
    fn concurrent_worktree_adds_do_not_collide() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-lock-worktrees");
        let names: Vec<String> = (0..6).map(|index| format!("slot-{index}")).collect();
        std::thread::scope(|scope| {
            for name in &names {
                let root = &root;
                scope.spawn(move || {
                    let path = root.join(".worktrees").join(name);
                    write(
                        root,
                        &[
                            "worktree",
                            "add",
                            "-b",
                            name,
                            path.to_str().unwrap(),
                            "HEAD",
                        ],
                    )
                    .unwrap()
                    .successful()
                    .unwrap_or_else(|error| panic!("{name}: {error}"));
                });
            }
        });
        let listed = read(&root, &["worktree", "list", "--porcelain"])
            .unwrap()
            .successful()
            .unwrap();
        for name in &names {
            assert!(
                listed.contains(&format!("branch refs/heads/{name}")),
                "{listed}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A write inside a critical section must not deadlock against the
    /// section that contains it.
    #[test]
    fn a_write_inside_a_critical_section_re_enters_the_lock() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-lock-reentry");
        locked(&root, Duration::from_millis(500), || {
            write(&root, &["branch", "inner"])
                .unwrap()
                .successful()
                .unwrap();
            locked(&root, Duration::from_millis(500), || {
                write(&root, &["branch", "innermost"])
                    .unwrap()
                    .successful()
                    .unwrap();
            })
            .unwrap();
        })
        .unwrap();
        let branches = read(&root, &["branch", "--list"])
            .unwrap()
            .successful()
            .unwrap();
        assert!(
            branches.contains("inner") && branches.contains("innermost"),
            "{branches}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_panic_inside_the_critical_section_releases_the_lock() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-lock-panic");
        let outcome = std::panic::catch_unwind(|| {
            let _ = locked(&root, DEFAULT_WRITE_TIMEOUT, || panic!("inside"));
        });
        assert!(outcome.is_err());
        locked(&root, Duration::from_millis(500), || ()).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_busy_lock_is_reported_by_name_after_the_timeout_and_reads_never_wait() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-lock-busy");
        let root_for_holder = root.clone();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                locked(&root_for_holder, DEFAULT_WRITE_TIMEOUT, || {
                    held_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                })
                .unwrap();
            });
            held_rx.recv().unwrap();

            // The holder releases only after this returns, so a read that
            // waited on the lock would never come back at all.
            assert!(
                read(&root, &["status", "--porcelain"])
                    .unwrap()
                    .is_success(),
                "a read runs while the lock is held"
            );

            let error = write_within(&root, &["branch", "blocked"], Duration::from_millis(150))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("write lock") && error.contains("busy"),
                "{error}"
            );
            release_tx.send(()).unwrap();
        });
        write(&root, &["branch", "after"])
            .unwrap()
            .successful()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Holds the lock until killed; the process side of the test below.
    /// Ignored so it never runs on its own.
    #[test]
    #[ignore]
    fn hold_the_lock_until_killed() {
        let Some(root) = std::env::var_os("UZE_GIT_LOCK_HOLD") else {
            return;
        };
        let root = PathBuf::from(root);
        locked(&root, DEFAULT_WRITE_TIMEOUT, || {
            std::fs::write(root.join("held"), b"").unwrap();
            std::thread::sleep(Duration::from_secs(120));
        })
        .unwrap();
    }

    #[test]
    fn a_lock_held_by_a_dead_process_is_reclaimed() {
        let _environment = uze_testkit::env::scope();
        let root = repository("git-lock-dead-holder");
        let mut holder = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::hold_the_lock_until_killed", "--ignored"])
            .env("UZE_GIT_LOCK_HOLD", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("held").exists() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "holder never took the lock"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            write_within(&root, &["branch", "blocked"], Duration::from_millis(150)).is_err(),
            "the lock is held across processes"
        );
        holder.kill().unwrap();
        holder.wait().unwrap();
        write_within(&root, &["branch", "reclaimed"], Duration::from_secs(5))
            .unwrap()
            .successful()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_directory_outside_a_repository_answers_rather_than_failing_to_spawn() {
        let _environment = uze_testkit::env::scope();
        let root = uze_testkit::temp::scratch("git-norepo");
        let output = read(&root, &["rev-parse", "--show-toplevel"]).unwrap();
        assert!(!output.is_success());
        std::fs::remove_dir_all(root).unwrap();
    }
}
