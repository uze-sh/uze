//! The runtime's key: what a client proves it is the person's own with,
//! rather than a process running inside one of the panes (ADR-056).
//!
//! Minted by each server when it takes the workspace claim and written
//! beside the claim, readable by this user alone. Nothing puts it in a
//! pane's environment, and a pane is never started in the directory that
//! holds it; a process that can read it anyway has stepped outside what a
//! sandboxed agent is given, and is still refused everything but opening a
//! space while the kernel places it inside a pane (see
//! [`Server::origin_of`]).

use super::*;

/// Where the running server's key is kept — beside the claim, for the
/// reasons [`workspace_lock_path`] gives.
pub(super) fn key_path() -> PathBuf {
    persisted_state_path().with_extension("key")
}

/// The key a client offers: whatever the running server last wrote, or
/// nothing when it cannot be read — a server that does not let the client
/// in then says so, rather than this guessing why.
pub fn server_key() -> String {
    fs::read_to_string(key_path())
        .map(|key| key.trim().to_owned())
        .unwrap_or_default()
}

pub(super) struct ServerKey(String);

impl ServerKey {
    /// A fresh key, written where clients read it.
    pub(super) fn mint() -> Result<Self, RuntimeError> {
        let key = Self(uze_platform::secret::token()?);
        key.write()?;
        Ok(key)
    }

    /// Writes the key back if it is not on disk as it was minted: the
    /// directory it lives in can be cleaned under a live server, and every
    /// client after that would be turned away from a server that is fine.
    pub(super) fn keep(&self) {
        if server_key() != self.0
            && let Err(error) = self.write()
        {
            tracing::warn!(%error, "could not put the terminal runtime's key back");
        }
    }

    pub(super) fn admits(&self, offered: &str) -> bool {
        uze_platform::secret::matches(offered.as_bytes(), self.0.as_bytes())
    }

    fn write(&self) -> io::Result<()> {
        write_atomically(&key_path(), self.0.as_bytes())
    }
}
