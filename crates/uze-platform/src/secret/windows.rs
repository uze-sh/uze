//! The system-preferred generator of Windows' CNG.

use std::io;

use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

pub(super) fn fill(buffer: &mut [u8]) -> io::Result<()> {
    for chunk in buffer.chunks_mut(u32::MAX as usize) {
        // SAFETY: `chunk` is `chunk.len()` writable bytes, and a null
        // algorithm handle is what the system-preferred flag asks for.
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status < 0 {
            return Err(io::Error::other(format!(
                "the system's generator refused: NTSTATUS {status:#x}"
            )));
        }
    }
    Ok(())
}
