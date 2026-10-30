//! Linux: a Landlock domain scoping abstract Unix sockets, applied to one
//! thread, and an abstract socket as the endpoint it seals off.

use std::{
    io,
    os::linux::net::SocketAddrExt,
    os::unix::net::{SocketAddr, UnixListener, UnixStream},
};

use landlock::{CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetStatus, Scope};

/// The Landlock ABI that scopes abstract Unix sockets.
const SCOPING_ABI: i32 = 6;

pub(super) fn available() -> bool {
    crate::confine::landlock_abi() >= SCOPING_ABI
        && std::thread::spawn(seal_this_thread)
            .join()
            .is_ok_and(|sealed| sealed.is_ok())
}

/// Landlock and `no_new_privs` are both per thread: only this thread, and
/// what it starts from now on, is sealed.
pub(super) fn seal_this_thread() -> io::Result<()> {
    if crate::confine::landlock_abi() < SCOPING_ABI {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this kernel's Landlock does not scope abstract sockets",
        ));
    }
    let landlock = |error: landlock::RulesetError| io::Error::other(format!("Landlock: {error}"));
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .scope(Scope::AbstractUnixSocket)
        .map_err(landlock)?
        .create()
        .map_err(landlock)?
        .restrict_self()
        .map_err(landlock)?;
    if status.ruleset == RulesetStatus::FullyEnforced && status.no_new_privs {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Landlock sealed this thread only {:?}",
            status.ruleset
        )))
    }
}

fn address(name: &str) -> io::Result<SocketAddr> {
    SocketAddr::from_abstract_name(name.as_bytes())
}

pub(super) fn bind(name: &str) -> io::Result<UnixListener> {
    UnixListener::bind_addr(&address(name)?)
}

/// An abstract socket has no file and so no permissions: any user can
/// connect to one, and the server checks its peer on accept. A client
/// checks the server the same way, so a name another user took first is
/// never mistaken for this user's server.
pub(super) fn connect(name: &str) -> io::Result<UnixStream> {
    let stream = UnixStream::connect_addr(&address(name)?)?;
    // SAFETY: no arguments, and it cannot fail.
    let me = unsafe { libc::geteuid() };
    match crate::probe::socket_peer_uid(&stream) {
        Some(uid) if uid != me => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("the endpoint {name} is served by another user (uid {uid})"),
        )),
        _ => Ok(stream),
    }
}

pub(super) fn runs_confined(pid: u32) -> Option<bool> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    no_new_privs_in(&status)
}

pub(super) fn this_process_runs_confined() -> bool {
    // SAFETY: `PR_GET_NO_NEW_PRIVS` takes no pointer and answers 0 or 1.
    unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) == 1 }
}

/// The `NoNewPrivs` line of a `/proc/<pid>/status`.
fn no_new_privs_in(status: &str) -> Option<bool> {
    status.lines().find_map(|line| {
        let value = line.strip_prefix("NoNewPrivs:")?.trim();
        Some(value == "1")
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;
    use crate::seal::sealed;

    #[test]
    fn the_status_line_says_whether_a_process_runs_confined() {
        assert_eq!(
            no_new_privs_in("Name:\tsh\nNoNewPrivs:\t1\nSeccomp:\t2\n"),
            Some(true)
        );
        assert_eq!(no_new_privs_in("NoNewPrivs:\t0\n"), Some(false));
        assert_eq!(no_new_privs_in("Name:\tsh\n"), None);
    }

    /// The property TERM-1 needed: a process started from a sealed thread
    /// cannot reach the endpoint, even after it leaves its session; the
    /// thread that bound it, unsealed, can. Skipped on a kernel without
    /// the scope, where [`available`] says so.
    #[test]
    fn a_sealed_process_cannot_reach_the_endpoint_however_it_leaves() {
        if !available() {
            eprintln!("skipped: this kernel cannot seal");
            return;
        }
        let confined_before = this_process_runs_confined();
        let name = format!("uze-seal-test-{}", crate::secret::token().unwrap());
        let listener = bind(&name).unwrap();
        let served = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).unwrap();
            byte[0]
        });

        let probe = format!(
            "import socket,os\n\
             os.setsid()\n\
             s=socket.socket(socket.AF_UNIX)\n\
             try:\n  s.connect('\\0{name}'); print('reached')\n\
             except PermissionError: print('refused')\n"
        );
        let sealed = sealed(|| {
            std::process::Command::new("python3")
                .args(["-c", &probe])
                .output()
        })
        .expect("this kernel seals")
        .expect("python3 runs");
        assert_eq!(String::from_utf8_lossy(&sealed.stdout).trim(), "refused");
        assert!(
            sealed.status.success(),
            "{}",
            String::from_utf8_lossy(&sealed.stderr)
        );

        let mut outside = connect(&name).expect("the unsealed side reaches it");
        outside.write_all(b"!").unwrap();
        assert_eq!(served.join().unwrap(), b'!');
        assert_eq!(
            this_process_runs_confined(),
            confined_before,
            "sealing a thread leaves the rest of the process as it was"
        );
    }

    #[test]
    fn a_sealed_thread_marks_what_it_starts_as_confined() {
        if !available() {
            return;
        }
        let mut child = sealed(|| {
            std::process::Command::new("sleep")
                .arg("5")
                .spawn()
                .unwrap()
        })
        .unwrap();
        assert_eq!(runs_confined(child.id()), Some(true));
        let _ = child.kill();
        let _ = child.wait();
    }
}
