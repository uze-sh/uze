//! The person's endpoint: where attaching and stopping are served, out of
//! reach of what runs in an agent's pane (audit TERM-1, amending ADR-056).
//!
//! Where the kernel can seal a pane ([`uze_platform::seal`]), every pane
//! the runtime launches a program in starts sealed, and the server serves
//! the person's verbs only at an endpoint a sealed process cannot connect
//! to, whatever session, parent, environment or file it holds. Its name is
//! random per server, so nothing can take it first, and is written beside
//! the key for the person's clients. The shared endpoint keeps what a pane
//! may ask (a space, the usage) and refuses the rest while this one
//! stands. Where nothing seals, there is no such endpoint and the shared
//! one serves everything, as before.

use super::*;

/// Which endpoint a connection arrived at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Door {
    /// The socket in `state/terminal`, which every process of the user,
    /// sealed or not, reaches.
    Shared,
    /// The person's endpoint, which no sealed process reaches.
    Person,
}

/// Where the name of the running server's person endpoint is kept.
pub(super) fn operator_path() -> PathBuf {
    persisted_state_path().with_extension("operator")
}

/// The person's endpoint a server listens at.
pub(super) struct PersonDoor {
    name: String,
}

impl PersonDoor {
    /// Binds the endpoint and says where it is, or `None` where nothing
    /// seals a pane and the shared endpoint is all there is. A stale name
    /// a previous server left is removed either way, so no client looks
    /// for a server that is gone.
    pub(super) fn open(sealing: bool) -> Option<(Self, transport::Listener)> {
        let _ = fs::remove_file(operator_path());
        if !sealing {
            return None;
        }
        let token = uze_platform::secret::token().ok()?;
        let name = format!("uze-person-{}-{token}", identity_of(&uze_home_dir()));
        let listener = match uze_platform::seal::bind(&name) {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!(%error, "could not open the person's endpoint; serving everything at the shared one");
                return None;
            }
        };
        let door = Self { name };
        if let Err(error) = door.write() {
            tracing::warn!(%error, "could not say where the person's endpoint is");
            return None;
        }
        Some((door, listener))
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }

    /// Writes the name back if it is not on disk as written, like the key
    /// ([`ServerKey::keep`]).
    pub(super) fn keep(&self) {
        if published_name().as_deref() != Some(self.name.as_str())
            && let Err(error) = self.write()
        {
            tracing::warn!(%error, "could not put the person's endpoint back");
        }
    }

    fn write(&self) -> io::Result<()> {
        write_atomically(&operator_path(), self.name.as_bytes())
    }
}

/// The name the running server's person endpoint was published under.
fn published_name() -> Option<String> {
    fs::read_to_string(operator_path())
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// Reaches the server the way a person's client must: at the person's
/// endpoint when the server published one and it answers, else at the
/// shared socket. Either way, a server that runs confined while this
/// client does not is refused: it was started inside a pane, or it is
/// something in a pane standing in for the server.
pub(super) fn connect_as_person(socket: &Path) -> io::Result<Stream> {
    let stream = match published_name().map(|name| uze_platform::seal::connect(&name)) {
        Some(Ok(stream)) => stream,
        _ => transport::connect(socket)?,
    };
    if transport::peer_pid(&stream).is_some_and(host::runs_more_confined_than_this) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the uze answering runs inside a sandbox this terminal does not; \
             it was started from a pane",
        ));
    }
    Ok(stream)
}
