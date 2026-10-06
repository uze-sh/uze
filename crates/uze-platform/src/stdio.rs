//! The process's own standard streams.

use std::time::{Duration, Instant};

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

/// Asks the terminal a question in its own escape-sequence language and
/// collects what it writes back, until `answered` says the reply is whole.
/// `None` where there is no terminal to ask, where it had not finished
/// answering when `patience` ran out, and on Windows, which does not ask.
///
/// Its input is taken out of line editing and echo for the exchange only,
/// since a reply arrives as input: echoed it lands on the screen, and line
/// buffered it waits for a newline that never comes. Ask before anything
/// else starts reading that input, or the reply is read as keystrokes.
pub fn ask_terminal(
    question: &[u8],
    answered: &dyn Fn(&[u8]) -> bool,
    patience: Duration,
) -> Option<Vec<u8>> {
    imp::ask_terminal(question, answered, Instant::now() + patience)
}

/// Whether escape sequences written to stdout reach a terminal that draws
/// them as styling: stdout is a terminal, and on Unix one that names itself
/// something other than `dumb`; on Windows a console, in which processing
/// them is switched on here, once, as a console leaves it off for a
/// program that does not ask (the classic console host's default).
pub fn escapes_reach_the_terminal() -> bool {
    static ANSWER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(imp::escapes_reach_the_terminal)
}

/// Sends everything later written to stdout nowhere, for this process and
/// the children it starts, while stderr still reaches the person.
pub fn silence_stdout() {
    imp::silence_stdout()
}

#[cfg(unix)]
mod imp {
    use std::os::fd::AsRawFd;

    use super::{Duration, Instant};

    pub(super) fn ask_terminal(
        question: &[u8],
        answered: &dyn Fn(&[u8]) -> bool,
        deadline: Instant,
    ) -> Option<Vec<u8>> {
        let mut tty = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .ok()?;
        ask_on(&mut tty, question, answered, deadline)
    }

    fn ask_on(
        tty: &mut std::fs::File,
        question: &[u8],
        answered: &dyn Fn(&[u8]) -> bool,
        deadline: Instant,
    ) -> Option<Vec<u8>> {
        let fd = tty.as_raw_fd();
        // Safety: `termios` is plain data, filled in whole by `tcgetattr`.
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        // Safety: `fd` is open for the life of `tty`.
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return None;
        }
        let mut quiet = saved;
        quiet.c_lflag &= !(libc::ICANON | libc::ECHO);
        // Safety: as above.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &quiet) } != 0 {
            return None;
        }
        let reply = exchange(tty, question, answered, deadline);
        // Safety: as above; puts back exactly what was read.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
        reply
    }

    fn exchange(
        tty: &mut std::fs::File,
        question: &[u8],
        answered: &dyn Fn(&[u8]) -> bool,
        deadline: Instant,
    ) -> Option<Vec<u8>> {
        use std::io::{ErrorKind, Read as _, Write as _};
        tty.write_all(question).ok()?;
        let mut reply = Vec::new();
        let mut chunk = [0u8; 256];
        while !answered(&reply) {
            let left = deadline.checked_duration_since(Instant::now())?;
            match readable_within(tty.as_raw_fd(), left) {
                Readiness::Ready => {}
                Readiness::Interrupted => continue,
                Readiness::TimedOut => return None,
            }
            match tty.read(&mut chunk) {
                Ok(0) => return None,
                Ok(read) => reply.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        Some(reply)
    }

    enum Readiness {
        Ready,
        Interrupted,
        TimedOut,
    }

    /// `select` rather than `poll`, because macOS's `poll` refuses a
    /// terminal device.
    fn readable_within(fd: std::os::fd::RawFd, wait: Duration) -> Readiness {
        // Safety: `fd_set` and `timeval` are plain data, and `select` only
        // reads and writes the two passed by reference.
        unsafe {
            let mut readable: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut readable);
            libc::FD_SET(fd, &mut readable);
            let mut timeout = libc::timeval {
                tv_sec: wait.as_secs() as libc::time_t,
                tv_usec: wait.subsec_micros() as libc::suseconds_t,
            };
            match libc::select(
                fd + 1,
                &mut readable,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut timeout,
            ) {
                ready if ready > 0 => Readiness::Ready,
                0 => Readiness::TimedOut,
                _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => {
                    Readiness::Interrupted
                }
                _ => Readiness::TimedOut,
            }
        }
    }

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

    pub(super) fn escapes_reach_the_terminal() -> bool {
        use std::io::IsTerminal as _;
        std::io::stdout().is_terminal() && std::env::var("TERM").is_ok_and(|term| term != "dumb")
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

    #[cfg(test)]
    mod tests {
        use std::{
            io::{Read as _, Write as _},
            os::fd::FromRawFd as _,
            thread,
        };

        use super::*;

        const QUESTION: &[u8] = b"\x1b]11;?\x1b\\\x1b[c";

        /// A pseudoterminal: the end a terminal emulator holds, and the
        /// end a program asks on.
        fn pseudoterminal() -> (std::fs::File, std::fs::File) {
            let (mut terminal, mut program) = (0, 0);
            // Safety: both descriptors are written by `openpty` and owned
            // by the files made from them; the optional arguments are null.
            unsafe {
                assert_eq!(
                    libc::openpty(
                        &mut terminal,
                        &mut program,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    ),
                    0,
                    "openpty"
                );
                (
                    std::fs::File::from_raw_fd(terminal),
                    std::fs::File::from_raw_fd(program),
                )
            }
        }

        fn ends_with_device_attributes(reply: &[u8]) -> bool {
            reply.ends_with(b"c")
        }

        fn local_modes(tty: &std::fs::File) -> libc::tcflag_t {
            // Safety: as in `ask_on`.
            let mut modes: libc::termios = unsafe { std::mem::zeroed() };
            assert_eq!(unsafe { libc::tcgetattr(tty.as_raw_fd(), &mut modes) }, 0);
            modes.c_lflag
        }

        #[test]
        fn the_reply_is_read_until_it_is_whole_and_the_question_reaches_the_terminal() {
            let (mut terminal, mut program) = pseudoterminal();
            let emulator = thread::spawn(move || {
                let mut asked = vec![0u8; QUESTION.len()];
                terminal.read_exact(&mut asked).expect("the question");
                // In two writes, as a reply crossing a connection arrives.
                terminal.write_all(b"\x1b]11;rgb:ffff/").expect("reply");
                thread::sleep(Duration::from_millis(20));
                terminal
                    .write_all(b"ffff/ffff\x07\x1b[?62c")
                    .expect("reply");
                (asked, terminal)
            });
            let reply = ask_on(
                &mut program,
                QUESTION,
                &ends_with_device_attributes,
                Instant::now() + Duration::from_secs(5),
            );
            let (asked, _terminal) = emulator.join().expect("emulator");
            assert_eq!(asked, QUESTION);
            assert_eq!(
                reply.as_deref(),
                Some(&b"\x1b]11;rgb:ffff/ffff/ffff\x07\x1b[?62c"[..])
            );
        }

        #[test]
        fn a_terminal_that_never_answers_is_given_up_on_at_the_deadline() {
            let (_terminal, mut program) = pseudoterminal();
            let started = Instant::now();
            let reply = ask_on(
                &mut program,
                QUESTION,
                &ends_with_device_attributes,
                Instant::now() + Duration::from_millis(100),
            );
            assert_eq!(reply, None);
            assert!(started.elapsed() < Duration::from_secs(2));
        }

        #[test]
        fn the_terminal_is_handed_back_editing_and_echoing_as_it_was() {
            let (_terminal, mut program) = pseudoterminal();
            let before = local_modes(&program);
            assert_ne!(before & libc::ICANON, 0, "a fresh terminal edits lines");
            let _ = ask_on(
                &mut program,
                QUESTION,
                &ends_with_device_attributes,
                Instant::now() + Duration::from_millis(50),
            );
            assert_eq!(local_modes(&program), before);
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::io::IntoRawHandle;

    use super::Instant;
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode, SetStdHandle,
    };

    /// Unknown on Windows: the desktop's own setting always answers there
    /// (`desktop::color_scheme`), so nothing has had to ask a console, and
    /// a console's input is not a byte stream a reply can simply be read
    /// back from.
    pub(super) fn ask_terminal(
        _question: &[u8],
        _answered: &dyn Fn(&[u8]) -> bool,
        _deadline: Instant,
    ) -> Option<Vec<u8>> {
        None
    }

    /// A console's mode is the console's, not this process's: setting it
    /// on stdout is what every program that draws in colour does, and it
    /// fails where stdout is no console.
    pub(super) fn escapes_reach_the_terminal() -> bool {
        // SAFETY: a standard handle is this process's for its lifetime;
        // both calls read or write only the mode passed by reference.
        unsafe {
            let output = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0;
            GetConsoleMode(output, &mut mode) != 0
                && (mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0
                    || SetConsoleMode(output, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0)
        }
    }

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
