//! What every Windows implementation here, and the Windows backends built
//! on this crate, share: wide strings, owned kernel handles and registry
//! values.

use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::Registry::{HKEY, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW},
};

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

/// A registry string, its `%VARIABLE%` references expanded unless `extra`
/// carries `RRF_NOEXPAND`: `RegGetValueW` expands an expandable string
/// unless told not to.
pub(crate) fn registry_string(root: HKEY, key: &str, name: &str, extra: u32) -> Option<OsString> {
    let key = wide(OsStr::new(key));
    let name = wide(OsStr::new(name));
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | extra;
    let mut size = 0u32;
    // SAFETY: both strings are NUL-terminated; a null buffer asks only for
    // the size.
    let status = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            name.as_ptr(),
            flags,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != 0 || size == 0 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY: `buffer` holds `size` bytes, as the call above asked for.
    let status = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            name.as_ptr(),
            flags,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let length = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    Some(OsString::from_wide(&buffer[..length]))
}

/// A registry `DWORD`.
pub(crate) fn registry_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let key = wide(OsStr::new(key));
    let name = wide(OsStr::new(name));
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both strings are NUL-terminated; `value` is `size` bytes.
    let status = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(value)
}
