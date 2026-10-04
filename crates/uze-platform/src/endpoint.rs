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
/// An endpoint at a path of the caller's choosing, for a test that runs its
/// own server.
pub use imp::scratch_endpoint;
pub use imp::{Listener, Stream, accept, bind, clear, endpoint, identity, pair, peer_pid, restore};

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
