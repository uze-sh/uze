//! A seal between the person's endpoint and what an agent's pane runs.
//!
//! The terminal runtime serves the person's verbs at an endpoint the
//! processes it starts for agents, and every process they start, cannot
//! connect to, and the kernel is what keeps them out: a new session, a
//! reparent to init or an environment of their choosing does not change
//! it. On Linux the endpoint is an abstract Unix socket and the seal a
//! Landlock domain (ABI 6) that handles no file or network access and only
//! scopes abstract sockets, so a sealed program reads, writes and reaches
//! the network exactly as before. Elsewhere there is no seal, said by
//! [`available`], and the caller keeps the layers it had.
//!
//! Also here: whether a process runs confined at all, which a sealed pane
//! and every common harness sandbox do and which no process can undo.

use std::io;

use crate::endpoint::{Listener, Stream};

/// Whether this machine can seal what a pane runs: asked of the kernel by
/// sealing a thread that does nothing else.
pub fn available() -> bool {
    imp::available()
}

/// Runs `start` on a thread of its own, sealed first, and answers what it
/// returned: every process `start` spawns is sealed too, and so is every
/// process those start, for good. The calling thread and the rest of this
/// process are left as they were. Fails, and `start` never runs, where the
/// seal cannot be applied.
pub fn sealed<T: Send>(start: impl FnOnce() -> T + Send) -> io::Result<T> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                imp::seal_this_thread()?;
                Ok(start())
            })
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("the sealed spawn panicked")))
    })
}

/// Listens at the endpoint named `name` that no sealed process made
/// outside it can reach. Only a name nobody can predict keeps another
/// process from taking it first: the caller chooses one.
pub fn bind(name: &str) -> io::Result<Listener> {
    imp::bind(name)
}

/// Reaches the endpoint named `name`, and only one of this user: refused
/// with `PermissionDenied` from inside a seal, and when it is served by
/// another user.
pub fn connect(name: &str) -> io::Result<Stream> {
    imp::connect(name)
}

/// Whether the process `pid` runs confined: on Linux under
/// `no_new_privs`, which a sealed pane and the sandboxes harnesses use
/// (bubblewrap, Landlock, Docker's `no-new-privileges`) set and nothing
/// clears. `None` where the platform cannot say.
pub fn runs_confined(pid: u32) -> Option<bool> {
    imp::runs_confined(pid)
}

/// Whether this process runs confined (see [`runs_confined`]); `false`
/// where the platform cannot say.
pub fn this_process_runs_confined() -> bool {
    imp::this_process_runs_confined()
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod unavailable;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(not(target_os = "linux"))]
use unavailable as imp;
