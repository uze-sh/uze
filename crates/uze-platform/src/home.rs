//! The user's home directory.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

/// The user's home directory, the one place every part of UZE asks for it:
/// `$HOME` on Unix, where tests and wrappers point it elsewhere on purpose;
/// the profile directory on Windows, where every harness keeps its
/// configuration, whatever `HOME` a POSIX-flavoured shell may export.
pub fn user_home() -> Option<PathBuf> {
    imp::user_home().filter(|home| !home.as_os_str().is_empty())
}

/// The variable this platform's home is read from, which a test or a
/// wrapper sets to move it.
pub const VARIABLE: &str = imp::VARIABLE;

/// Every variable a program here may read its home from: [`VARIABLE`], and
/// `HOME` on Windows too, which Git for Windows honours when it is set. A
/// child given a home of its own is given it in all of them.
pub const VARIABLES: &[&str] = imp::VARIABLES;

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
/// and a bare relative path both name something inside it. A rooted path is never bare, drive or not: `/srv` stays on
/// the current drive on Windows.
pub fn expand(typed: &str) -> PathBuf {
    let path = PathBuf::from(typed);
    match (typed.strip_prefix('~'), user_home()) {
        (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', MAIN_SEPARATOR])),
        (None, Some(home)) if !path.has_root() => home.join(path),
        _ => path,
    }
}

/// The directories a user's profile names by variable beside the home,
/// relative to it: `APPDATA` and `LOCALAPPDATA` on Windows, where a program
/// looks for them before it looks at the home; none on Unix.
pub const PROFILE_DIRECTORIES: &[(&str, &str)] = imp::PROFILE_DIRECTORIES;

/// A directory a long-running process can stand in without holding
/// anybody's work in use: `/` on Unix. Windows has none every user may
/// stand in that is nobody's checkout, so the person's home, theirs alone.
pub fn unclaimed_directory() -> PathBuf {
    imp::unclaimed_directory()
}

#[cfg(unix)]
mod imp {
    use std::path::PathBuf;

    pub(super) fn unclaimed_directory() -> PathBuf {
        PathBuf::from("/")
    }

    pub(super) const PROFILE_DIRECTORIES: &[(&str, &str)] = &[];

    pub(super) const VARIABLE: &str = "HOME";
    pub(super) const VARIABLES: &[&str] = &["HOME"];

    pub(super) fn user_home() -> Option<PathBuf> {
        std::env::var_os(VARIABLE).map(PathBuf::from)
    }
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    pub(super) fn unclaimed_directory() -> PathBuf {
        user_home().unwrap_or_else(std::env::temp_dir)
    }

    pub(super) const PROFILE_DIRECTORIES: &[(&str, &str)] = &[
        ("APPDATA", r"AppData\Roaming"),
        ("LOCALAPPDATA", r"AppData\Local"),
    ];

    pub(super) const VARIABLE: &str = "USERPROFILE";
    pub(super) const VARIABLES: &[&str] = &["USERPROFILE", "HOME"];

    /// `USERPROFILE`, then the profile the token names: what
    /// `std::env::home_dir` answers.
    pub(super) fn user_home() -> Option<PathBuf> {
        std::env::home_dir()
    }
}
