//! Shared child-process discipline: every long-running child UZE spawns runs
//! in its own process group, is polled against a deadline, and — on timeout —
//! has its whole group killed, never just the direct child. Hook handlers,
//! Git acquisition, installer runners, and vendor-CLI capture all share these
//! helpers so no caller silently regresses to a bare `child.kill()`.
//!
//! Not part of the public contract; workspace-internal process plumbing.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// How much of a shell command's output is kept, **per stream**: stdout and
/// stderr are read by separate threads and capped independently, so a
/// command that fills both is reported with up to twice this much. What is
/// kept is the tail of each; whatever fell off the front is counted and
/// announced rather than silently lost.
const MAX_SHELL_OUTPUT_BYTES: usize = 64 * 1024;

/// How long a finished command's readers are given to hand over what they
/// captured. They are already at EOF in every ordinary case; this bounds the
/// one case that is not — a descendant still holding a pipe open.
const READER_GRACE: Duration = Duration::from_secs(2);

pub use uze_platform::process::{Seat, Tree, spawn_tree};

/// Polls `child` to completion but never longer than `timeout`.
///
/// Returning `(status, false)` means the process exited on its own.
/// Returning `(status, true)` means the deadline was reached: the child's
/// whole [`Tree`] was ended (twice, because a descendant forked between the
/// two can otherwise survive the first) and the direct child was reaped so
/// it cannot stay a zombie.
pub fn wait_with_timeout(
    child: &mut Child,
    tree: &Tree,
    timeout: Duration,
) -> io::Result<(ExitStatus, bool)> {
    let (status, ending) = wait_until(child, tree, timeout, || false)?;
    Ok((status, ending == Ending::TimedOut))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ending {
    Exited,
    TimedOut,
    Interrupted,
}

/// [`wait_with_timeout`] for a child seated [`Seat::NoTerminal`]: the
/// terminal's Ctrl-C no longer reaches it, so an interrupt `watch` saw ends
/// its tree the way the deadline would.
pub fn wait_with_timeout_or_interrupt(
    child: &mut Child,
    tree: &Tree,
    timeout: Duration,
    watch: &InterruptWatch,
) -> io::Result<(ExitStatus, Ending)> {
    wait_until(child, tree, timeout, || watch.interrupted())
}

fn wait_until(
    child: &mut Child,
    tree: &Tree,
    timeout: Duration,
    interrupted: impl Fn() -> bool,
) -> io::Result<(ExitStatus, Ending)> {
    let pid = child.id();
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((status, Ending::Exited)),
            Ok(None) if interrupted() || Instant::now() >= deadline => {
                let ending = if interrupted() {
                    Ending::Interrupted
                } else {
                    Ending::TimedOut
                };
                tree.end();
                // The second sweep happens while the child is dead but not
                // yet reaped: until it is, neither its pid nor its group id
                // can be handed to another process, so both can only reach
                // what this child started.
                wait_without_reaping(pid);
                tree.end();
                let status = child.wait()?;
                return Ok((status, ending));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(source) => {
                tree.end();
                let _ = child.wait();
                return Err(source);
            }
        }
    }
}

pub use uze_platform::interrupt::InterruptWatch;

fn wait_without_reaping(pid: u32) {
    uze_platform::process::wait_without_reaping(pid);
}

/// Reads a child handle to EOF, keeping the **last** `cap` bytes and
/// reporting how many were dropped off the front.
///
/// The tail, not the head: every runner a gate or a `setup` step wraps
/// writes what went wrong last and its progress noise first, so a head-kept
/// cap hands the operator 64 KiB of "Compiling …" and none of the failure
/// it was reported for.
///
/// Past the cap it keeps reading and throws the oldest bytes away rather
/// than returning. A reader that stopped early would leave the pipe
/// undrained, and the child then blocks on its next `write()` — or takes
/// `SIGPIPE` once the handle is dropped. Either way a merely chatty child
/// becomes a hang until its deadline, or a spurious failure, which is the
/// opposite of what a cap is for: the cap bounds *memory*, not how much the
/// child is allowed to say.
pub fn read_bounded<R: Read>(mut handle: R, cap: usize) -> (Vec<u8>, usize) {
    let mut bytes: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut dropped = 0usize;
    // Trimming only once the buffer has grown to twice the cap keeps the
    // cost of holding a tail linear in what the child said: each compaction
    // moves at most `cap` bytes and buys `cap` bytes of headroom.
    let slack = cap.saturating_mul(2);
    let mut trim = |bytes: &mut Vec<u8>, limit: usize| {
        if bytes.len() > limit {
            let excess = bytes.len() - cap;
            bytes.drain(..excess);
            dropped += excess;
        }
    };
    loop {
        match handle.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                bytes.extend_from_slice(&buffer[..read]);
                trim(&mut bytes, slack);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    trim(&mut bytes, cap);
    (bytes, dropped)
}

/// Whether an executable of this name is reachable through `PATH`. A
/// generated artifact may depend on a small system program (the hook
/// wrapper's `jq`); this is how a diagnostic checks for one without
/// running it.
pub fn program_on_path(program: &str) -> bool {
    crate::harness_runtime::harness_search_path()
        .iter()
        .any(|directory| {
            crate::harness_runtime::executable_candidates(directory, program)
                .iter()
                .any(|candidate| crate::harness_runtime::is_executable_file(candidate))
        })
}

/// `command` handed to the shell this platform runs authored lines in (see
/// [`uze_platform::shell`]).
pub fn shell_invocation(command: &str) -> Command {
    uze_platform::shell::command(command)
}

/// Runs a shell command in `cwd`, bounded in time and output. Returns
/// whether it exited zero, and its combined stdout and stderr — for the
/// project-declared commands (a checkout's setup, a delivery's gate, a
/// forge CLI) whose output is what the operator or the agent is told.
pub fn run_shell_bounded(cwd: &Path, command: &str, timeout: Duration) -> (bool, String) {
    let mut invocation = shell_invocation(command);
    invocation
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(cwd);
    let (mut child, tree) = match spawn_tree(&mut invocation, Seat::OwnGroup) {
        Ok(spawned) => spawned,
        Err(error) => return (false, format!("could not run `{command}`: {error}")),
    };
    // One reader per stream, never one reading them in turn: a pipe holds
    // about 64 KiB, and a gate command is stderr-heavy (every compiler and
    // test runner is). Reading stdout to EOF first would let stderr fill and
    // block the child, so stdout never reaches EOF either and the whole
    // command hangs until its deadline — reported as a timeout, with nothing
    // captured to say otherwise.
    let stdout = drain_on_thread(child.stdout.take().expect("piped"));
    let stderr = drain_on_thread(child.stderr.take().expect("piped"));
    let (status, timed_out) = match wait_with_timeout(&mut child, &tree, timeout) {
        Ok(outcome) => outcome,
        Err(error) => return (false, format!("`{command}` failed: {error}")),
    };
    // The whole group was killed if it timed out, so both pipes close and
    // the readers finish; a descendant that left the group could still hold
    // one open, so the wait is bounded rather than unconditional.
    let mut swept = false;
    let stdout = stdout.collect(&tree, &mut swept);
    let stderr = stderr.collect(&tree, &mut swept);
    let captured = combine_streams(&stdout, &stderr);
    if timed_out {
        // What the command managed to say before the deadline is usually the
        // only evidence of *why* it never finished, so it is reported rather
        // than discarded.
        let mut report = format!("`{command}` timed out after {}s", timeout.as_secs());
        if !captured.trim().is_empty() {
            report.push('\n');
            report.push_str(captured.trim());
        }
        return (false, report);
    }
    (status.success(), captured.trim().to_owned())
}

/// One stream's reader: the thread draining it, and the channel it answers
/// through so the caller can bound how long it waits.
struct Drain {
    reader: thread::JoinHandle<()>,
    answer: mpsc::Receiver<(Vec<u8>, usize)>,
}

/// What a stream had to say, or why nothing could be said for it.
enum Stream {
    Read {
        bytes: Vec<u8>,
        dropped: usize,
    },
    /// The reader never reached EOF: something still holds the pipe open.
    Unread,
}

/// Starts draining `handle` immediately, keeping the join handle so a reader
/// that cannot finish is a bounded wait rather than a thread leaked for the
/// life of the process.
fn drain_on_thread<R: Read + Send + 'static>(handle: R) -> Drain {
    let (sender, answer) = mpsc::channel();
    let reader = thread::spawn(move || {
        let _ = sender.send(read_bounded(handle, MAX_SHELL_OUTPUT_BYTES));
    });
    Drain { reader, answer }
}

impl Drain {
    /// Waits out the reader, sweeping the tree once if it cannot finish.
    ///
    /// A descendant that left the group — a `setsid`'d daemon, a dev server,
    /// a language server a suite started — still holds the pipe, so the
    /// reader never sees EOF. Killing the group is what closes it; `swept`
    /// keeps the two streams from each paying for their own kill.
    fn collect(self, tree: &Tree, swept: &mut bool) -> Stream {
        if let Ok((bytes, dropped)) = self.answer.recv_timeout(READER_GRACE) {
            // Sending is the reader's last act, so this joins a thread that
            // is already on its way out rather than waiting on one.
            let _ = self.reader.join();
            return Stream::Read { bytes, dropped };
        }
        if !*swept {
            tree.end_survivors();
            *swept = true;
        }
        match self.answer.recv_timeout(READER_GRACE) {
            Ok((bytes, dropped)) => {
                let _ = self.reader.join();
                Stream::Read { bytes, dropped }
            }
            // Joining here would block on a `read` nothing can end. The
            // thread is left to finish on its own, and the caller is told
            // the stream is missing rather than handed an empty one.
            Err(_) => Stream::Unread,
        }
    }
}

impl Stream {
    fn bytes(&self) -> &[u8] {
        match self {
            Stream::Read { bytes, .. } => bytes,
            Stream::Unread => &[],
        }
    }

    fn dropped(&self) -> usize {
        match self {
            Stream::Read { dropped, .. } => *dropped,
            Stream::Unread => 0,
        }
    }
}

/// Both streams as the operator reads them, with the two things that would
/// otherwise be a silent lie stated out loud: output that fell off the front
/// of the cap, and a stream nothing could be read from.
fn combine_streams(stdout: &Stream, stderr: &Stream) -> String {
    let mut combined = String::new();
    let dropped = stdout.dropped() + stderr.dropped();
    if dropped > 0 {
        combined.push_str(&format!(
            "... [output truncated, {dropped} bytes dropped]\n"
        ));
    }
    for (name, stream) in [("stdout", stdout), ("stderr", stderr)] {
        if matches!(stream, Stream::Unread) {
            combined.push_str(&format!(
                "... [{name} could not be read: a process is still holding it open]\n"
            ));
        }
    }
    combined.push_str(&String::from_utf8_lossy(stdout.bytes()));
    let stderr = String::from_utf8_lossy(stderr.bytes());
    if !stderr.trim().is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }
    combined
}

#[cfg(test)]
mod tests {

    // Signals a process, as only Unix does.
    #[cfg(unix)]
    #[test]
    fn an_interrupt_kills_a_child_the_terminal_no_longer_reaches() {
        let _interrupts = uze_testkit::process::interrupts();
        let watch = InterruptWatch::install();
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & sleep 30"]);
        let (mut child, tree) = spawn_tree(&mut command, Seat::NoTerminal).unwrap();
        let group = child.id();
        // SAFETY: plain `raise(3)`; the watch catches it.
        unsafe {
            libc::raise(libc::SIGINT);
        }

        let started = Instant::now();
        let (_, ending) =
            wait_with_timeout_or_interrupt(&mut child, &tree, Duration::from_secs(60), &watch)
                .unwrap();

        assert_eq!(ending, Ending::Interrupted);
        assert!(started.elapsed() < Duration::from_secs(10));
        drop(watch);
        assert!(group_is_empty(group), "the whole tree is gone");
    }

    /// Whether no process is left in group `pgid`: signal 0 to the group
    /// delivers nothing and answers `ESRCH` once it is empty, on every Unix
    /// (a `/proc` scan here found nothing on macOS, which has none, and so
    /// proved nothing there).
    // Asks the kernel about a process group, as only Unix keeps them.
    #[cfg(unix)]
    fn group_is_empty(pgid: u32) -> bool {
        // SAFETY: signal 0 checks for existence and delivers nothing.
        let answer = unsafe { libc::kill(-(pgid as libc::pid_t), 0) };
        answer == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }

    #[test]
    fn a_program_is_found_only_where_path_actually_holds_an_executable() {
        assert!(
            super::program_on_path(uze_platform::shell::ARGV[0]),
            "the system shell is on PATH"
        );
        assert!(!super::program_on_path("a-program-nobody-installed"));
    }

    use super::*;

    /// One step, spelled for each shell, as a project declares one: what is
    /// under test is how a step is run and reported, in whichever shell this
    /// platform runs it.
    fn step(posix: &str, windows: &str) -> String {
        crate::shell::ShellCommand::spelled(posix, windows)
            .here()
            .expect("the step is spelled for every platform")
            .to_owned()
    }

    #[test]
    fn read_bounded_caps_and_counts_what_it_dropped() {
        let payload = vec![b'x'; 10_000];
        let (bytes, dropped) = read_bounded(&payload[..], 4096);
        assert_eq!(bytes.len(), 4096);
        assert_eq!(dropped, 10_000 - 4096);
        let (bytes, dropped) = read_bounded(&payload[..], 100);
        assert_eq!(bytes.len(), 100);
        assert_eq!(dropped, 10_000 - 100);
    }

    #[test]
    fn read_bounded_reads_through_without_overflow() {
        let payload = vec![b'a'; 100];
        let (bytes, dropped) = read_bounded(&payload[..], 4096);
        assert_eq!(bytes, payload);
        assert_eq!(dropped, 0);
    }

    /// What a runner says last is what it failed on; what it says first is
    /// progress noise. A head-kept cap handed the operator 64 KiB of
    /// "Compiling …" and none of the failure the gate was reported for.
    #[test]
    fn read_bounded_keeps_the_end_of_a_long_stream() {
        let mut payload = b"first\n".repeat(20_000);
        payload.extend_from_slice(b"FAILURES: test_auth_redirect FAILED\n");
        let (bytes, dropped) = read_bounded(&payload[..], 4096);

        assert!(dropped > 0);
        assert!(
            String::from_utf8_lossy(&bytes).contains("test_auth_redirect FAILED"),
            "the failure line was dropped in favour of the progress noise"
        );
    }

    /// A pipe holds ~64 KiB. A command writing past that on stderr while
    /// stdout is still open blocks unless both streams are drained at once,
    /// and blocks for the command's *whole* timeout — 30 minutes for a
    /// delivery gate. Finishing at all is the assertion.
    #[test]
    fn a_stderr_heavy_command_is_not_starved_by_an_open_stdout() {
        let started = Instant::now();
        let (succeeded, output) = run_shell_bounded(
            Path::new("."),
            &step(
                "head -c 1048576 /dev/zero | tr '\\0' 'e' 1>&2; echo done",
                "[Console]::Error.Write('e' * 1048576); 'done'",
            ),
            Duration::from_secs(30),
        );
        assert!(succeeded, "the command did not exit zero: {output}");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the command was starved rather than drained"
        );
        assert!(output.contains("done"), "stdout was lost: {output}");
    }

    /// The mirror case: output past the cap must be discarded, not left in
    /// the pipe. A reader that stopped would block the child, or `SIGPIPE`
    /// it once the handle dropped — a verbose command reported as failed.
    #[test]
    fn a_command_writing_past_the_cap_still_exits_zero_with_truncated_output() {
        let (succeeded, output) = run_shell_bounded(
            Path::new("."),
            &step(
                "head -c 1048576 /dev/zero | tr '\\0' 'o'",
                "[Console]::Out.Write('o' * 1048576)",
            ),
            Duration::from_secs(30),
        );
        assert!(succeeded, "a verbose command was reported as failed");
        assert!(
            output.len() <= MAX_SHELL_OUTPUT_BYTES + 64,
            "{}",
            output.len()
        );
        assert!(!output.is_empty(), "nothing was captured");
        assert!(
            output.contains("[output truncated, "),
            "truncation was not announced: {}",
            &output[..80.min(output.len())]
        );
    }

    /// The gate the reviewer reproduced: thousands of progress lines, then
    /// the one line naming what failed. A report that keeps the head is a
    /// report with no evidence in it.
    #[test]
    fn a_failing_gate_reports_the_failure_and_not_the_progress_noise() {
        let (succeeded, output) = run_shell_bounded(
            Path::new("."),
            &step(
                "i=0; while [ $i -lt 4000 ]; do echo \"   Compiling crate-$i v0.1.0\"; \
                 i=$((i+1)); done; echo 'FAILURES: test_auth_redirect FAILED'; exit 1",
                "foreach ($i in 0..3999) { \"   Compiling crate-$i v0.1.0\" }; \
                 'FAILURES: test_auth_redirect FAILED'; exit 1",
            ),
            Duration::from_secs(60),
        );
        assert!(!succeeded);
        assert!(
            output.contains("FAILURES: test_auth_redirect FAILED"),
            "the failure line never reached the report"
        );
        assert!(
            output.contains("[output truncated, "),
            "the dropped progress output was not announced"
        );
    }

    /// A command that never finishes still had something to say about why.
    #[test]
    fn a_timed_out_command_reports_what_it_managed_to_say() {
        let (succeeded, output) = run_shell_bounded(
            Path::new("."),
            &step("echo preface; sleep 30", "'preface'; Start-Sleep 30"),
            Duration::from_millis(300),
        );
        assert!(!succeeded);
        assert!(output.contains("timed out"), "{output}");
        assert!(
            output.contains("preface"),
            "the captured tail was lost: {output}"
        );
    }

    /// A gate or `setup` step that leaves a descendant on the pipe — a dev
    /// server, a language server a suite starts — used to have its output
    /// replaced by `""` after the grace expired, so a step that *failed* was
    /// reported with no evidence at all. Sweeping the group closes the pipe.
    #[test]
    fn a_step_whose_pipe_a_survivor_holds_open_is_still_reported() {
        let started = Instant::now();
        let (succeeded, output) = run_shell_bounded(
            Path::new("."),
            &step(
                "sleep 60 & echo 'the step said this'",
                "Start-Process -NoNewWindow powershell.exe '-NoProfile','-Command','Start-Sleep 60'; \
                 'the step said this'",
            ),
            Duration::from_secs(30),
        );
        assert!(succeeded);
        assert!(
            output.contains("the step said this"),
            "the step's output was lost to a survivor on the pipe: {output:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the survivor was never swept"
        );
    }

    /// A stream nothing could be read from is a fact about the report, not
    /// an empty stream: saying so is the difference between "the step was
    /// silent" and "we could not hear it".
    #[test]
    fn a_stream_that_could_not_be_read_says_so() {
        let combined = combine_streams(
            &Stream::Read {
                bytes: b"partial".to_vec(),
                dropped: 12,
            },
            &Stream::Unread,
        );
        assert!(combined.contains("[output truncated, 12 bytes dropped]"));
        assert!(combined.contains("[stderr could not be read"));
        assert!(combined.contains("partial"));
    }

    #[test]
    fn wait_with_timeout_kills_a_hung_child() {
        let (mut child, tree) = spawn_tree(
            &mut uze_platform::shell::command(&step("sleep 30", "Start-Sleep 30")),
            Seat::OwnGroup,
        )
        .unwrap();
        let (status, timed_out) =
            wait_with_timeout(&mut child, &tree, Duration::from_millis(300)).unwrap();
        assert!(timed_out);
        assert!(!status.success());
    }

    #[test]
    fn wait_with_timeout_returns_promptly_for_a_fast_child() {
        // The shell exiting at once, not `true`: macOS ships no `/bin/true`
        // and Windows none at all, and any process that exits at once will do.
        let (mut child, tree) =
            spawn_tree(&mut uze_platform::shell::command("exit 0"), Seat::OwnGroup).unwrap();
        let (status, timed_out) =
            wait_with_timeout(&mut child, &tree, Duration::from_secs(5)).unwrap();
        assert!(!timed_out);
        assert!(status.success());
    }
}
