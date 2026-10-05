//! Noticing a Ctrl+C while a child that cannot receive it runs.
//!
//! A child started without a terminal does not see the person's Ctrl+C, so
//! whoever waits on it watches for the interrupt instead, ends the child,
//! and then lets the interrupt end this process the way it would have.

/// Catches the interrupt while it lives, restoring whatever handled it
/// before.
pub struct InterruptWatch {
    /// Restores the previous handler when dropped.
    _watch: imp::Watch,
}

impl InterruptWatch {
    pub fn install() -> Self {
        Self {
            _watch: imp::Watch::install(),
        }
    }

    /// Whether an interrupt arrived since [`InterruptWatch::install`].
    pub fn interrupted(&self) -> bool {
        imp::interrupted()
    }

    /// Restores the previous handler and delivers the interrupt to it, so
    /// the process ends the way the Ctrl+C would have ended it.
    pub fn deliver(self) {
        drop(self);
        imp::deliver();
    }
}

/// Lets a Ctrl+C reach this process and what it starts with its default
/// effect again. Whether it is ignored is inherited: a process started in
/// a group of its own on Windows ignores it, and so would every program a
/// person runs under it, which then nothing could interrupt. On Unix the
/// same holds for an ignored `SIGINT`.
pub fn restore_default() {
    imp::restore_default()
}

#[cfg(unix)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};

    static INTERRUPTED: AtomicBool = AtomicBool::new(false);

    extern "C" fn note(_signal: libc::c_int) {
        INTERRUPTED.store(true, Ordering::SeqCst);
    }

    pub(super) struct Watch {
        previous: libc::sigaction,
    }

    impl Watch {
        pub(super) fn install() -> Self {
            INTERRUPTED.store(false, Ordering::SeqCst);
            // SAFETY: both structs are plain data, zeroed is valid for them,
            // and the handler only stores to an atomic.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = note as extern "C" fn(libc::c_int) as usize;
                libc::sigemptyset(&mut action.sa_mask);
                let mut previous: libc::sigaction = std::mem::zeroed();
                libc::sigaction(libc::SIGINT, &action, &mut previous);
                Self { previous }
            }
        }
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            // SAFETY: `previous` is what `sigaction` handed back on install.
            unsafe { libc::sigaction(libc::SIGINT, &self.previous, std::ptr::null_mut()) };
        }
    }

    pub(super) fn interrupted() -> bool {
        INTERRUPTED.load(Ordering::SeqCst)
    }

    pub(super) fn deliver() {
        // SAFETY: plain `raise(3)`.
        unsafe { libc::raise(libc::SIGINT) };
    }

    pub(super) fn restore_default() {
        // SAFETY: installs the default disposition; no handler runs.
        unsafe { libc::signal(libc::SIGINT, libc::SIG_DFL) };
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::{
        Foundation::{FALSE, TRUE},
        System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler},
    };

    static INTERRUPTED: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn note(event: u32) -> i32 {
        if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
            INTERRUPTED.store(true, Ordering::SeqCst);
            return TRUE;
        }
        FALSE
    }

    pub(super) struct Watch;

    impl Watch {
        pub(super) fn install() -> Self {
            INTERRUPTED.store(false, Ordering::SeqCst);
            // SAFETY: registers a handler that only stores to an atomic.
            unsafe { SetConsoleCtrlHandler(Some(note), TRUE) };
            Self
        }
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            // SAFETY: removes the handler `install` registered.
            unsafe { SetConsoleCtrlHandler(Some(note), FALSE) };
        }
    }

    pub(super) fn interrupted() -> bool {
        INTERRUPTED.load(Ordering::SeqCst)
    }

    /// The default Ctrl+C action of a console process is to end it with
    /// the status Windows gives an interrupted program.
    pub(super) fn deliver() {
        const STATUS_CONTROL_C_EXIT: i32 = 0xC000_013Au32 as i32;
        std::process::exit(STATUS_CONTROL_C_EXIT);
    }

    pub(super) fn restore_default() {
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
        // SAFETY: clears the inherited ignore flag; no routine is named.
        unsafe { SetConsoleCtrlHandler(None, 0) };
    }
}
