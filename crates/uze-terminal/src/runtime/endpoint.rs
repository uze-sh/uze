//! The endpoint: attaching to a server, finding or starting one, and the socket it listens on.

use super::*;

/// Connects to the user's one server, starting it when none answers —
/// with its first space at `seat`, which only matters for a server that has
/// nothing persisted yet. The caller then sends `Attach` naming the seat it
/// wants a space for.
pub fn attach(seat: &SpaceSeat) -> Result<Stream, RuntimeError> {
    let _span = tracing::info_span!("terminal.attach", root = %seat.root.display()).entered();
    let socket = socket_path()?;
    // Settled before connecting, and by who is behind the socket rather than
    // by what it would say: a server of another build is alive, so a connect
    // succeeds, and one built to another framing may never answer an
    // `Attach` well enough to refuse it.
    match arrival(workspace_is_claimed(), listener_at(&socket)) {
        // The claim says a server is alive. `connect_waiting` gives it the
        // two seconds its endpoint watch needs to put a wiped socket back;
        // past that the server is alive somewhere this build cannot reach
        // — an endpoint named by rules an older one used — and the
        // workspace is shut until it ends. Ending it is not a loss: the
        // shape of every space and pane is persisted, so the server that
        // replaces it restores them and its agents resume their own
        // conversations.
        Arrival::Connect => match connect_waiting(&socket) {
            Ok(stream) => return Ok(stream),
            Err(cause) => match claim_holder() {
                Some(pid) => {
                    tracing::warn!(
                        pid,
                        socket = %socket.display(),
                        "a server holds this workspace and answers nowhere this build looks; \
                         retiring it"
                    );
                    retire(pid, &socket);
                }
                None => return Err(unreachable(&socket, Some(cause))),
            },
        },
        // Another image is not another protocol. A `make install` over a
        // running server is the ordinary state of this repository's own
        // development, and it leaves every later `uze` looking at a server
        // whose binary is no longer the one on disk — while that server is
        // running somebody's agents, whose processes end with it. So it is
        // asked rather than assumed: a server that answers this build's
        // handshake can serve this build, whatever it was built from, and
        // only one that cannot is retired.
        Arrival::Ask(pid) => {
            // A server that answered a moment ago and cannot be reached
            // now is one that has just gone: taken as an unanswered
            // question rather than as an error, since the answer to that
            // is the same server started fresh.
            if serves_this_build(&socket)
                && let Ok(stream) = connect_waiting(&socket)
            {
                return Ok(stream);
            }
            tracing::warn!(
                pid,
                socket = %socket.display(),
                "the server here does not answer this build's handshake; retiring it"
            );
            retire(pid, &socket);
        }
        Arrival::Replace(pid) => retire(pid, &socket),
        Arrival::Start => {}
    }
    start_server(seat)?;
    connect_waiting(&socket)
}

/// How long the server at the endpoint has to answer whether it can serve
/// this client. Bounded because the alternative to an answer is retiring
/// it, and a server that says nothing in two seconds is a server no
/// attach can wait on.
pub(super) const ANSWERS_WITHIN: Duration = Duration::from_secs(2);

/// Whether the server at `socket` can serve this build, asked the one way
/// that can answer it: by handshaking with it.
///
/// Its image says which binary it was started from and nothing about what
/// it speaks — [`PROTOCOL_VERSION`] is what says that, and a server
/// carrying this one is a server this client can talk to however its
/// binary was built. A snapshot in answer is the proof: the server read
/// this build's `Attach`, accepted its version and described the workspace
/// in a shape this build could read back. An error, a hang-up, silence,
/// or bytes this build cannot read are each the opposite — and are what a
/// server built to another framing answers, which is the state this
/// question exists to find.
///
/// Attaches for the answer and leaves, the way [`open_space`] does: no
/// size, so no pane anybody is looking at is resized, and `Detach` before
/// the caller's own connection is made.
pub(super) fn serves_this_build(socket: &Path) -> bool {
    let Ok(mut stream) = transport::connect(socket) else {
        return false;
    };
    // Both directions: an attach must not be able to hang on a server that
    // accepted the connection and then stopped reading it, which is the
    // same failure as one that never answers.
    if stream.set_read_timeout(Some(ANSWERS_WITHIN)).is_err()
        || stream.set_write_timeout(Some(ANSWERS_WITHIN)).is_err()
    {
        return false;
    }
    let asked = send_request(
        &mut stream,
        &ClientRequest::Attach {
            version: PROTOCOL_VERSION,
            columns: 0,
            rows: 0,
            seating: Seating::WhereItLeftOff,
        },
    );
    if asked.is_err() {
        return false;
    }
    // A read timeout is per read, so a server dribbling events would renew
    // it forever: the whole question is bounded, not each answer to it.
    let deadline = Instant::now() + ANSWERS_WITHIN;
    let served = loop {
        match read_event(&mut stream) {
            Ok(Some(ClientEvent::Snapshot { .. })) => break true,
            Ok(Some(ClientEvent::Error { .. })) | Ok(None) | Err(_) => break false,
            // Anything else is a server talking, which is neither answer
            // yet — a repaint can reach a client before its snapshot does.
            Ok(Some(_)) if Instant::now() < deadline => {}
            Ok(Some(_)) => break false,
        }
    };
    let _ = send_request(&mut stream, &ClientRequest::Detach);
    served
}

/// Says what a failed connect to a claimed workspace actually means,
/// which the error from the socket cannot.
///
/// Only reached where the claim names nobody this process can act on: a
/// server older than [`record_claimant`], or a pid the process table no
/// longer vouches for. Whoever holds the workspace then has to be found
/// by hand, so the message says so rather than reporting `No such file
/// or directory` about a path the operator never typed.
pub(super) fn unreachable(socket: &Path, cause: Option<RuntimeError>) -> RuntimeError {
    let because = cause.map_or_else(String::new, |cause| format!(" ({cause})"));
    let find = host::find_server();
    RuntimeError::Protocol(format!(
        "a uze is serving this workspace and answers nowhere this build looks — not at \
         {}{because} — and the claim does not name it, so it is older than this build's \
         record of who serves. Find it with {find}, end it, and open uze again.",
        socket.display()
    ))
}

/// What [`attach`] does about the endpoint it found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Arrival {
    Connect,
    Start,
    /// Connect to the server at this pid if it can serve this build, and
    /// only otherwise end it and start one.
    Ask(u32),
    /// End the server at this pid, then start one.
    Replace(u32),
}

/// The decision [`attach`] makes, from the two facts that can prove it: who
/// holds the workspace claim, and who the kernel says is listening.
///
/// Nothing here unlinks an endpoint — only a server holding the claim does
/// (see [`serve`]) — so a listener nobody can vouch for is connected to
/// rather than taken down: a platform that cannot read the process table
/// must never cost a live session its socket. Nothing here ends a live
/// server of this workspace either: that is [`serves_this_build`]'s
/// answer to give, and only after the server has failed to give it.
pub(super) fn arrival(claimed: bool, listener: Listener) -> Arrival {
    match (claimed, listener) {
        // Alive, serving this workspace, and built from another image.
        // Which of those matters is settled by asking it, never by the
        // image: retiring a live server costs every agent it runs its
        // process, and the image is no evidence that it had to be paid.
        (true, Listener::AnotherBuild(pid)) => Arrival::Ask(pid),
        (true, _) => Arrival::Connect,
        // A server claims the workspace before it binds, so a `uze`
        // listening with the claim free is serving a workspace deleted
        // under it — `$UZE_HOME` removed while it ran. Attaching to it would
        // show spaces from a world that no longer exists.
        (false, Listener::ThisBuild(pid) | Listener::AnotherBuild(pid)) => Arrival::Replace(pid),
        (false, Listener::Nobody | Listener::Unrecognized) => Arrival::Start,
    }
}

/// Who answers at the endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Listener {
    Nobody,
    /// This very executable, and so compiled with this `PROTOCOL_VERSION`.
    ThisBuild(u32),
    /// A `uze` running another image — a `make install` over a running
    /// server, which also leaves the image it was started from reading as
    /// deleted. Which protocol it speaks is a separate question, and the
    /// only one that decides anything: see [`serves_this_build`].
    AnotherBuild(u32),
    /// A process the process table cannot vouch for as `uze`: another
    /// program, or a platform that cannot say.
    Unrecognized,
}

pub(super) fn listener_at(socket: &Path) -> Listener {
    listening_peer(socket).map_or(Listener::Nobody, identify)
}

pub(super) fn identify(pid: u32) -> Listener {
    if runs_this_executable(pid) {
        Listener::ThisBuild(pid)
    } else if runs_uze(pid) {
        Listener::AnotherBuild(pid)
    } else {
        Listener::Unrecognized
    }
}

/// Where the user's server listens — one per `UZE_HOME`, which is what
/// "user" means to UZE: a second home is a second world, with a server of
/// its own.
///
/// The endpoint goes beside the workspace it serves, under `$UZE_HOME`,
/// for the two reasons [`workspace_lock_path`] gives for the claim: a
/// cleaner that can reach it has taken the workspace too, and it is the
/// same path for every terminal, whatever their environment says.
///
/// Both halves of that were costing sessions. `XDG_RUNTIME_DIR` is
/// somebody else's variable — on WSL it is routinely set to a
/// `/run/user/<uid>` that does not exist — so the endpoint fell to the
/// temp dir, where `systemd-tmpfiles` takes it out from under a live
/// server (see [`spawn_endpoint_watch`]); and two terminals whose
/// environments disagree about `XDG_RUNTIME_DIR` or `TMPDIR` computed
/// two different endpoints for one workspace, so the second one found
/// nothing listening at a path the first had never used.
///
/// The three older candidates remain, for the one thing `$UZE_HOME`
/// cannot promise: length. `sockaddr_un.sun_path` is ~100 bytes and a
/// home is wherever the operator put it, so a path that would not fit
/// steps to the runtime directory, the system temp dir, and `/tmp` in
/// turn. Falling back does not weaken isolation: the socket is named
/// after a hash of `UZE_HOME`, so two homes stay two endpoints wherever
/// they land.
pub fn socket_path() -> Result<PathBuf, RuntimeError> {
    let home = uze_home_dir();
    Ok(transport::endpoint(transport::Address {
        directory: &home.join("state").join("terminal"),
        namespace: "uze",
        name: &format!("uze-{}", identity_of(&home)),
    })?)
}

/// Asks the running server for a space at `seat` — created when
/// none is — and answers with its label. For a `uze` started inside one of
/// the server's own panes: it must not open a client inside a client, so
/// it opens a space in the one it is already in and leaves. An error when
/// no server is running.
pub fn open_space(seat: SpaceSeat) -> Result<String, RuntimeError> {
    let _span = tracing::info_span!("terminal.open_space", root = %seat.root.display()).entered();
    let mut stream = transport::connect(&socket_path()?)
        .map_err(|_| RuntimeError::Protocol("no running uze to open a space in".into()))?;
    send_request(
        &mut stream,
        &ClientRequest::Attach {
            version: PROTOCOL_VERSION,
            columns: 0,
            rows: 0,
            seating: crate::Seating::Open(seat),
        },
    )?;
    let label = loop {
        match read_event(&mut stream)? {
            Some(ClientEvent::Snapshot { session }) => {
                break session.selected_space().label.clone();
            }
            Some(ClientEvent::Error { message }) => return Err(RuntimeError::Protocol(message)),
            Some(_) => {}
            None => return Err(RuntimeError::Protocol("the server hung up".into())),
        }
    };
    let _ = send_request(&mut stream, &ClientRequest::Detach);
    Ok(label)
}

/// Stops the user's server, answering whether there was one to stop.
///
/// "Nothing is running" is the ordinary state of this command, not a
/// failure: after a reboot, after the server exited, and — on WSL — after
/// a `/tmp` cleaner took the socket out from under a server that was
/// running. A missing socket and a socket nobody is listening on are both
/// that state, and reporting them as errors made every teardown script and
/// journey run end on a failure it was right to ignore.
pub fn stop() -> Result<bool, RuntimeError> {
    let _span = tracing::info_span!("terminal.stop").entered();
    let socket = socket_path()?;
    let mut stream = match transport::connect(&socket) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            // Nothing answers here, which is not the same as nothing
            // running: a server on an endpoint this build no longer names
            // still holds the workspace, and a `stop` that reported
            // success while it did is what left restarting the machine as
            // the only way out. What the claim names is what is stopped;
            // a claim that names nobody — a server older than the record
            // — is said, never reported as success.
            return match claim_holder() {
                Some(pid) => {
                    retire(pid, &socket);
                    Ok(true)
                }
                None if workspace_is_claimed() => Err(unreachable(&socket, None)),
                None => Ok(false),
            };
        }
        Err(error) => return Err(error.into()),
    };
    write_message(&mut stream, &ClientRequest::Stop)?;
    match read_message::<_, ClientEvent>(&mut BufReader::new(stream))? {
        Some(ClientEvent::Stopped) => Ok(true),
        Some(ClientEvent::Error { message }) => Err(RuntimeError::Protocol(message)),
        _ => Err(RuntimeError::Protocol(
            "server did not acknowledge stop".into(),
        )),
    }
}

/// A directory every pane's `PATH` starts with, named once by the binary
/// that serves. The runtime does not know what is in it; the binary puts the
/// workspace's own launchers there, so a program typed into a pane, or
/// started by one, finds them first without the operator's shell being told
/// anything.
pub(super) static PANE_PATH_FIRST: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Names the directory every pane's `PATH` starts with. Set before
/// [`serve`]; a second call is ignored, since panes already spawned were
/// given the first.
pub fn put_first_on_pane_path(directory: PathBuf) {
    let _ = PANE_PATH_FIRST.set(directory);
}

/// The program a pane's program is started through where it can join its
/// group only from inside (see [`uze_platform::process::pane::grouped`]): the
/// `uze` binary serving, whose `terminal host-pane` answers it. Unset, a
/// pane's program is started as it is.
pub(super) static PANE_HOST: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Names the binary that hosts a pane's program. Set before [`serve`].
pub fn host_panes_with(executable: PathBuf) {
    let _ = PANE_HOST.set(executable);
}

/// `inherited`, with `first` moved to its front.
pub(super) fn path_with_first(
    inherited: Option<std::ffi::OsString>,
    first: &Path,
) -> std::ffi::OsString {
    let rest = inherited
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| !uze_platform::path::same_path(entry, first));
    env::join_paths(std::iter::once(first.to_path_buf()).chain(rest))
        .unwrap_or_else(|_| first.as_os_str().to_owned())
}

/// Serves the user's one workspace. `seat` is the first space when nothing
/// is persisted yet, and is otherwise ignored.
pub fn serve(seat: SpaceSeat) -> Result<(), RuntimeError> {
    let _span = tracing::info_span!("terminal.serve", root = %seat.root.display()).entered();
    let socket = socket_path()?;
    // The workspace is claimed before the endpoint is: `Server::new` takes
    // the lock that makes this the one server restoring these spaces, so by
    // the time the socket is bound no other live server of this workspace
    // can own it — whatever sits at the path is a crashed server's
    // leftover or a server of a workspace deleted under it, and is
    // reclaimed; and a socket that later stops being the one bound here is
    // something [`spawn_endpoint_watch`] reclaims rather than a peer's.
    let (server, damage) = Server::new(seat, socket.clone())?;
    let state = Arc::new(server);
    let listener = bind_endpoint(&socket)?;
    spawn_damage_broadcaster(Arc::clone(&state), damage);
    spawn_status_ticker(Arc::clone(&state));
    spawn_endpoint_watch(Arc::clone(&state));
    let stopping = Arc::clone(&state);
    uze_platform::process::listen_for_stop(&stop_channel(&socket), move || {
        stopping.persist();
        stopping.shut_down();
    });

    let accepting = Arc::clone(&state);
    thread::spawn(move || accept_connections(listener, accepting));

    // Waited on rather than joined: after a rebind the first listener
    // blocks on an inode nothing can reach, so its loop never returns.
    // `shut_down` has already stopped every pane. The endpoint is cleared
    // under the same flag [`spawn_endpoint_watch`] holds while it decides
    // whether to rebind, so the watch cannot put it back, and a rebind in
    // progress finishes before the clearing.
    let _stopped = state.await_stop();
    transport::clear(&socket);
    Ok(())
}

/// Binds the endpoint — only ever called by the server holding the
/// workspace claim, for which nothing at the endpoint can be a live peer.
pub(super) fn bind_endpoint(socket: &Path) -> Result<transport::Listener, RuntimeError> {
    Ok(transport::bind(socket)?)
}

/// Accepts until the server stops. A failed `accept` is logged and
/// survived: returning would take every live pane down with it, for a
/// condition (a descriptor limit, an aborted handshake) that passes.
pub(super) fn accept_connections(listener: transport::Listener, server: Arc<Server>) {
    loop {
        let stream = transport::accept(&listener);
        if *server.stopped.lock().expect("stop state poisoned") {
            break;
        }
        match stream {
            Ok(stream) => {
                let client_state = Arc::clone(&server);
                thread::spawn(move || client_state.handle_client(stream));
            }
            Err(error) => {
                tracing::warn!(%error, "the terminal endpoint failed to accept a connection");
                thread::sleep(ACCEPT_RETRY_DELAY);
            }
        }
    }
}

/// How long the accept loop rests after a failed `accept`, so a descriptor
/// limit does not turn it into a busy loop.
pub(super) const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(50);
