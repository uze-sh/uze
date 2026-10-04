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
        let path = std::env::temp_dir().join(format!("uze-process-{name}-{}", std::process::id()));
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

        drop(holder);
        try_lock(&other, Mode::Exclusive).unwrap();
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
