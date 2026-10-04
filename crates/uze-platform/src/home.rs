//! The user's home directory.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

/// The user's home directory, the one place every part of UZE asks for it:
/// `$HOME` on Unix, where tests and wrappers point it elsewhere on purpose;
/// the profile directory on Windows, where every harness keeps its
/// configuration, whatever `HOME` a POSIX-flavoured shell may export.
pub fn user_home() -> Option<PathBuf> {
    imp::user_home().filter(|home| !home.as_os_str().is_empty())
}

/// `path` as a person reads it, the way a shell prompt shows it: under the
/// home directory it starts with `~`, in this platform's separator.
pub fn shorten(path: &Path) -> String {
    let Some(rest) =
        user_home().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
    else {
        return path.display().to_string();
    };
    if rest.as_os_str().is_empty() {
        "~".to_owned()
    } else {
        format!("~{MAIN_SEPARATOR}{}", rest.display())
    }
}

/// What a person typed, resolved against the home directory: a leading `~`
/// (followed by either separator) and a bare relative path both name
/// something inside it.
pub fn expand(typed: &str) -> PathBuf {
    let path = PathBuf::from(typed);
    match (typed.strip_prefix('~'), user_home()) {
        (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', MAIN_SEPARATOR])),
        (None, Some(home)) if path.is_relative() => home.join(path),
        _ => path,
    }
}

#[cfg(unix)]
mod imp {
    use std::path::PathBuf;

    pub(super) fn user_home() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    pub(super) fn user_home() -> Option<PathBuf> {
        std::env::home_dir()
    }
}
