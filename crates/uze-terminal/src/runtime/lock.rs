//! Where the workspace lives on disk, and the lock that keeps one server on it.

use super::*;

/// `$UZE_HOME`, or `$HOME/.uze` — resolved directly rather than through
/// `uze-core`'s `UzeHome` so this crate's own dependency footprint stays
/// untouched. The current directory is the last resort, so a server can
/// still start in an environment with neither.
pub(super) fn uze_home_dir() -> PathBuf {
    env::var_os("UZE_HOME")
        .map(PathBuf::from)
        .or_else(|| uze_platform::home::user_home().map(|home| home.join(".uze")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Where the server persists the workspace's space/tab shape between runs —
/// deliberately not [`socket_path`]'s `XDG_RUNTIME_DIR`/temp directory
/// (that's routinely wiped on reboot, exactly the case this needs to
/// survive). One file per user under `state/terminal/`, mirroring the
/// `state/…json` layout `UzeHome::state_dir()` already uses for everything
/// else UZE persists.
pub(super) fn persisted_state_path() -> PathBuf {
    uze_home_dir()
        .join("state")
        .join("terminal")
        .join("workspace.json")
}

/// The file whose advisory lock says which process is serving the
/// persisted workspace — beside the workspace itself, under `$UZE_HOME`,
/// never in the runtime directory a `/tmp` cleaner can take away.
pub(super) fn workspace_lock_path() -> PathBuf {
    persisted_state_path().with_extension("lock")
}

/// Held for the life of a server: the proof that no other process is
/// restoring — and persisting over — the same workspace, and the one proof
/// of liveness [`attach`] trusts.
///
/// The endpoint alone cannot give that proof. `systemd-tmpfiles` wiping
/// `/tmp` under a live server takes the socket with it, so the next
/// `attach` reads "no server", starts a second one, and both restore the
/// same `workspace.json`: every agent exists twice in the same checkout,
/// and the two servers persist over each other. This lock lives where the
/// workspace lives, so a cleaner that can reach it has taken the workspace
/// too.
///
/// `flock` and not a pid file: the kernel releases it when the holder dies,
/// however it dies — a crash, a `kill -9`, a zombie nobody reaped — so a
/// stale claim is impossible by construction. A server holds it exclusively;
/// [`workspace_is_claimed`] asks with a shared lock. Panes never inherit it:
/// the descriptor is opened close-on-exec, and `portable-pty` closes every
/// descriptor above stdio in the child before `exec` besides.
pub(super) struct WorkspaceLock {
    pub(super) file: fs::File,
}

impl WorkspaceLock {
    pub(super) fn acquire() -> Result<Self, RuntimeError> {
        let mut file = open_workspace_lock()?;
        loop {
            match try_lock(&file, LockMode::Exclusive) {
                Ok(()) => {
                    record_claimant(&mut file);
                    return Ok(Self { file });
                }
                Err(LockRefusal::Interrupted) => {}
                Err(LockRefusal::Unsupported(error)) => return Err(RuntimeError::Io(error)),
                Err(LockRefusal::Contended) => {
                    if held_by_a_server(&file)? {
                        return Err(RuntimeError::Protocol(
                            "another uze terminal server is already serving this workspace".into(),
                        ));
                    }
                    // A client was asking; it lets go as soon as it has its
                    // answer.
                    thread::yield_now();
                }
            }
        }
    }
}

/// Writes this server's pid, and when it started, into the claim it has
/// just taken.
///
/// The lock alone proves a server is alive and says nothing about which
/// one, and `flock` names no holder. A client that cannot reach the
/// endpoint then has no way to end what is holding the workspace — the
/// state an operator lands in whenever the endpoint's own rules change
/// between builds, where `uze workspace stop` looked at the new endpoint,
/// found nothing, and reported nothing to stop while the old server held
/// the workspace shut. Restarting the machine was the only way out.
///
/// The start time is what makes the pid safe to act on later: a pid alone
/// names whoever the kernel handed the number to since, and a `uze` client
/// is exactly the kind of process that gets it.
///
/// Best-effort by construction: the claim is the lock, never this. What
/// is written here is a lead, and every reader corroborates it against
/// the process table before acting on it (see [`claim_holder`]).
pub(super) fn record_claimant(file: &mut fs::File) {
    let pid = std::process::id();
    let _ = file.set_len(0);
    match uze_platform::probe::started_at(pid) {
        Some(started) => {
            let _ = write!(file, "{pid} {started}");
        }
        None => {
            let _ = write!(file, "{pid}");
        }
    }
    let _ = file.flush();
}

impl WorkspaceLock {
    /// Erases the lead [`record_claimant`] wrote, once this server is done:
    /// the lock is released by the kernel when the process ends, but the
    /// bytes stay, naming a pid that will belong to somebody else.
    pub(super) fn withdraw(&self) {
        let _ = self.file.set_len(0);
    }
}

/// A server named by the claim: the pid it recorded and when that process
/// started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Claimant {
    pub(super) pid: u32,
    pub(super) started: u64,
}

/// The server the claim names, while it holds the claim and the process
/// table still says that pid is the same `uze` that recorded it. `None`
/// where the claim is free, nothing was recorded, the record carries no
/// start time, the pid died, or it was recycled — in which case the
/// caller has to say so rather than signal a stranger.
pub(super) fn claim_holder() -> Option<Claimant> {
    if !workspace_is_claimed() {
        return None;
    }
    let recorded = fs::read_to_string(workspace_lock_path()).ok()?;
    let claimant = parse_claimant(&recorded)?;
    (runs_uze(claimant.pid)
        && uze_platform::probe::started_at(claimant.pid) == Some(claimant.started))
    .then_some(claimant)
}

pub(super) fn parse_claimant(recorded: &str) -> Option<Claimant> {
    let mut fields = recorded.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let started = fields.next()?.parse().ok()?;
    fields.next().is_none().then_some(Claimant { pid, started })
}

/// Whether a live server holds the workspace claim. A filesystem that
/// cannot lock answers "claimed": the lock proves nothing there, and
/// replacing a server on no evidence would end a live session.
pub(super) fn workspace_is_claimed() -> bool {
    open_workspace_lock()
        .and_then(|file| held_by_a_server(&file))
        .unwrap_or(true)
}

/// Whether the lock on `file` is held exclusively — by a server — asked by
/// taking it shared for an instant.
///
/// Shared is how a client asks, so any number of clients ask at once, and a
/// server starting while one does is not refused as though it had met
/// another server: its exclusive attempt fails beside the asker, and this
/// question, granted beside an asker and refused beside a server, is what
/// tells the two apart.
pub(super) fn held_by_a_server(file: &fs::File) -> io::Result<bool> {
    loop {
        match try_lock(file, LockMode::Shared) {
            Ok(()) => {
                unlock(file);
                return Ok(false);
            }
            Err(LockRefusal::Interrupted) => {}
            Err(LockRefusal::Contended) => return Ok(true),
            Err(LockRefusal::Unsupported(error)) => return Err(error),
        }
    }
}

pub(super) fn open_workspace_lock() -> io::Result<fs::File> {
    let path = workspace_lock_path();
    if let Some(parent) = path.parent() {
        private_directory(parent)?;
    }
    uze_platform::fs::private_file(
        fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true),
    )
    .open(&path)
}

/// Creates `directory` for this user alone, or narrows it to them: what
/// the runtime keeps there — the workspace with every tab's command and
/// launch environment, the claim, the key — is nobody else's to read.
pub(super) fn private_directory(directory: &Path) -> io::Result<()> {
    uze_platform::fs::create_private_dir_all(directory)
}

pub(super) use uze_platform::lock::Mode as LockMode;

pub(super) fn try_lock(file: &fs::File, mode: LockMode) -> Result<(), LockRefusal> {
    uze_platform::lock::try_lock(file, mode).map_err(classify_lock_refusal)
}

pub(super) fn unlock(file: &fs::File) {
    uze_platform::lock::unlock(file);
}

/// Why `flock` said no.
///
/// Only one of its answers means another holder. A signal arriving mid-call
/// is not an answer at all, and a filesystem that cannot lock — `ENOLCK`,
/// and the `EOPNOTSUPP`/`ENOSYS` some NFS, FUSE and 9p mounts give — is a
/// different failure entirely: reading either as contention told the person
/// to go and stop a server that does not exist, permanently, with no
/// command that could clear it.
#[derive(Debug)]
pub(super) enum LockRefusal {
    Interrupted,
    Contended,
    Unsupported(io::Error),
}

pub(super) fn classify_lock_refusal(error: io::Error) -> LockRefusal {
    match error.kind() {
        io::ErrorKind::Interrupted => LockRefusal::Interrupted,
        io::ErrorKind::WouldBlock => LockRefusal::Contended,
        _ => LockRefusal::Unsupported(error),
    }
}
