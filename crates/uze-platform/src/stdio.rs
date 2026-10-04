//! The process's own standard streams.

/// Makes a write to a reader that went away end the process quietly, as a
/// command piped into `head` is expected to. Called only on paths that print
/// a report: a server writing to a peer that hung up must see the error,
/// not die of it.
pub fn die_quietly_on_a_closed_pipe() {
    imp::die_quietly_on_a_closed_pipe()
}

/// Sends everything later written to stdout nowhere, for this process and
/// the children it starts, while stderr still reaches the person.
pub fn silence_stdout() {
    imp::silence_stdout()
}

#[cfg(unix)]
mod imp {
    use std::os::fd::AsRawFd;

    pub(super) fn die_quietly_on_a_closed_pipe() {
        // Safety: `SIG_DFL` is the disposition the process started life
        // with; it installs no handler of our own.
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    }

    pub(super) fn silence_stdout() {
        if let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") {
            // Safety: both descriptors are open.
            unsafe { libc::dup2(null.as_raw_fd(), libc::STDOUT_FILENO) };
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{STD_OUTPUT_HANDLE, SetStdHandle};

    /// Windows has no signal for it: a write to a closed pipe is an error
    /// like any other, which the report's own writer answers.
    pub(super) fn die_quietly_on_a_closed_pipe() {}

    /// The standard library asks for the stdout handle on every write, and
    /// a child inherits the one set here.
    pub(super) fn silence_stdout() {
        if let Ok(null) = std::fs::OpenOptions::new().write(true).open("NUL") {
            // Safety: the handle is open and is never closed: it is stdout
            // for the rest of the process.
            unsafe { SetStdHandle(STD_OUTPUT_HANDLE, null.into_raw_handle()) };
        }
    }
}
