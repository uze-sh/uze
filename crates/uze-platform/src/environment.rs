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

/// Takes `directory` off the `PATH` every shell opened from now on searches,
/// wherever an entry names it; whether one did. The undoing of
/// [`add_to_user_path`]: on Windows the user's `Path` in the registry, its
/// other entries kept as stored, and running programs told it changed. On
/// Unix nothing was ever added, so nothing is taken away.
pub fn remove_from_user_path(directory: &std::path::Path) -> std::io::Result<bool> {
    imp::remove_from_user_path(directory)
}

/// Windows' alone: its user `Path` is the only search path UZE adds to.
#[cfg(windows)]
pub(crate) use imp::holds;

/// Taking entries off a search path as the registry stores it, in UTF-16
/// units: Windows' alone, written over units rather than over Windows so
/// every platform's tests read it.
#[cfg_attr(not(windows), allow(dead_code))]
mod stored_path {
    const SEPARATOR: u16 = b';' as u16;
    const QUOTE: u16 = b'"' as u16;

    /// What taking entries off a stored search path came to.
    #[derive(Debug, Eq, PartialEq)]
    pub(super) enum Removal {
        /// No entry was taken off.
        Unchanged,
        /// The path without them: every other entry's units as stored, in
        /// order, joined as they were.
        Removed(Vec<u16>),
        /// The path cannot be split or read without guessing — a quote
        /// that never closes, an entry that is not UTF-16 — so it is to be
        /// left exactly as it is.
        Unreadable,
    }

    /// Takes off every entry `names` answers true for, given the entry as
    /// text, quotes and all, for a reader that knows what they mean. An
    /// entry is what lies between two `;` outside double quotes, so a
    /// quoted directory holding a `;` is one entry; an empty one is kept,
    /// as stored.
    pub(super) fn remove_entries(stored: &[u16], names: impl Fn(&str) -> bool) -> Removal {
        let Some(entries) = split(stored) else {
            return Removal::Unreadable;
        };
        let mut kept: Vec<&[u16]> = Vec::with_capacity(entries.len());
        for entry in &entries {
            let Ok(text) = String::from_utf16(entry) else {
                return Removal::Unreadable;
            };
            if text.is_empty() || !names(&text) {
                kept.push(entry);
            }
        }
        if kept.len() == entries.len() {
            return Removal::Unchanged;
        }
        Removal::Removed(kept.join(&SEPARATOR))
    }

    fn split(stored: &[u16]) -> Option<Vec<&[u16]>> {
        let mut entries = Vec::new();
        let mut start = 0;
        let mut quoted = false;
        for (at, &unit) in stored.iter().enumerate() {
            match unit {
                QUOTE => quoted = !quoted,
                SEPARATOR if !quoted => {
                    entries.push(&stored[start..at]);
                    start = at + 1;
                }
                _ => {}
            }
        }
        if quoted {
            return None;
        }
        entries.push(&stored[start..]);
        Some(entries)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn units(text: &str) -> Vec<u16> {
            text.encode_utf16().collect()
        }

        fn naming(directory: &'static str) -> impl Fn(&str) -> bool {
            move |entry| {
                entry
                    .replace('"', "")
                    .trim_end_matches('\\')
                    .eq_ignore_ascii_case(directory)
            }
        }

        #[test]
        fn every_entry_naming_it_goes_and_the_rest_stay_as_stored() {
            let stored = units(r"C:\a;%HOME%\b;C:\uze\bin;;C:\UZE\BIN\;C:\c");
            assert_eq!(
                remove_entries(&stored, naming(r"C:\uze\bin")),
                Removal::Removed(units(r"C:\a;%HOME%\b;;C:\c"))
            );
        }

        #[test]
        fn a_path_without_it_is_unchanged() {
            let stored = units(r"C:\a;C:\b");
            assert_eq!(
                remove_entries(&stored, naming(r"C:\uze\bin")),
                Removal::Unchanged
            );
        }

        #[test]
        fn a_quoted_entry_holding_a_separator_is_one_entry() {
            let stored = units(r#"C:\a;"C:\x;y";C:\uze\bin"#);
            assert_eq!(
                remove_entries(&stored, naming(r"C:\uze\bin")),
                Removal::Removed(units(r#"C:\a;"C:\x;y""#))
            );
            assert_eq!(
                remove_entries(&units(r#""C:\uze\bin";C:\a"#), naming(r"C:\uze\bin")),
                Removal::Removed(units(r"C:\a"))
            );
            // Split on the `;` inside the quotes, `C:\x` would be taken off.
            assert_eq!(
                remove_entries(&units(r#""C:\x;y""#), naming(r"C:\x")),
                Removal::Unchanged
            );
        }

        #[test]
        fn an_entry_that_is_not_text_leaves_the_path_alone() {
            let mut stored = units(r"C:\uze\bin;");
            // A lone surrogate: no string holds it, so a path read through
            // one would come back changed, and the entry cannot be judged.
            stored.push(0xD800);
            assert_eq!(
                remove_entries(&stored, naming(r"C:\uze\bin")),
                Removal::Unreadable
            );
        }

        #[test]
        fn a_quote_that_never_closes_leaves_the_path_alone() {
            let stored = units(r#"C:\uze\bin;"C:\a"#);
            assert_eq!(
                remove_entries(&stored, naming(r"C:\uze\bin")),
                Removal::Unreadable
            );
        }
    }
}

#[cfg(unix)]
mod imp {
    use std::ffi::OsString;

    pub(super) fn path_of_a_new_shell() -> Option<OsString> {
        None
    }

    pub(super) fn add_to_user_path(_directory: &std::path::Path) -> std::io::Result<bool> {
        Ok(false)
    }

    pub(super) fn remove_from_user_path(_directory: &std::path::Path) -> std::io::Result<bool> {
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
        write_user_path(&path)?;
        Ok(true)
    }

    /// Every entry that names `directory`, as a new shell would expand it,
    /// goes; the rest are written back exactly as stored, unit for unit. A
    /// `Path` that cannot be split without guessing is left as it is, and
    /// the error says so.
    pub(super) fn remove_from_user_path(directory: &std::path::Path) -> std::io::Result<bool> {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        use super::stored_path::{Removal, remove_entries};

        let Some(stored) = registry_string(HKEY_CURRENT_USER, USER, "Path", RRF_NOEXPAND)? else {
            return Ok(false);
        };
        let units: Vec<u16> = stored.encode_wide().collect();
        match remove_entries(&units, |entry| holds(OsStr::new(&expand(entry)), directory)) {
            Removal::Unchanged => Ok(false),
            Removal::Removed(kept) => {
                write_user_path(&OsString::from_wide(&kept))?;
                Ok(true)
            }
            Removal::Unreadable => Err(std::io::Error::other(format!(
                "the user Path cannot be split into entries without changing it, so it was \
                 left as it is: take {} off it yourself",
                directory.display()
            ))),
        }
    }

    /// One entry with its `%VARIABLE%`s expanded from this process's
    /// environment, as the shell reading it would.
    fn expand(entry: &str) -> String {
        let mut expanded = String::new();
        let mut rest = entry;
        while let Some(start) = rest.find('%') {
            expanded.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('%') {
                Some(end) => {
                    let name = &after[..end];
                    match std::env::var(name) {
                        Ok(value) => expanded.push_str(&value),
                        Err(_) => {
                            expanded.push('%');
                            expanded.push_str(name);
                            expanded.push('%');
                        }
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    expanded.push('%');
                    rest = after;
                }
            }
        }
        expanded.push_str(rest);
        expanded
    }

    /// Writes the user's `Path` as an expandable string, and tells running
    /// programs the environment changed.
    fn write_user_path(path: &OsStr) -> std::io::Result<()> {
        let value = wide(path);
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
        Ok(())
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
