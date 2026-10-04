//! The endpoint on Linux and macOS: a Unix-domain socket in a directory
//! only its user can reach.

use std::{
    env, fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

/// How long a Unix-domain socket path may be, with room to spare.
///
/// `sockaddr_un.sun_path` holds 104 bytes on macOS and 108 on Linux, and the
/// whole path has to fit or `bind` fails with `SUN_LEN` — an error naming the
/// limit and nothing about which directory exhausted it. The smaller of the
/// two, less a little, is what [`endpoint`] holds itself to, so the same
/// directory is usable on either platform.
pub const MAX_SOCKET_PATH: usize = 100;

/// Beside the workspace, under `home`, unless that path is too long for a
/// socket: then the session's runtime directory, the system temp dir and
/// `/tmp` in turn. Falling back does not weaken isolation — the socket is
/// named after `identity`, so two homes stay two endpoints wherever they
/// land.
pub fn endpoint(home: &Path, identity: &str) -> io::Result<PathBuf> {
    let named = |root: &Path| root.join(format!("uze-{identity}.sock"));
    // SAFETY: `getuid` takes no arguments and cannot fail.
    let owner = unsafe { libc::getuid() };
    let candidates = [
        home.join("state").join("terminal"),
        env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir)
            .join(format!("uze-runtime-{owner}")),
        env::temp_dir().join(format!("uze-runtime-{owner}")),
        PathBuf::from("/tmp").join(format!("uze-runtime-{owner}")),
    ];
    let mut refused = None;
    candidates
        .into_iter()
        .find(|candidate| {
            if named(candidate).as_os_str().len() > MAX_SOCKET_PATH {
                return false;
            }
            // A sandboxed terminal can expose a runtime directory while
            // denying writes below it, and a directory that already exists
            // may be somebody else's — either way the next candidate is
            // tried rather than the whole attach failing.
            match fs::create_dir_all(candidate).and_then(|()| private_directory(candidate, owner)) {
                Ok(()) => true,
                Err(error) => {
                    refused = Some(error);
                    false
                }
            }
        })
        .map(|runtime| named(&runtime))
        .ok_or_else(|| {
            refused.unwrap_or_else(|| {
                io::Error::other(
                    "no runtime directory short enough for a socket path; \
                     set XDG_RUNTIME_DIR to a shorter one",
                )
            })
        })
}

/// Binds over whatever sits at the path — only ever called by the server
/// holding the workspace claim, for which nothing there can be a live peer.
/// The socket is created inside a directory [`endpoint`] has already proven
/// to be this user's alone, so the moment between `bind` and the mode below
/// is not a window anything can walk through.
pub fn bind(endpoint: &Path) -> io::Result<Listener> {
    match fs::remove_file(endpoint) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let listener = Listener::bind(endpoint)?;
    fs::set_permissions(endpoint, fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Reaches the server at `endpoint`, and only a server of this user. The
/// directory holding the socket admits nobody else, so this is the same
/// question asked of the connection itself, as the Windows half must.
pub fn connect(endpoint: &Path) -> io::Result<Stream> {
    let stream = Stream::connect(endpoint)?;
    // SAFETY: no arguments, and it cannot fail.
    let me = unsafe { libc::geteuid() };
    match crate::probe::socket_peer_uid(&stream) {
        Some(uid) if uid != me => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} is served by another user (uid {uid})",
                endpoint.display()
            ),
        )),
        _ => Ok(stream),
    }
}

pub fn accept(listener: &Listener) -> io::Result<Stream> {
    listener.accept().map(|(stream, _)| stream)
}

pub fn peer_pid(stream: &Stream) -> Option<u32> {
    crate::probe::socket_peer(stream)
}

/// Removes the socket file a stopped server leaves.
pub fn clear(endpoint: &Path) {
    let _ = fs::remove_file(endpoint);
}

/// What tells the socket a server bound from one that replaced it at the
/// same path — the case of a `/tmp` cleaner taking it from a live server.
pub type Identity = (u64, u64);

pub fn identity(endpoint: &Path) -> Option<Identity> {
    let metadata = fs::metadata(endpoint).ok()?;
    Some((metadata.dev(), metadata.ino()))
}

/// Puts back the directory the endpoint lives in, held to the ownership
/// and mode [`endpoint`] demanded of it in the first place — a cleaner that
/// took the socket usually took the directory too.
pub fn restore(endpoint: &Path) -> io::Result<()> {
    let Some(directory) = endpoint.parent() else {
        return Ok(());
    };
    fs::create_dir_all(directory)?;
    // SAFETY: as in `endpoint`.
    private_directory(directory, unsafe { libc::getuid() })
}

/// Proves `candidate` is a directory `owner` owns and nobody else can reach
/// into — the condition for putting a socket in it that carries every
/// pane's contents and accepts input into every agent.
///
/// Existing is not evidence of anything. `create_dir_all` answers `Ok(())`
/// for a path that is already there, *including a symlink to a directory*,
/// and `set_permissions` follows symlinks. Where no `XDG_RUNTIME_DIR` is
/// set — WSL, containers, CI, any non-logind shell — the runtime directory
/// lands in a world-writable temp dir under a name any local user can
/// predict and create first. `symlink_metadata` is what asks about the
/// entry itself rather than about whatever it points at.
pub(crate) fn private_directory(candidate: &Path, owner: libc::uid_t) -> io::Result<()> {
    let metadata = fs::symlink_metadata(candidate)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other(format!(
            "{} is not a directory",
            candidate.display()
        )));
    }
    if metadata.uid() != owner {
        return Err(io::Error::other(format!(
            "{} belongs to another user",
            candidate.display()
        )));
    }
    // Ours, so a mode that lets anyone else in is ours to correct rather
    // than to refuse — this is the ordinary first-run path when the umask
    // is permissive.
    if metadata.mode() & 0o077 != 0 {
        fs::set_permissions(candidate, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Two connected ends of one connection, with no endpoint anybody else
/// can reach.
pub fn pair() -> io::Result<(Stream, Stream)> {
    Stream::pair()
}

/// An endpoint a test owns: a socket file in its scratch `directory`.
pub fn scratch_endpoint(directory: &Path, name: &str) -> PathBuf {
    directory.join(name)
}
