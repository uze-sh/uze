//! A controlling terminal a process is seated at.
//!
//! What tells a person's process from a daemon's: a shell in a pane, an
//! editor's terminal, a build started from either, all run in a session
//! that holds a terminal somebody can type at, and `probe` reports them
//! attended. A process that put itself in a session of its own holds none,
//! and is reported unattended. A test standing in for somebody at work in
//! a directory needs the same: a pseudoterminal of its own, held open for
//! as long as the stand-in runs, since a hangup takes the terminal from
//! every process in the session.
//!
//! Windows keeps no controlling terminal, and `probe` answers unknown there,
//! which every caller reads as attended: seating a process is nothing to do.

use std::process::Command;

/// A pseudoterminal to seat a process at, alive for as long as this value
/// is.
pub struct Terminal(platform::Terminal);

impl Terminal {
    /// Opens a terminal of its own. Panics when the platform has one and
    /// cannot open it: a test asking for a terminal has nothing to do
    /// without one.
    pub fn open() -> Self {
        Self(platform::Terminal::open())
    }

    /// Seats the child `command` starts at this terminal, between its fork
    /// and its exec: a session of its own, with this terminal as its
    /// controlling one.
    pub fn seat(&self, command: &mut Command) {
        self.0.seat(command);
    }

    /// Seats this very process at this terminal.
    pub fn sit_here(&self) {
        self.0.sit_here();
    }
}

#[cfg(unix)]
mod platform {
    use std::ffi::{CStr, CString};
    use std::os::fd::{FromRawFd as _, OwnedFd};
    use std::process::Command;

    pub(super) struct Terminal {
        /// Held for its life, never read: the terminal exists while it is open.
        _master: OwnedFd,
        slave: CString,
    }

    impl Terminal {
        pub(super) fn open() -> Self {
            // SAFETY: plain libc calls; the descriptor is owned from here on.
            let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
            assert!(
                master >= 0,
                "no pseudoterminal: {}",
                std::io::Error::last_os_error()
            );
            // SAFETY: `master` is the descriptor just opened.
            unsafe {
                assert_eq!(libc::grantpt(master), 0);
                assert_eq!(libc::unlockpt(master), 0);
            }
            Self {
                slave: slave_of(master),
                // SAFETY: `master` is open and owned by nothing else.
                _master: unsafe { OwnedFd::from_raw_fd(master) },
            }
        }

        pub(super) fn seat(&self, command: &mut Command) {
            use std::os::unix::process::CommandExt as _;
            let slave = self.slave.clone();
            // SAFETY: runs in the child between fork and exec, calling only
            // async-signal-safe libc functions.
            unsafe {
                command.pre_exec(move || sit_at(&slave));
            }
        }

        pub(super) fn sit_here(&self) {
            sit_at(&self.slave).expect("this process sits at its terminal");
        }
    }

    /// The path of the slave end of `master`, asked in a way several
    /// threads may ask at once: tests open terminals in parallel, and
    /// `ptsname` answers them all from one static buffer.
    #[cfg(target_os = "linux")]
    fn slave_of(master: std::os::fd::RawFd) -> CString {
        let mut name = [0 as libc::c_char; 128];
        // SAFETY: `name` is `len` bytes, and `ptsname_r` writes at most that.
        let asked = unsafe { libc::ptsname_r(master, name.as_mut_ptr(), name.len()) };
        assert_eq!(
            asked,
            0,
            "no name for the terminal: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: `ptsname_r` NUL-terminates what it writes.
        unsafe { CStr::from_ptr(name.as_ptr()) }.to_owned()
    }

    /// Darwin has no `ptsname_r`; the kernel answers the same question
    /// through an ioctl, into a buffer of the caller's own.
    #[cfg(target_os = "macos")]
    fn slave_of(master: std::os::fd::RawFd) -> CString {
        let mut name = [0 as libc::c_char; 128];
        // SAFETY: `TIOCPTYGNAME` writes at most 128 bytes, the size of `name`.
        let asked = unsafe { libc::ioctl(master, libc::TIOCPTYGNAME as _, name.as_mut_ptr()) };
        assert_eq!(
            asked,
            0,
            "no name for the terminal: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: the kernel NUL-terminates what it writes.
        unsafe { CStr::from_ptr(name.as_ptr()) }.to_owned()
    }

    /// The remaining Unixes: `ptsname`, under a lock of this crate's own,
    /// which is the most that can be done for a static buffer.
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn slave_of(master: std::os::fd::RawFd) -> CString {
        static NAMING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _naming = NAMING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: `ptsname` answers with a static buffer, copied under the
        // lock before anything else in this crate could ask it.
        unsafe { CStr::from_ptr(libc::ptsname(master)) }.to_owned()
    }

    /// A new session, with the terminal at `slave` as its controlling one.
    /// The descriptor stays open for the process's life, and is inherited
    /// by what it runs, the way a shell's own three are: on the BSDs,
    /// Darwin among them, the last close of the slave detaches the terminal
    /// from the session, where Linux keeps it until a hangup.
    fn sit_at(slave: &CStr) -> std::io::Result<()> {
        // SAFETY: plain libc calls on a path and a descriptor this process owns.
        unsafe {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let fd = libc::open(slave.as_ptr(), libc::O_RDWR);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(fd, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use std::process::Command;

    /// Nothing to hold: there is no controlling terminal to be seated at.
    pub(super) struct Terminal;

    impl Terminal {
        pub(super) fn open() -> Self {
            Self
        }

        pub(super) fn seat(&self, _command: &mut Command) {}

        pub(super) fn sit_here(&self) {}
    }
}
