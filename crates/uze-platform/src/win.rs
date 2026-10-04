//! What every Windows implementation here, and the Windows backends built
//! on this crate, share: wide strings and owned kernel handles.

use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

/// `text` as the NUL-terminated UTF-16 the Win32 API takes.
pub fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// A kernel handle closed when dropped.
pub struct Owned(pub HANDLE);

// SAFETY: a kernel handle is usable from any thread.
unsafe impl Send for Owned {}
unsafe impl Sync for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: owned, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}
