//! The environment a program started now would be given, as opposed to the
//! one this process inherited when it started.

use std::ffi::OsString;

/// The `PATH` a shell opened now would search, where the platform keeps it
/// somewhere a program can read; `None` where it does not.
///
/// An installer that adds its directory to the search path reaches every
/// shell opened after it, and no process already running. On Windows that
/// path is the machine's and the user's `Path` in the registry, read and
/// expanded here as a new shell would. On Unix it lives in the person's
/// shell startup files, which only that shell can read.
pub fn path_of_a_new_shell() -> Option<OsString> {
    imp::path_of_a_new_shell()
}

#[cfg(unix)]
mod imp {
    use std::ffi::OsString;

    pub(super) fn path_of_a_new_shell() -> Option<OsString> {
        None
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
        RegGetValueW,
    };

    use crate::win::wide;

    const MACHINE: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
    const USER: &str = "Environment";

    /// The machine's entries first, then the user's: the order Windows
    /// builds a new process's `Path` in.
    pub(super) fn path_of_a_new_shell() -> Option<OsString> {
        let parts: Vec<OsString> = [(HKEY_LOCAL_MACHINE, MACHINE), (HKEY_CURRENT_USER, USER)]
            .into_iter()
            .filter_map(|(root, key)| expanded_value(root, key, "Path"))
            .filter(|value| !value.is_empty())
            .collect();
        (!parts.is_empty()).then(|| parts.join(OsStr::new(";")))
    }

    /// A string value, its `%VARIABLE%` references expanded: `RegGetValueW`
    /// expands an expandable string unless told not to.
    fn expanded_value(root: HKEY, key: &str, name: &str) -> Option<OsString> {
        let key = wide(OsStr::new(key));
        let name = wide(OsStr::new(name));
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
        let mut size = 0u32;
        // SAFETY: both strings are NUL-terminated; a null buffer asks only
        // for the size.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the platform keeps it, it is the path every program on the
    /// machine is found by, so the system's own directory is on it.
    #[test]
    fn a_new_shell_s_path_reaches_the_system() {
        let Some(path) = path_of_a_new_shell() else {
            return;
        };
        let system = std::env::var_os("SystemRoot").map(|root| {
            std::path::PathBuf::from(root)
                .join("System32")
                .to_string_lossy()
                .to_lowercase()
        });
        assert!(
            std::env::split_paths(&path).any(|directory| system.as_deref()
                == Some(directory.to_string_lossy().to_lowercase().as_str())),
            "{path:?}"
        );
    }
}
