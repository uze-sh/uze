//! What every Windows implementation in this crate shares: wide strings,
//! owned kernel handles, registry values and the security descriptors and
//! SIDs that say whose something is.

use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        PSECURITY_DESCRIPTOR, PSID,
    },
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

/// The well-known SID of the built-in Administrators group, which owns
/// what an elevated token creates.
pub(crate) const ADMINISTRATORS: &str = "S-1-5-32-544";

/// A security descriptor granting the current token's user, and no one
/// else, full access; `inherited` is the SDDL inheritance its children
/// take (`OICI` for a directory, empty for none). The user SID and not the
/// owner SID: an elevated token's owner is the Administrators group.
pub(crate) struct OwnerOnly(pub PSECURITY_DESCRIPTOR);

impl OwnerOnly {
    pub(crate) fn new(inherited: &str) -> std::io::Result<Self> {
        let sid = crate::process::current_user()?;
        let sddl = wide(OsStr::new(&format!("D:P(A;{inherited};GA;;;{sid})")));
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; the descriptor is freed with
        // LocalFree in Drop, as the API requires.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for OwnerOnly {
    fn drop(&mut self) {
        // SAFETY: allocated by the conversion above with LocalAlloc.
        unsafe { LocalFree(self.0) };
    }
}

/// `sid` in its `S-1-…` text form; `None` when it cannot be converted.
///
/// # Safety
///
/// `sid` must point to a valid SID for the length of the call.
pub(crate) unsafe fn sid_text(sid: PSID) -> Option<String> {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: the caller vouches for `sid`; `text` is freed below.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return None;
    }
    // SAFETY: a NUL-terminated wide string from the conversion above, then
    // freed with LocalFree as the API requires.
    unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Some(sid)
    }
}

/// A registry string, its `%VARIABLE%` references expanded unless `extra`
/// carries `RRF_NOEXPAND`: `RegGetValueW` expands an expandable string
/// unless told not to. `None` when the value is absent; an error when it is
/// there and could not be read, which a caller about to write the value
/// back must never mistake for absent.
pub(crate) fn registry_string(
    root: HKEY,
    key: &str,
    name: &str,
    extra: u32,
) -> std::io::Result<Option<OsString>> {
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    const ERROR_MORE_DATA: u32 = 234;
    let key = wide(OsStr::new(key));
    let name = wide(OsStr::new(name));
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | extra;
    // The value can grow between asking its size and reading it, which the
    // read answers with `ERROR_MORE_DATA` and the size it needs now.
    let mut size = 0u32;
    let mut buffer: Vec<u16> = Vec::new();
    loop {
        // SAFETY: both strings are NUL-terminated; `buffer` holds `size`
        // bytes, and a null buffer asks only for the size.
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                std::ptr::null_mut(),
                if buffer.is_empty() {
                    std::ptr::null_mut()
                } else {
                    buffer.as_mut_ptr().cast()
                },
                &mut size,
            )
        };
        match status {
            ERROR_FILE_NOT_FOUND => return Ok(None),
            ERROR_MORE_DATA => buffer = vec![0u16; (size as usize).div_ceil(2)],
            0 if buffer.is_empty() && size > 0 => buffer = vec![0u16; (size as usize).div_ceil(2)],
            0 => break,
            failed => return Err(std::io::Error::from_raw_os_error(failed as i32)),
        }
    }
    let length = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    Ok(Some(OsString::from_wide(&buffer[..length])))
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
