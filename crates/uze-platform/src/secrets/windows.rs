//! The Windows Credential Manager, as a generic credential of the
//! signed-in user, through the `windows-sys` already in the tree.

use std::io;

use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};

use super::SecretKey;

pub(super) const KEPT_IN: &str = "the Credential Manager";

fn target(key: &SecretKey<'_>) -> Vec<u16> {
    format!("{}:{}", key.service, key.account)
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

pub(super) fn store(key: &SecretKey<'_>, value: &str) -> io::Result<()> {
    let mut target = target(key);
    let mut blob = value.as_bytes().to_vec();
    let size = u32::try_from(blob.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a secret is too long"))?;
    // SAFETY: an all-zero CREDENTIALW is a valid, empty one.
    let mut credential: CREDENTIALW = unsafe { std::mem::zeroed() };
    credential.Type = CRED_TYPE_GENERIC;
    credential.TargetName = target.as_mut_ptr();
    credential.CredentialBlobSize = size;
    credential.CredentialBlob = blob.as_mut_ptr();
    credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
    // SAFETY: every pointer in `credential` outlives the call.
    let written = unsafe { CredWriteW(&credential, 0) };
    blob.fill(0);
    if written == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn read(key: &SecretKey<'_>) -> io::Result<Option<String>> {
    let target = target(key);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `target` is NUL-terminated; on success `credential` points
    // at a buffer the system allocated, freed below.
    let found = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if found == 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(code) if code == ERROR_NOT_FOUND as i32 => Ok(None),
            _ => Err(error),
        };
    }
    // SAFETY: the system says the blob holds `CredentialBlobSize` bytes.
    let value = unsafe {
        let credential = &*credential;
        let bytes = if credential.CredentialBlob.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        String::from_utf8_lossy(bytes).into_owned()
    };
    // SAFETY: `credential` came from `CredReadW`.
    unsafe { CredFree(credential.cast()) };
    Ok(Some(value))
}

pub(super) fn delete(key: &SecretKey<'_>) -> io::Result<()> {
    let target = target(key);
    // SAFETY: `target` is NUL-terminated.
    let deleted = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if deleted == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_NOT_FOUND as i32) {
            return Err(error);
        }
    }
    Ok(())
}
