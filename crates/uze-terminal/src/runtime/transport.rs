//! The endpoint's transport: where a client reaches the server, how the
//! server accepts it, and how a server tells its endpoint is still its own
//! (ADR-038).
//!
//! A Unix-domain socket on Linux and macOS, a named pipe on Windows. The
//! protocol, the handshake and the server's lifecycle name only what this
//! module exports, so they are written once.

#[cfg(windows)]
mod pipe;
#[cfg(unix)]
mod unix;

#[cfg(windows)]
use pipe as imp;
#[cfg(unix)]
use unix as imp;

#[cfg(all(unix, test))]
pub(crate) use imp::MAX_SOCKET_PATH;
/// Reaches the server listening at `endpoint` directly: what a client
/// that must not replace that server does instead of attaching.
pub use imp::connect;
#[cfg(test)]
pub(crate) use imp::scratch_endpoint;
pub use imp::{Listener, Stream, accept, bind, clear, endpoint, identity, pair, peer_pid, restore};
