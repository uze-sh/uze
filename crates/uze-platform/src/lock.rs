//! A lock that holds across processes, released by the kernel when its
//! holder dies however it dies.
//!
//! `flock` on Unix. `LockFileEx` on Windows, over one byte far past
//! anything the file holds: Windows locks are mandatory, and a lock over
//! the whole file would hide what the holder wrote in it (its pid) from the
//! very readers that need to name it.

use std::{fs::File, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Exclusive,
    Shared,
}

/// Takes the lock without waiting. `file` must be open for reading or
/// writing: Windows refuses to lock a handle opened only to append. A lock somebody else holds is an error
/// of kind [`io::ErrorKind::WouldBlock`]; any other error is the platform
/// failing to lock at all, which a caller must never read as "held".
pub fn try_lock(file: &File, mode: Mode) -> io::Result<()> {
    platform::try_lock(file, mode)
}

/// Lets go of a lock [`try_lock`] took. Closing the file does the same.
pub fn unlock(file: &File) {
    platform::unlock(file)
}

#[cfg(unix)]
mod platform {
    use std::{fs::File, io, os::fd::AsRawFd};

    use super::Mode;

    pub(super) fn try_lock(file: &File, mode: Mode) -> io::Result<()> {
        let operation = match mode {
            Mode::Exclusive => libc::LOCK_EX,
            Mode::Shared => libc::LOCK_SH,
        } | libc::LOCK_NB;
        // SAFETY: `file` owns the descriptor for the whole call, and a lock
        // it takes is released by the kernel when the descriptor closes.
        if unsafe { libc::flock(file.as_raw_fd(), operation) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn unlock(file: &File) {
        // SAFETY: as in `try_lock`.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    }
}

#[cfg(windows)]
mod platform {
    use std::{fs::File, io, os::windows::io::AsRawHandle};

    use windows_sys::Win32::{
        Foundation::{ERROR_IO_PENDING, ERROR_LOCK_VIOLATION, GetLastError},
        Storage::FileSystem::{
            LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, UnlockFileEx,
        },
        System::IO::OVERLAPPED,
    };

    use super::Mode;

    const LOCKED_OFFSET: u64 = u64::MAX - 1;

    fn range() -> OVERLAPPED {
        // SAFETY: zeroed is the documented initial state.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.Anonymous.Anonymous.Offset = LOCKED_OFFSET as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (LOCKED_OFFSET >> 32) as u32;
        overlapped
    }

    pub(super) fn try_lock(file: &File, mode: Mode) -> io::Result<()> {
        let mut overlapped = range();
        let mut flags = LOCKFILE_FAIL_IMMEDIATELY;
        if mode == Mode::Exclusive {
            flags |= LOCKFILE_EXCLUSIVE_LOCK;
        }
        // SAFETY: `file` owns its handle for the call; a synchronous handle
        // completes the lock before returning.
        if unsafe { LockFileEx(file.as_raw_handle(), flags, 0, 1, 0, &mut overlapped) } != 0 {
            return Ok(());
        }
        // SAFETY: reads this thread's last error.
        match unsafe { GetLastError() } {
            // A synchronous handle answers a contended immediate lock with
            // ERROR_IO_PENDING on some builds; it is still "held".
            ERROR_LOCK_VIOLATION | ERROR_IO_PENDING => Err(io::ErrorKind::WouldBlock.into()),
            error => Err(io::Error::from_raw_os_error(error as i32)),
        }
    }

    pub(super) fn unlock(file: &File) {
        let mut overlapped = range();
        // SAFETY: as in `try_lock`.
        unsafe { UnlockFileEx(file.as_raw_handle(), 0, 1, 0, &mut overlapped) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("uze-platform-{name}-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn open(path: &std::path::Path) -> File {
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .unwrap()
    }

    #[test]
    fn a_holder_in_another_process_refuses_this_one_and_is_named() {
        use std::io::{BufRead, Write as _};
        let path = scratch("across-processes");
        let mut holder = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lock::tests::hold_the_lock_named_in_the_environment",
            ])
            .args(["--ignored", "--nocapture", "--test-threads", "1"])
            .env(HOLDER, &path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = std::io::BufReader::new(holder.stdout.take().unwrap()).lines();
        assert!(
            // The harness prints the test's name on the same line first.
            lines.any(|line| line.is_ok_and(|line| line.ends_with("held"))),
            "the holder took the lock"
        );

        let other = open(&path);
        let refused = try_lock(&other, Mode::Exclusive).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("pid={}", holder.id()),
            "who holds it is still readable"
        );

        drop(holder.stdin.take());
        assert!(holder.wait().unwrap().success());
        try_lock(&other, Mode::Exclusive).unwrap();
        drop(other);
        let _ = std::io::stdout().flush();
        let _ = std::fs::remove_file(&path);
    }

    const HOLDER: &str = "UZE_PLATFORM_LOCK_HOLDER";

    /// Not a test of its own: the second process of the one above, which
    /// takes the lock, says so, and holds it until its input closes.
    #[test]
    #[ignore = "started by a_holder_in_another_process_refuses_this_one_and_is_named"]
    fn hold_the_lock_named_in_the_environment() {
        use std::io::{Read as _, Write as _};
        let Some(path) = std::env::var_os(HOLDER) else {
            return;
        };
        let mut file = open(std::path::Path::new(&path));
        try_lock(&file, Mode::Exclusive).unwrap();
        write!(file, "pid={}", std::process::id()).unwrap();
        file.flush().unwrap();
        println!("held");
        std::io::stdout().flush().unwrap();
        let _ = std::io::stdin().read(&mut [0u8; 1]);
    }

    #[test]
    fn an_exclusive_holder_refuses_another_and_keeps_its_contents_readable() {
        use std::io::Write as _;
        let path = scratch("exclusive");
        let mut holder = open(&path);
        try_lock(&holder, Mode::Exclusive).unwrap();
        write!(holder, "pid=42").unwrap();
        holder.flush().unwrap();

        let other = open(&path);
        let refused = try_lock(&other, Mode::Exclusive).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::WouldBlock);
        let refused = try_lock(&other, Mode::Shared).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "pid=42");

        // Released by `unlock`, not by dropping the holder: a sibling test
        // forking a child holds a copy of every descriptor until that child
        // execs, and with it the lock a close alone would end.
        unlock(&holder);
        try_lock(&other, Mode::Exclusive).unwrap();
        drop(holder);
        drop(other);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn shared_holders_coexist_and_keep_an_exclusive_one_out() {
        let path = scratch("shared");
        let first = open(&path);
        let second = open(&path);
        try_lock(&first, Mode::Shared).unwrap();
        try_lock(&second, Mode::Shared).unwrap();
        let writer = open(&path);
        assert_eq!(
            try_lock(&writer, Mode::Exclusive).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        unlock(&first);
        unlock(&second);
        try_lock(&writer, Mode::Exclusive).unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
