//! Processes: ending one with everything it started, asking one to stop,
//! starting one detached from the terminal that launched it, and keeping a
//! pane's processes as one unit.
//!
//! A "group" is what ends together: a Unix process group, led by a child
//! started in a group of its own; on Windows, a process and its tree (or a
//! Job Object, for a pane).

use std::{
    io,
    process::{Child, Command},
};

/// Whether `pid` is still running: `None` when the platform will not say.
pub fn alive(pid: u32) -> Option<bool> {
    imp::alive(pid)
}

/// Ends `pid` outright. Only for a process the caller has just confirmed is
/// the one it means.
pub fn terminate(pid: u32) {
    imp::terminate(pid)
}

/// Asks `pid` to stop on its own terms: `SIGTERM` on Unix; on Windows the
/// stop event named `channel`, which a process waits on with
/// [`listen_for_stop`]. A request nobody answers is followed by
/// [`terminate`].
pub fn request_stop(pid: u32, channel: &str) {
    imp::request_stop(pid, channel)
}

/// Runs `on_stop` when another process calls [`request_stop`] on this one
/// through `channel`. On Unix a stop request is `SIGTERM`, whose default
/// already ends the process, so nothing is installed.
pub fn listen_for_stop(channel: &str, on_stop: impl FnOnce() + Send + 'static) {
    imp::listen_for_stop(channel, on_stop)
}

/// Where a [`Tree`] is started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Seat {
    /// A group of its own on this terminal: a Ctrl+C meant for the person's
    /// command here does not reach it.
    OwnGroup,
    /// No terminal to ask anything on: a question it would put to a person
    /// gets no answer and takes its default instead of waiting for one
    /// nobody is shown. A session of its own on Unix; on Windows a hidden
    /// console of its own, which with its standard input redirected is no
    /// terminal a program asks on (see [`crate::stdio::terminal`]). The
    /// caller redirects standard input.
    NoTerminal,
}

/// A child and every process it starts, ended as one: a process group on
/// Unix, a Job Object on Windows, which a descendant joins whether or not
/// it outlives the child. Dropping it ends nothing.
pub struct Tree(imp::Tree);

impl Tree {
    /// Ends the child and everything it started. The child must still be
    /// unreaped, so no pid in the tree can have been handed to anybody
    /// else.
    pub fn end(&self) {
        self.0.end(true)
    }

    /// What [`Tree::end`] can still do once the child itself was reaped:
    /// end what it left behind — a dev server, a language server holding
    /// its pipes.
    pub fn end_survivors(&self) {
        self.0.end(false)
    }
}

/// Spawns `command` as the root of a [`Tree`] seated as `seat`. On Windows
/// the child starts suspended and runs only once it is in the job, so
/// nothing it starts can be outside it.
pub fn spawn_tree(command: &mut Command, seat: Seat) -> io::Result<(Child, Tree)> {
    let (child, tree) =
        imp::spawn_tree(command, seat).map_err(|error| unpassable(error, command))?;
    Ok((child, Tree(tree)))
}

/// An argument the platform refuses to hand `command`'s program as it is,
/// named as such: Windows passes a `.cmd` or `.bat` its arguments through
/// `cmd.exe`, and refuses one that cannot get there unaltered rather than
/// let it change the command.
fn unpassable(error: io::Error, command: &Command) -> io::Error {
    if error.kind() != io::ErrorKind::InvalidInput {
        return error;
    }
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "an argument cannot be passed to `{}` unaltered: {error}",
            std::path::Path::new(command.get_program()).display()
        ),
    )
}

/// Blocks until `pid` has exited, leaving it for `Child::wait` to reap, so
/// its tree can still be ended in between.
pub fn wait_without_reaping(pid: u32) {
    imp::wait_without_reaping(pid)
}

/// Spawns `command` so it outlives the terminal that started it: closing
/// that terminal, or a Ctrl+C in it, does not reach the child.
pub fn spawn_detached(command: &mut Command) -> io::Result<Detached> {
    imp::spawn_detached(command)
}

/// A child started by [`spawn_detached`].
pub struct Detached {
    pub child: Child,
    /// False where the launching host would not let the child go (a
    /// Windows job that forbids breakaway): it then ends with that host,
    /// which the caller says.
    pub outlives_host: bool,
}

/// Clears `command`'s environment down to `PATH` and what a process on this
/// platform cannot run without: nothing more on Unix; on Windows the
/// system's own variables, without which Winsock does not start and no
/// HTTPS clone reaches its host.
pub fn clear_environment(command: &mut Command) {
    command.env_clear();
    for key in std::iter::once("PATH").chain(imp::SYSTEM_ENVIRONMENT.iter().copied()) {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

/// Whether `pid` is the program a launcher whose pid is `launcher` ran with
/// [`run_in_place`]: that same pid on Unix, where the launcher `exec`s; its
/// direct child on Windows, started after it — a parent pid Windows reused
/// for a later process names somebody else.
pub fn launched_by(pid: u32, launcher: u32) -> bool {
    imp::launched_by(pid, launcher)
}

/// Runs `command` in this process's place, with its exit status as this
/// process's own. Returns only when it could not be started.
pub fn run_in_place(command: &mut Command) -> io::Error {
    imp::run_in_place(command)
}

/// Processor time spent so far: by this process, and by the children it has
/// waited for where the platform keeps that account.
pub fn cpu_time() -> CpuTime {
    imp::cpu_time()
}

/// See [`cpu_time`].
#[derive(Clone, Copy, Debug)]
pub struct CpuTime {
    pub own: std::time::Duration,
    /// `None` where children are not accounted to their parent (Windows).
    pub children: Option<std::time::Duration>,
}

/// Who this process runs as, as the platform names a user: the uid, or the
/// SID on Windows.
pub fn current_user() -> io::Result<String> {
    imp::current_user()
}

/// How a person finds the running `program arguments` from their own
/// shell, to end it by hand, as a command they can type.
pub fn finding_command(program: &str, arguments: &str) -> String {
    imp::finding_command(program, arguments)
}

pub mod pane;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;
/// What the probe asks of a process here and nowhere else: whose it is,
/// which on Unix the walk already reads beside each pid.
#[cfg(windows)]
pub(crate) use windows::user_of;

#[cfg(test)]
mod tests {
    use super::*;

    /// What a detached child outlives: the console it was started from,
    /// closed. The launcher here has a console of its own, and closing it is
    /// ending its `conhost`, which takes every process attached to it, as
    /// the window's close button does. Windows only: a Unix terminal closing
    /// is a hangup, which the detached child's own session never receives.
    #[cfg(windows)]
    #[test]
    fn a_detached_child_outlives_the_console_it_was_started_from() {
        use std::io::BufRead;
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let mut launcher = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process::tests::start_a_detached_child_and_wait"])
            .args(["--ignored", "--nocapture", "--test-threads", "1"])
            .env(DETACH, "1")
            .creation_flags(CREATE_NEW_CONSOLE)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let child: u32 = std::io::BufReader::new(launcher.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok)
            .find_map(|line| line.rsplit("detached ").next()?.trim().parse().ok())
            .expect("the launcher said which child it started");
        let listed = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command"])
            .arg(format!(
                "(Get-CimInstance Win32_Process -Filter \"ParentProcessId={} AND Name='conhost.exe'\").ProcessId",
                launcher.id()
            ))
            .output()
            .unwrap();
        let console: Vec<u32> = String::from_utf8_lossy(&listed.stdout)
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect();
        assert!(!console.is_empty(), "the launcher has a console of its own");
        for host in console {
            terminate(host);
        }
        let _ = launcher.wait();
        assert_eq!(
            alive(child),
            Some(true),
            "the detached child outlived the console"
        );
        terminate(child);
    }

    const DETACH: &str = "UZE_PLATFORM_DETACH";

    /// Not a test of its own: the launcher above, which starts a detached
    /// child, says which, and waits to be ended with its console.
    #[test]
    #[ignore = "started by a_detached_child_outlives_the_console_it_was_started_from"]
    fn start_a_detached_child_and_wait() {
        if std::env::var_os(DETACH).is_none() {
            return;
        }
        let mut command = crate::shell::command("Start-Sleep 60");
        let detached = spawn_detached(&mut command).unwrap();
        println!("detached {}", detached.child.id());
        std::thread::sleep(std::time::Duration::from_secs(60));
    }

    /// A line of this platform's shell runs, and says how it ended, with no
    /// terminal to ask on: Windows PowerShell given no console at all ran
    /// nothing and exited zero.
    /// A batch file is started like any program, its arguments reaching it
    /// whole; one Windows cannot pass it unaltered is refused, by name.
    #[cfg(windows)]
    #[test]
    fn a_batch_file_runs_and_an_unpassable_argument_is_named() {
        let root = uze_testkit::temp::scratch("batch-file");
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("echo-first.cmd");
        std::fs::write(&script, "@echo off\r\necho [%~1]\r\n").unwrap();

        let mut command = Command::new(&script);
        command
            .arg("two words")
            .stdout(std::process::Stdio::piped());
        let (child, _tree) = spawn_tree(&mut command, Seat::NoTerminal).unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "[two words]"
        );

        let mut refused = Command::new(&script);
        refused.arg("line\nbreak");
        let Err(error) = spawn_tree(&mut refused, Seat::NoTerminal) else {
            panic!("an argument cmd.exe cannot carry was passed");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("echo-first.cmd"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Ending a tree ends what its root started: a grandchild the shell
    /// started and waits on is gone once the tree is. Windows only: a Job
    /// Object is what holds the grandchild there, and the Unix process
    /// group is proven by the terminal's and the subprocess tests.
    #[cfg(windows)]
    #[test]
    fn ending_a_tree_ends_the_grandchild_it_started() {
        let root = uze_testkit::temp::scratch("tree-grandchild");
        std::fs::create_dir_all(&root).unwrap();
        let recorded = root.join("grandchild.pid");
        let line = format!(
            "$p = Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep 60' \
             -PassThru -WindowStyle Hidden; Set-Content -LiteralPath {} $p.Id; Wait-Process -Id $p.Id",
            crate::shell::quote(&recorded.display().to_string())
        );
        let mut command = crate::shell::command(&line);
        command.stdin(std::process::Stdio::null());
        let (mut child, tree) = spawn_tree(&mut command, Seat::NoTerminal).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let grandchild = loop {
            if let Some(pid) = std::fs::read_to_string(&recorded)
                .ok()
                .and_then(|text| text.trim().parse::<u32>().ok())
            {
                break pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the grandchild started"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        assert_eq!(alive(grandchild), Some(true));

        tree.end();
        let _ = child.wait();
        wait_without_reaping(grandchild);
        assert_ne!(
            alive(grandchild),
            Some(true),
            "the grandchild ended with its tree"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_shell_line_with_no_terminal_still_runs() {
        let mut command = crate::shell::command("exit 7");
        command.stdin(std::process::Stdio::null());
        let (mut child, _tree) = spawn_tree(&mut command, Seat::NoTerminal).unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(7));
    }
}
