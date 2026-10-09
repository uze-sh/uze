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
/// Whether the process at the other end of an accepted connection runs as
/// another user, as the kernel recorded it: true only when the platform
/// says so. The endpoint's own permissions already admit nobody else; a
/// server asks this as well so that a permission somebody loosened is not
/// the only thing between another account and every pane.
pub use imp::peer_is_another_user;
/// An endpoint at a path of the caller's choosing, for a test that runs its
/// own server.
pub use imp::scratch_endpoint;
pub use imp::{Listener, Stream, accept, bind, clear, endpoint, identity, pair, peer_pid, restore};

/// Where an endpoint is to be found, as its caller names it: this crate
/// knows how to make one only this user reaches, and nothing of whose it
/// is or what it is called.
#[derive(Clone, Copy, Debug)]
pub struct Address<'a> {
    /// Where it belongs, when the platform can put it there: a socket
    /// path has a length limit a directory chosen by a person can pass.
    pub directory: &'a std::path::Path,
    /// What names the private directories it falls back to, one per user.
    pub namespace: &'a str,
    /// The endpoint's own name, which tells two of them apart wherever
    /// they land.
    pub name: &'a str,
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    /// One connection read and written at once from several threads, as
    /// the terminal's client and server do (a reader thread, a writer
    /// thread and requests), carries every byte in order each way. A
    /// connection whose two directions share state (the overlapped pipe's
    /// handle) would lose or mix data here first.
    /// Not a test of its own: connects to the endpoint `UZE_ENDPOINT_PROBE`
    /// names and prints what came of it, for a check driven from outside
    /// with a server of another account.
    #[test]
    #[ignore = "driven by hand against another account's server"]
    fn connect_to_the_endpoint_named_in_the_environment() {
        let Some(endpoint) = std::env::var_os("UZE_ENDPOINT_PROBE") else {
            return;
        };
        match super::connect(std::path::Path::new(&endpoint)) {
            Ok(_) => println!("connected"),
            Err(error) => println!("refused: {:?}: {error}", error.kind()),
        }
    }

    /// A shared runtime name another account created first — a file, a
    /// directory of theirs — is stepped over to a directory nobody could
    /// have predicted, and every later caller for the same address finds
    /// that same one.
    #[cfg(unix)]
    #[test]
    fn a_shared_runtime_name_somebody_else_took_does_not_shut_this_user_out() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-taken");
        let deep = scratch.join("a".repeat(120));
        std::fs::create_dir_all(&deep).unwrap();
        let mut env = uze_testkit::env::scope();
        env.set("XDG_RUNTIME_DIR", &scratch);
        let namespace = format!("uze-taken-{}", std::process::id());
        // SAFETY: no arguments, and it cannot fail.
        let uid = unsafe { libc::getuid() };
        let shared = format!("{namespace}-runtime-{uid}");
        let taken = [
            scratch.join(&shared),
            std::env::temp_dir().join(&shared),
            std::path::Path::new("/tmp").join(&shared),
        ];
        for path in &taken {
            let _ = std::fs::write(path, b"");
        }
        let address = super::Address {
            directory: &deep,
            namespace: &namespace,
            name: "w",
        };

        let chosen = super::endpoint(address).expect("a taken name is not fatal");
        let directory = chosen.parent().unwrap().to_path_buf();
        assert!(!taken.contains(&directory));
        assert!(chosen.as_os_str().len() <= super::MAX_SOCKET_PATH);
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(&directory).unwrap().permissions()
            ) & 0o777,
            0o700
        );
        assert_eq!(super::endpoint(address).unwrap(), chosen);
        drop(super::bind(&chosen).expect("and it binds"));

        for path in &taken {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_dir_all(&directory);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_connection_read_and_written_at_once_carries_every_byte() {
        const FRAMES: usize = 2_000;
        const FRAME: usize = 1_024;
        let scratch = uze_testkit::temp::socket_scratch("endpoint-stress");
        std::fs::create_dir_all(&scratch).unwrap();
        let endpoint = super::scratch_endpoint(&scratch, "stress.sock");
        let listener = super::bind(&endpoint).unwrap();
        // The server echoes what it reads, reading and writing on two
        // threads over clones of one connection.
        let server = std::thread::spawn(move || {
            let incoming = super::accept(&listener).unwrap();
            let mut outgoing = incoming.try_clone().unwrap();
            let mut incoming = incoming;
            let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();
            let writer = std::thread::spawn(move || {
                for frame in receiver {
                    outgoing.write_all(&frame).unwrap();
                }
            });
            let mut frame = vec![0u8; FRAME];
            for _ in 0..FRAMES {
                incoming.read_exact(&mut frame).unwrap();
                sender.send(frame.clone()).unwrap();
            }
            drop(sender);
            writer.join().unwrap();
        });

        let client = super::connect(&endpoint).unwrap();
        let mut reading = client.try_clone().unwrap();
        let mut writing = client;
        let writer = std::thread::spawn(move || {
            for index in 0..FRAMES {
                writing
                    .write_all(&vec![(index % 251) as u8; FRAME])
                    .unwrap();
            }
        });
        let mut frame = vec![0u8; FRAME];
        for index in 0..FRAMES {
            reading.read_exact(&mut frame).unwrap();
            assert!(
                frame.iter().all(|byte| *byte == (index % 251) as u8),
                "frame {index} came back whole and in order"
            );
        }
        writer.join().unwrap();
        server.join().unwrap();
        super::clear(&endpoint);
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
