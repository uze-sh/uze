//! A local endpoint only this user reaches: where a client finds a server
//! of the same user, how the server accepts it, and how a server tells the
//! endpoint is still its own (ADR-038).
//!
//! A Unix-domain socket in a directory only the user can enter on Linux and
//! macOS, a named pipe whose ACL names the user's SID on Windows. A caller
//! (the terminal runtime's protocol, handshake and lifecycle) names only
//! what this module exports, so it is written once.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

/// The longest socket path the platform's `sockaddr_un` holds.
#[cfg(unix)]
pub use imp::MAX_SOCKET_PATH;
/// Reaches the server listening at `endpoint` directly: what a client
/// that must not replace that server does instead of attaching.
pub use imp::connect;
/// An endpoint at a path of the caller's choosing, for a test that runs its
/// own server.
pub use imp::scratch_endpoint;
pub use imp::{Listener, Stream, accept, bind, clear, endpoint, identity, pair, peer_pid, restore};
