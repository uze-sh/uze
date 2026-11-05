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
#[cfg(unix)]
pub(crate) use unix::shared_private_directory;
#[cfg(windows)]
use windows as imp;

/// The longest socket path the platform's `sockaddr_un` holds.
#[cfg(unix)]
pub use imp::MAX_SOCKET_PATH;
/// Reaches the server listening at `endpoint` directly: what a client
/// that must not replace that server does instead of attaching.
pub use imp::connect;
/// Whether something a server bound outlasts it at `endpoint`: a socket
/// file stays until someone removes it, a named pipe goes with its last
/// handle. What tells a stopped server's leftover from an endpoint that
/// was never there.
pub use imp::lingers;
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

    /// Makes a directory this test owns open to others, answering its mode.
    #[cfg(unix)]
    fn open_directory(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        mode(path)
    }

    #[cfg(unix)]
    fn mode(path: &std::path::Path) -> u32 {
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(path).unwrap().permissions())
            & 0o7777
    }

    /// Where a shared runtime name is looked for, with the session's runtime
    /// directory and the temp dir both set to `scratch`.
    #[cfg(unix)]
    fn shared_names(scratch: &std::path::Path, name: &str) -> [std::path::PathBuf; 2] {
        [scratch.join(name), std::path::Path::new("/tmp").join(name)]
    }

    /// A link is not a directory of this user's, whatever it reaches: the
    /// socket directory is not there, and what the link reaches is left
    /// exactly as it was.
    #[cfg(unix)]
    #[test]
    fn a_link_at_a_socket_directory_name_is_refused_and_what_it_reaches_keeps_its_mode() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-link-socket");
        let reached = scratch.join("reached");
        assert_eq!(open_directory(&reached), 0o755);
        let mut env = uze_testkit::env::scope();
        env.set("XDG_RUNTIME_DIR", &scratch);
        env.set("TMPDIR", &scratch);
        let label = format!("uze-link-socket-{}", std::process::id());
        // SAFETY: no arguments, and it cannot fail.
        let uid = unsafe { libc::getuid() };
        let links = shared_names(&scratch, &format!("{label}-{uid}"));
        for link in &links {
            std::os::unix::fs::symlink(&reached, link).unwrap();
        }

        let answered = crate::fs::user_socket_directory(&label, 10);
        let after = mode(&reached);

        for link in &links {
            let _ = std::fs::remove_file(link);
        }
        let _ = std::fs::remove_dir_all(&scratch);
        assert_eq!(answered, None);
        assert_eq!(after, 0o755);
    }

    /// An endpoint's own directory being a link sends it elsewhere, without
    /// changing what the link reaches.
    #[cfg(unix)]
    #[test]
    fn an_endpoint_directory_that_is_a_link_is_refused_and_what_it_reaches_keeps_its_mode() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-link-own");
        let reached = scratch.join("reached");
        assert_eq!(open_directory(&reached), 0o755);
        let link = scratch.join("link");
        std::os::unix::fs::symlink(&reached, &link).unwrap();
        let mut env = uze_testkit::env::scope();
        env.set("XDG_RUNTIME_DIR", &scratch);
        env.set("TMPDIR", &scratch);
        let namespace = format!("uze-link-own-{}", std::process::id());

        let chosen = super::endpoint(super::Address {
            directory: &link,
            namespace: &namespace,
            name: "w",
        })
        .expect("the next directory is used");
        let after = mode(&reached);

        let directory = chosen.parent().unwrap().to_path_buf();
        let _ = std::fs::remove_dir_all(&scratch);
        assert_ne!(directory, link);
        assert_eq!(after, 0o755);
    }

    /// Past every shared name, an address whose directory is a link has no
    /// place to remember what it chose: it is refused, with nothing written
    /// through the link and what the link reaches left as it was.
    #[cfg(unix)]
    #[test]
    fn an_address_that_can_only_be_remembered_through_a_link_is_refused() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-link-record");
        let reached = scratch.join("reached");
        assert_eq!(open_directory(&reached), 0o755);
        let link = scratch.join("a".repeat(120));
        std::os::unix::fs::symlink(&reached, &link).unwrap();
        let mut env = uze_testkit::env::scope();
        env.set("XDG_RUNTIME_DIR", &scratch);
        env.set("TMPDIR", &scratch);
        let namespace = format!("uze-link-record-{}", std::process::id());
        // SAFETY: no arguments, and it cannot fail.
        let uid = unsafe { libc::getuid() };
        let taken = shared_names(&scratch, &format!("{namespace}-runtime-{uid}"));
        for path in &taken {
            std::fs::write(path, b"").unwrap();
        }

        let answered = super::endpoint(super::Address {
            directory: &link,
            namespace: &namespace,
            name: "w",
        });
        let after = mode(&reached);
        let written = std::fs::read_dir(&reached).unwrap().count();

        for path in &taken {
            let _ = std::fs::remove_file(path);
        }
        if let Ok(chosen) = &answered {
            let _ = std::fs::remove_dir_all(chosen.parent().unwrap());
        }
        let _ = std::fs::remove_dir_all(&scratch);
        assert!(answered.is_err(), "{answered:?}");
        assert_eq!(after, 0o755);
        assert_eq!(written, 0);
    }

    /// A directory put back after a cleaner took it is held to the same
    /// demand as the first time: a link in its place is refused unchanged.
    #[cfg(unix)]
    #[test]
    fn restoring_over_a_link_is_refused_and_what_it_reaches_keeps_its_mode() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-link-restore");
        let reached = scratch.join("reached");
        assert_eq!(open_directory(&reached), 0o755);
        let link = scratch.join("link");
        std::os::unix::fs::symlink(&reached, &link).unwrap();

        let restored = super::restore(&link.join("w.sock"));
        let after = mode(&reached);

        let _ = std::fs::remove_dir_all(&scratch);
        assert!(restored.is_err(), "{restored:?}");
        assert_eq!(after, 0o755);
    }

    /// This user's own directory at a runtime name is still adopted and
    /// narrowed: the first run under a permissive umask depends on it.
    #[cfg(unix)]
    #[test]
    fn this_users_own_directory_at_a_runtime_name_is_narrowed_and_used() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-own-open");
        let own = scratch.join("own");
        assert_eq!(open_directory(&own), 0o755);

        super::restore(&own.join("w.sock")).expect("this user's own directory is usable");
        let after = mode(&own);

        let _ = std::fs::remove_dir_all(&scratch);
        assert_eq!(after, 0o700);
    }

    /// A runtime directory that is not there yet is made private, along
    /// with whatever leads to it.
    #[cfg(unix)]
    #[test]
    fn a_runtime_directory_not_there_yet_is_made_private_with_its_parents() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-absent");
        let deep = scratch.join("one").join("two");

        super::restore(&deep.join("w.sock")).expect("made");
        let after = (mode(&scratch.join("one")), mode(&deep));

        let _ = std::fs::remove_dir_all(&scratch);
        assert_eq!(after, (0o700, 0o700));
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

    /// A listener that is gone leaves its endpoint behind only where the
    /// endpoint is a file, and clearing it leaves nothing either way.
    #[test]
    fn a_stopped_listener_lingers_only_where_its_endpoint_is_a_file() {
        let scratch = uze_testkit::temp::socket_scratch("endpoint-lingers");
        std::fs::create_dir_all(&scratch).unwrap();
        let endpoint = super::scratch_endpoint(&scratch, "gone.sock");
        assert!(!super::lingers(&endpoint), "nothing was ever bound");

        drop(super::bind(&endpoint).unwrap());

        assert_eq!(super::lingers(&endpoint), cfg!(unix));
        super::clear(&endpoint);
        assert!(!super::lingers(&endpoint));
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
