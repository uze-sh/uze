//! Paths as the platform compares them.
//!
//! On Windows one directory has many spellings: `canonicalize` answers with
//! a verbatim `\\?\C:\…` prefix, Git answers `C:/…` with forward slashes,
//! and the filesystem ignores case. Anything that *identifies* a path — a
//! project's id, a checkout's owner, a link's containment — must reduce
//! those spellings to one, or the same project is two projects depending on
//! which call produced its root. Anything that *shows* a path must not show
//! the verbatim prefix at all. On Unix every function here is the identity
//! it looks like.

use std::{
    io,
    path::{Path, PathBuf},
};

/// `fs::canonicalize`, without the verbatim prefix Windows adds where the
/// path does not need it.
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    path.canonicalize()
        .map(|canonical| strip_verbatim(&canonical))
}

/// [`canonical`] as a method, so a call site reads as the `canonicalize`
/// it replaces.
pub trait Canonical {
    fn canonical(&self) -> io::Result<PathBuf>;
}

impl Canonical for Path {
    fn canonical(&self) -> io::Result<PathBuf> {
        canonical(self)
    }
}

impl Canonical for PathBuf {
    fn canonical(&self) -> io::Result<PathBuf> {
        canonical(self)
    }
}

/// `\\?\C:\x` as `C:\x`, and `\\?\UNC\server\share` as `\\server\share`.
/// A verbatim path that would mean something else without its prefix — one
/// longer than the legacy limit, or naming a component Win32 would
/// reinterpret — is kept as it is.
pub fn strip_verbatim(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.as_os_str().to_string_lossy();
        let stripped = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else if let Some(rest) = text.strip_prefix(r"\\?\") {
            rest.to_owned()
        } else {
            return path.to_path_buf();
        };
        let reinterpreted = stripped.len() >= 260
            || stripped
                .split('\\')
                .any(|part| part.ends_with('.') || part.ends_with(' '));
        if reinterpreted {
            path.to_path_buf()
        } else {
            PathBuf::from(stripped)
        }
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// The one spelling a path is identified by: no verbatim prefix and, on
/// Windows, backslashes and lower case, since the filesystem distinguishes
/// neither.
pub fn identity(path: &Path) -> String {
    let stripped = strip_verbatim(path);
    let text = stripped.to_string_lossy();
    if cfg!(windows) {
        text.replace('/', "\\").to_lowercase()
    } else {
        text.into_owned()
    }
}

/// Whether `path` is anchored anywhere but where it is joined: an absolute
/// path, and on Windows also `\\etc` (rooted on the current drive) and
/// `C:etc` (relative to that drive's own directory). `is_absolute` says no
/// to both, and either one joined onto a root escapes it.
pub fn is_anchored(path: &Path) -> bool {
    path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::RootDir | std::path::Component::Prefix(_)
            )
        })
}

/// `label` as a name the host filesystem holds as an ordinary file or
/// directory name.
///
/// The identity on Unix. On Windows the characters NTFS refuses or reads as
/// syntax (`<>:"/\|?*`, and controls) become `-`, a trailing dot or space is
/// dropped, and a reserved device name gains a `_`: `flow:review` written
/// as-is would not fail, it would create an alternate data stream `review`
/// on a file named `flow`, which no harness reads.
pub fn file_name_for(label: &str) -> String {
    if !cfg!(windows) {
        return label.to_owned();
    }
    let mut name: String = label
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            control if control.is_control() => '-',
            other => other,
        })
        .collect();
    while name.ends_with(['.', ' ']) {
        name.pop();
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        // After the stem, not at the end: `COM1.md_` is still `COM1`.
        let at = name.find('.').unwrap_or(name.len());
        name.insert(at, '_');
    }
    name
}

/// Whether two spellings name the same path, by the platform's rules.
pub fn same_path(a: &Path, b: &Path) -> bool {
    identity(a) == identity(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_becomes_a_name_the_host_filesystem_holds() {
        if cfg!(windows) {
            assert_eq!(file_name_for("flow:review"), "flow-review");
            assert_eq!(file_name_for("con"), "con_");
            assert_eq!(file_name_for("COM1.md"), "COM1_.md");
            assert_eq!(file_name_for("trailing. "), "trailing");
        } else {
            assert_eq!(file_name_for("flow:review"), "flow:review");
        }
    }

    #[test]
    fn a_rooted_path_is_anchored_on_every_platform() {
        assert!(is_anchored(Path::new("/etc")));
        assert!(!is_anchored(Path::new("packages/inner")));
        assert!(!is_anchored(Path::new("../escape")));
        if cfg!(windows) {
            assert!(is_anchored(Path::new(r"\etc")));
            assert!(is_anchored(Path::new("C:etc")));
            assert!(is_anchored(Path::new(r"C:\etc")));
        }
    }

    #[test]
    fn spellings_of_one_path_share_an_identity() {
        if cfg!(windows) {
            assert!(same_path(
                Path::new(r"\\?\C:\Users\A\proj"),
                Path::new("c:/users/a/proj")
            ));
        } else {
            assert!(same_path(Path::new("/a/b"), Path::new("/a/b")));
            assert!(!same_path(Path::new("/a/B"), Path::new("/a/b")));
        }
    }
}
