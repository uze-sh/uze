//! The process's own standard streams.

/// Makes a write to a reader that went away end the process quietly, as a
/// command piped into `head` is expected to. Called only on paths that print
/// a report: a server writing to a peer that hung up must see the error,
/// not die of it.
pub fn die_quietly_on_a_closed_pipe() {
    imp::die_quietly_on_a_closed_pipe()
}

/// The terminal a program asks its person on, as one that asks would find
/// it: what to read the answer from, and where to ask. `None` where there
/// is none to ask on.
///
/// On Unix `/dev/tty`, whatever the standard streams are: the controlling
/// terminal, which a process in a session of its own does not have. On
/// Windows the console, but only while standard input is it: a Windows
/// program reaches its person through the console its input comes from,
/// and one whose input is redirected is not asked anything.
pub fn terminal() -> Option<(std::fs::File, std::fs::File)> {
    imp::terminal()
}

/// Sends everything later written to stdout nowhere, for this process and
/// the children it starts, while stderr still reaches the person.
pub fn silence_stdout() {
    imp::silence_stdout()
}

#[cfg(unix)]
mod imp {
    use std::os::fd::AsRawFd;

    pub(super) fn terminal() -> Option<(std::fs::File, std::fs::File)> {
        let open = |write: bool| {
            std::fs::OpenOptions::new()
                .read(!write)
                .write(write)
                .open("/dev/tty")
                .ok()
        };
        Some((open(false)?, open(true)?))
    }

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

    /// The console's own input and screen buffers.
    pub(super) fn terminal() -> Option<(std::fs::File, std::fs::File)> {
        use std::io::IsTerminal as _;
        if !std::io::stdin().is_terminal() {
            return None;
        }
        let input = std::fs::OpenOptions::new().read(true).open("CONIN$").ok()?;
        let output = std::fs::OpenOptions::new()
            .write(true)
            .open("CONOUT$")
            .ok()?;
        Some((input, output))
    }

    /// Windows has no signal for it: a write to a closed pipe is an error,
    /// and `println!` answers an error by panicking. That one panic is what
    /// SIGPIPE's default disposition is on Unix — the process ends, quietly,
    /// because its reader went away — and every other panic is reported as
    /// it was.
    pub(super) fn die_quietly_on_a_closed_pipe() {
        let report = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic| {
            if reader_went_away(panic) {
                std::process::exit(0);
            }
            report(panic);
        }));
    }

    /// `println!` panics with "failed printing to stdout: <error>", and a
    /// reader that closed its end is `ERROR_NO_DATA` (232) or
    /// `ERROR_BROKEN_PIPE` (109).
    fn reader_went_away(panic: &std::panic::PanicHookInfo<'_>) -> bool {
        let message = panic
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.payload().downcast_ref::<&str>().copied())
            .unwrap_or_default();
        message.starts_with("failed printing to stdout")
            && ["(os error 232)", "(os error 109)"]
                .iter()
                .any(|code| message.ends_with(code))
    }

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
