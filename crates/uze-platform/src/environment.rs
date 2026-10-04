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

/// Puts `directory` on the `PATH` every shell opened from now on searches,
/// first, unless it is there already; whether it was added. On Windows the
/// user's `Path` in the registry, and running programs are told it changed.
/// On Unix that path lives in the person's shell startup files, which are
/// theirs to edit, so nothing is changed there.
pub fn add_to_user_path(directory: &std::path::Path) -> std::io::Result<bool> {
    imp::add_to_user_path(directory)
}

/// Windows' alone: its user `Path` is the only search path UZE adds to.
#[cfg(windows)]
pub(crate) use imp::holds;

#[cfg(unix)]
mod imp {
    use std::ffi::OsString;

    pub(super) fn path_of_a_new_shell() -> Option<OsString> {
        None
    }

    pub(super) fn add_to_user_path(_directory: &std::path::Path) -> std::io::Result<bool> {
        Ok(false)
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::{OsStr, OsString};

    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_EXPAND_SZ, RRF_NOEXPAND, RegSetKeyValueW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    use std::path::Path;

    use crate::win::{registry_string, wide};

    /// Whether the search path `path` already reaches `directory`: an entry
    /// names it by the platform's rules for a path, whatever separator it ends
    /// with. The one rule both the installer's check and doctor's ask.
    pub(crate) fn holds(path: &OsStr, directory: &Path) -> bool {
        let bare = |entry: &Path| {
            let spelled = entry.to_string_lossy();
            let trimmed = spelled.trim_end_matches(std::path::is_separator);
            Path::new(if trimmed.is_empty() {
                spelled.as_ref()
            } else {
                trimmed
            })
            .to_path_buf()
        };
        let wanted = bare(directory);
        std::env::split_paths(path).any(|entry| crate::path::same_path(&bare(&entry), &wanted))
    }

    const MACHINE: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
    const USER: &str = "Environment";

    /// The machine's entries first, then the user's: the order Windows
    /// builds a new process's `Path` in.
    pub(super) fn path_of_a_new_shell() -> Option<OsString> {
        let parts: Vec<OsString> = [(HKEY_LOCAL_MACHINE, MACHINE), (HKEY_CURRENT_USER, USER)]
            .into_iter()
            .filter_map(|(root, key)| registry_string(root, key, "Path", 0).ok().flatten())
            .filter(|value| !value.is_empty())
            .collect();
        (!parts.is_empty()).then(|| parts.join(OsStr::new(";")))
    }

    /// A `Path` that cannot be read is an error, never an empty one: written
    /// back as empty, it would replace every directory the person had on
    /// it with this one. What is written back is the value as stored, its
    /// `%VARIABLE%`s unexpanded and its bytes untouched; only the check
    /// reads it expanded, as a new shell does.
    pub(super) fn add_to_user_path(directory: &std::path::Path) -> std::io::Result<bool> {
        let stored = registry_string(HKEY_CURRENT_USER, USER, "Path", RRF_NOEXPAND)?;
        let expanded = registry_string(HKEY_CURRENT_USER, USER, "Path", 0)?;
        if expanded.is_some_and(|expanded| holds(&expanded, directory)) {
            return Ok(false);
        }
        let mut path = directory.as_os_str().to_owned();
        if let Some(stored) = stored.filter(|stored| !stored.is_empty()) {
            path.push(";");
            path.push(&stored);
        }
        let value = wide(&path);
        let key = wide(OsStr::new(USER));
        let name = wide(OsStr::new("Path"));
        // SAFETY: every string is NUL-terminated, and the value's length is
        // its bytes, terminator included.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_EXPAND_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        if status != 0 {
            return Err(std::io::Error::from_raw_os_error(status as i32));
        }
        let environment = wide(OsStr::new("Environment"));
        let mut answer = 0;
        // SAFETY: a broadcast with a NUL-terminated string, bounded by its
        // timeout so a hung window cannot hold this up.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                &mut answer,
            )
        };
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One rule for "already on the path", whatever the entry ends with.
    /// Windows only: the user's `Path` in the registry is the only one UZE
    /// adds to, and so the only one it asks this of.
    #[cfg(windows)]
    #[test]
    fn a_directory_is_held_by_an_entry_naming_it() {
        use imp::holds;
        let directory = std::env::temp_dir().join("uze-bin");
        let spelled = directory.to_string_lossy().into_owned();
        let separator = std::path::MAIN_SEPARATOR;
        let path = |entries: &[&str]| std::env::join_paths(entries).unwrap();
        assert!(holds(&path(&["/elsewhere", &spelled]), &directory));
        assert!(holds(
            &path(&[&format!("{spelled}{separator}")]),
            &directory
        ));
        assert!(!holds(&path(&["/elsewhere"]), &directory));
        assert!(!holds(&path(&[&format!("{spelled}-other")]), &directory));
    }

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
