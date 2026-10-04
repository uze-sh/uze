//! The Unix-domain socket endpoint: a file in a directory this user owns,
//! short enough for `sun_path`, and left behind by a server that crashed.

use super::*;
use crate::runtime::transport::MAX_SOCKET_PATH;
use std::os::unix::fs::PermissionsExt;

/// `XDG_RUNTIME_DIR` is somebody else's variable and can be arbitrarily
/// deep. A socket path that does not fit `sun_path` fails at `bind` with
/// an error naming the limit and not the directory — which reached a
/// user as `could not acquire package: terminal runtime I/O error: path
/// must be shorter than SUN_LEN`, from a command that has nothing to do
/// with sockets.
#[test]
fn a_runtime_directory_too_long_for_a_socket_is_stepped_over() {
    let deep = uze_testkit::temp::socket_scratch("deep").join("a".repeat(120));
    std::fs::create_dir_all(&deep).unwrap();
    let mut env = uze_testkit::env::scope();
    // Both of the candidates that come before `/tmp`: a home is
    // wherever the operator put it, and so is somebody else's
    // runtime directory.
    env.set("UZE_HOME", &deep);
    env.set("XDG_RUNTIME_DIR", &deep);

    let socket = socket_path().expect("a too-long runtime directory is not fatal");
    assert!(
        socket.as_os_str().len() <= MAX_SOCKET_PATH,
        "the chosen socket path must fit sun_path, got {} bytes: {}",
        socket.as_os_str().len(),
        socket.display()
    );
    assert!(
        !socket.starts_with(&deep),
        "the directory that could not hold the socket must not have been chosen"
    );
    // Binding is the only real proof: the length rule exists to make this
    // call succeed, so the test performs it rather than trusting the
    // arithmetic.
    let _ = std::fs::remove_file(&socket);
    let listener = std::os::unix::net::UnixListener::bind(&socket)
        .expect("the chosen path must actually bind");
    drop(listener);
    let _ = std::fs::remove_file(&socket);
    let _ = std::fs::remove_dir_all(&deep);
}

/// The server holding the workspace claim binds over whatever it finds
/// at the socket path: a crashed server's leftover file is not a peer,
/// and refusing to bind over it left the workspace unreachable.
#[test]
fn a_stale_socket_is_reclaimed_by_the_server_that_binds() {
    let scratch = uze_testkit::temp::socket_scratch("reclaim");
    std::fs::create_dir_all(&scratch).unwrap();
    let socket = scratch.join("test.sock");
    leave_a_stale_socket(&socket);
    assert!(std::os::unix::net::UnixStream::connect(&socket).is_err());

    let listener = bind_endpoint(&socket).expect("a stale socket is bound over");
    assert!(
        std::os::unix::net::UnixStream::connect(&socket).is_ok(),
        "the reclaimed endpoint answers"
    );

    drop(listener);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The endpoint directory decides where a socket carrying every pane's
/// contents lives. `create_dir_all` answers `Ok(())` for a path that is
/// already there — a symlink to somewhere else included — and
/// `set_permissions` follows symlinks, so "it exists" is not evidence
/// of anything where the name is one any local user can predict.
#[test]
fn a_runtime_directory_that_is_not_ours_to_own_is_stepped_over() {
    let scratch = uze_testkit::temp::socket_scratch("dirowner");
    let xdg = scratch.join("xdg");
    let elsewhere = scratch.join("elsewhere");
    std::fs::create_dir_all(&xdg).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    let owner = uze_platform::process::current_user().unwrap();
    let candidate = xdg.join(format!("uze-runtime-{owner}"));
    std::os::unix::fs::symlink(&elsewhere, &candidate).unwrap();

    let mut env = uze_testkit::env::scope();
    // `UZE_HOME` leads the candidates, so it is pointed somewhere too
    // long to hold a socket: what is under test is the runtime
    // directory behind it.
    env.set("UZE_HOME", scratch.join("h".repeat(120)));
    env.set("XDG_RUNTIME_DIR", &xdg);
    let socket = socket_path().expect("a bad candidate is stepped over, not fatal");
    assert!(
        !socket.starts_with(&xdg),
        "a symlinked candidate must not be adopted, got {}",
        socket.display()
    );

    // A directory this user genuinely owns is theirs to correct rather
    // than to refuse — a permissive umask on first run is the ordinary
    // way one is created too open.
    std::fs::remove_file(&candidate).unwrap();
    std::fs::create_dir_all(&candidate).unwrap();
    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o777)).unwrap();
    let socket = socket_path().expect("our own directory is usable");
    assert!(socket.starts_with(&candidate));
    let mode = std::fs::metadata(&candidate).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700, "the mode is corrected, not inherited");

    let _ = std::fs::remove_dir_all(&scratch);
}

/// Where the process [`leave_a_stale_socket`] runs binds its socket.
const STALE_SOCKET: &str = "UZE_TERMINAL_TEST_STALE_SOCKET";

/// The process side of [`leave_a_stale_socket`]. Ignored so the suite
/// never runs it on its own, and guarded by [`STALE_SOCKET`] so
/// `--include-ignored` binds nothing.
#[test]
#[ignore = "binds a socket and exits, run by leave_a_stale_socket in a process of its own"]
fn binds_a_socket_and_exits() {
    if let Some(path) = std::env::var_os(STALE_SOCKET) {
        std::os::unix::net::UnixListener::bind(path).expect("the socket path binds");
    }
}

/// A socket file nobody listens on — the shape a crashed server leaves.
///
/// Bound by a process of its own that has exited before this returns:
/// a listener this process bound and dropped is copied into every child
/// a sibling test forks until that child's `exec`, and answers a
/// connect for as long.
fn leave_a_stale_socket(path: &Path) {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "runtime::tests::socket_files::binds_a_socket_and_exits",
        ])
        .env(STALE_SOCKET, path)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("this test binary runs itself");
    assert!(status.success() && path.exists(), "{status}");
}

/// The shape a `/tmp` cleaner leaves: the file is there, the server is not.
/// A socket nobody answers is nothing to stop.
#[test]
fn stopping_at_a_socket_nobody_answers_is_not_a_failure() {
    let scratch = uze_testkit::temp::socket_scratch("stop-stale");
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &scratch);
    env.set("XDG_RUNTIME_DIR", &scratch);
    let socket = socket_path().expect("an endpoint can always be named");
    leave_a_stale_socket(&socket);

    assert!(crate::runtime::stop().is_ok());

    let _ = std::fs::remove_file(&socket);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A socket file nobody listens on names nobody, so there is nobody to
/// signal.
#[test]
fn a_socket_nobody_listens_on_names_nobody() {
    let scratch = uze_testkit::temp::socket_scratch("nobody");
    std::fs::create_dir_all(&scratch).unwrap();
    let socket = scratch.join("test.sock");
    leave_a_stale_socket(&socket);

    assert_eq!(listener_at(&socket), Listener::Nobody);

    let _ = std::fs::remove_dir_all(&scratch);
}
