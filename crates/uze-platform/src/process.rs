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

/// Ends an unreaped child and everything it started. The child must have
/// been started [`in_own_group`]; it is still unreaped, so no pid in the
/// group can have been handed to anybody else.
pub fn end_group(pid: u32) {
    imp::end_group(pid)
}

/// What [`end_group`] can still do once the child itself was reaped: end
/// what it left in its group where the platform keeps the group alive past
/// its leader. Nothing where it does not, since the pid may already be
/// somebody else's.
pub fn end_reaped_group(pid: u32) {
    imp::end_reaped_group(pid)
}

/// Blocks until `pid` has exited, leaving it for `Child::wait` to reap, so
/// its group can still be ended in between.
pub fn wait_without_reaping(pid: u32) {
    imp::wait_without_reaping(pid)
}

/// Starts `command` in a group of its own, so [`end_group`] reaches it and
/// everything it starts, and a Ctrl+C meant for this terminal does not.
pub fn in_own_group(command: &mut Command) {
    imp::in_own_group(command)
}

/// Starts `command` with no terminal to ask anything on: a question it
/// would put to a person gets no answer and takes its default instead of
/// waiting for one nobody is shown.
pub fn without_terminal(command: &mut Command) {
    imp::without_terminal(command)
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

/// A pane's processes as one unit: the program a pane runs and everything
/// it starts, ended together when the pane closes.
pub struct Group(imp::Group);

impl Group {
    /// Takes `pid` (just spawned) and what it will start as one unit, or
    /// `None` where it cannot be: on Unix a program that does not lead a
    /// group of its own, which a group signal would then overreach.
    pub fn adopt(pid: u32) -> Option<Self> {
        imp::Group::adopt(pid).map(Self)
    }

    /// Ends every process in the unit.
    pub fn end(&self) {
        self.0.end()
    }

    /// The processes in the unit, oldest first, where the platform lists
    /// them; empty where it does not.
    pub fn members(&self) -> Vec<u32> {
        self.0.members()
    }
}

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;
