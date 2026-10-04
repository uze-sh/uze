//! The processes around a server: what a tab relaunches, how a server is started, reached and retired.

use super::*;

/// Common interactive-shell `comm` names, plus the server's own generic
/// "shell" placeholder before a pane's first status probe resolves —
/// recognized here purely to say "not worth trying to relaunch this by
/// name", the same judgment call `orchestrator.rs`'s sidebar used to make
/// with an identical list before agent classification took it over
/// client-side. This one is unrelated to that: naming ordinary shells is
/// general POSIX-adjacent knowledge, not the specific-harness knowledge
/// `uze-core`'s vendor-neutrality rule is actually about, so it's fine for
/// this crate to hold.
pub(super) const PLAIN_SHELL_PROCESS_NAMES: [&str; 12] = [
    "shell",
    "zsh",
    "bash",
    "sh",
    "dash",
    "fish",
    "ksh",
    "tcsh",
    "cmd",
    "powershell",
    "pwsh",
    "nu",
];

/// A best-effort relaunch command for a pane that was spawned as a shell
/// (see [`PaneRuntime::launch`]) but whose last-
/// known foreground process isn't an ordinary shell — `Some([process])` to
/// try relaunching that same program by name on restore, `None` when it
/// looks like nothing worth relaunching was there (a plain shell, or the
/// probe never resolved). Works for a shim-launched agent typed straight
/// into a "$ shell" tab specifically *because* `PaneRuntime::foreground_status`
/// already resolves such a process to its invoked alias (`claude`, not a
/// version string) via `UZE_SHIM_NAME` — this just trusts that value.
/// A name, never a path. What this reads is the *name a live process
/// reports*, and a process can choose what that says — `UZE_SHIM_NAME` is
/// an ordinary environment variable, so a script run once in a pane can
/// set it to anything. Whatever comes back here is persisted and then
/// spawned by the server on the next restart, so a candidate carrying a
/// separator (`/tmp/payload`) is refused: relaunching resolves a command
/// through `PATH` like a person typing it, and never a path this pane
/// chose.
pub(super) fn relaunch_command_for_process(process: &str) -> Option<Vec<String>> {
    let trimmed = process.trim();
    if trimmed.is_empty()
        || trimmed.contains(['/', '\\', ':'])
        || PLAIN_SHELL_PROCESS_NAMES.contains(&trimmed)
    {
        return None;
    }
    Some(vec![trimmed.to_owned()])
}

/// The binary to start a server with: this one, unless this one is no
/// longer on disk.
///
/// `current_exe` reads `/proc/self/exe`, and a binary replaced under a
/// running process — a `make install` while a client is up, which is the
/// ordinary state of this repository's own development — resolves to
/// `<path> (deleted)`. Spawning that answers `No such file or directory`,
/// from a command that never named a file: the operator is told a path is
/// missing and given no way to tell which. [`runs_this_executable`]
/// already knows this state exists; this is the other half of knowing it.
///
/// The replacement is `uze` as `PATH` resolves it — the same binary the
/// operator just installed over this one, which is the one they want
/// serving anyway.
pub(super) fn server_executable() -> Result<PathBuf, RuntimeError> {
    let current = env::current_exe()?;
    if current.exists() {
        return Ok(current);
    }
    which_uze().ok_or_else(|| {
        RuntimeError::Protocol(format!(
            "{} is gone (replaced while it ran) and no `uze` on PATH replaces it",
            current.display()
        ))
    })
}

/// `uze` as `PATH` resolves it, resolved here rather than left to the
/// shell: `Command::new("uze")` would search the *server's* environment,
/// and the server is spawned with the client's.
pub(super) fn which_uze() -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(uze_platform::executable::file_name("uze")))
        .find(|candidate| candidate.is_file())
}

pub(super) fn start_server(seat: &SpaceSeat) -> Result<(), RuntimeError> {
    let detached =
        uze_platform::process::spawn_detached(&mut server_command(&server_executable()?, seat))?;
    if !detached.outlives_host {
        tracing::warn!(
            "this terminal does not let processes leave it; the uze server will end when it closes"
        );
    }
    Ok(())
}

pub(super) fn server_command(executable: &Path, seat: &SpaceSeat) -> std::process::Command {
    let mut command = std::process::Command::new(executable);
    command
        .args(["terminal", "serve", "--root"])
        .arg(&seat.root)
        // The first `uze` usually runs inside an agent's checkout, and a
        // server left working there for its whole life would hold that
        // checkout in use long after the agent ended.
        .current_dir(host::server_directory())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        // The server outlives the command that started it, and every pane
        // inherits its environment: a trace context left here would join
        // everything run in any pane to that one command's trace.
        .env_remove("TRACEPARENT")
        .env_remove("TRACESTATE");
    command
}

/// How long a client waits for a server to answer: one that is still
/// restoring its panes, or one whose socket a cleaner took and whose
/// [`spawn_endpoint_watch`] has yet to put it back.
pub(super) const READY_WITHIN: Duration = Duration::from_secs(2);

pub(super) fn connect_waiting(socket: &Path) -> Result<Stream, RuntimeError> {
    let deadline = Instant::now() + READY_WITHIN;
    loop {
        match transport::connect(socket) {
            Ok(stream) => return Ok(stream),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(25))
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Err(RuntimeError::Protocol(
                    "terminal server did not become ready".into(),
                ));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// Whether `pid` is running `uze` — asked again right before a signal, since
/// the kernel's answer about a socket's peer is a moment old by then.
///
/// The *path* is expected to differ (an upgrade moving the binary is the
/// ordinary reason a server is being replaced), so the image's file name is
/// what is compared — through Linux's marker for a binary replaced
/// underneath a live process, which is exactly the state the server being
/// replaced is in. A process the table cannot read — a zombie, a platform
/// [`process_probe`] does not answer for — is not `uze`.
pub(super) fn runs_uze(pid: u32) -> bool {
    uze_platform::probe::executable_of(pid)
        .and_then(|image| {
            image
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .is_some_and(|name| host::is_uze_image(&name))
}

/// Whether `pid` runs the same executable image as this process. The image
/// stops resolving to this path once the binary is replaced underneath a
/// live server (a `cargo install --force` mid-session), which is exactly
/// the state a server being replaced is in.
pub(super) fn runs_this_executable(pid: u32) -> bool {
    let Some(mine) = env::current_exe().ok() else {
        return false;
    };
    uze_platform::probe::executable_of(pid)
        .is_some_and(|image| uze_platform::path::same_path(&image, &mine))
}

/// The pid listening on `socket`. The kernel stamps the listener's
/// credentials onto the connection, so this is the listener's own and not
/// something a connection could claim. `None` when nobody
/// answers, or when the platform cannot say.
pub(super) fn listening_peer(socket: &Path) -> Option<u32> {
    let stream = transport::connect(socket).ok()?;
    transport::peer_pid(&stream)
}

/// How long a server being replaced has to let go before it is made to.
pub(super) const RETIRE_WITHIN: Duration = Duration::from_secs(1);

/// Ends a server this client cannot use: asked to stop first — its
/// persisted workspace is what lets the fresh server restore the same tabs
/// — and ended outright only if it has not let go of the endpoint and the
/// claim promptly. A pid that is not running `uze` by the time it would be
/// stopped is left alone.
pub(super) fn retire(pid: u32, socket: &Path) {
    let released = || listening_peer(socket) != Some(pid) && !workspace_is_claimed();
    let channel = stop_channel(socket);
    for ask_first in [true, false] {
        if !runs_uze(pid) {
            return;
        }
        if ask_first {
            uze_platform::process::request_stop(pid, &channel);
        } else {
            uze_platform::process::terminate(pid);
        }
        let deadline = Instant::now() + RETIRE_WITHIN;
        while !released() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
        if released() {
            return;
        }
    }
}

/// The name a server listens for a stop request under: its endpoint's own
/// name, a shape no build changes, so any build can retire any other.
pub(super) fn stop_channel(socket: &Path) -> String {
    socket
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
