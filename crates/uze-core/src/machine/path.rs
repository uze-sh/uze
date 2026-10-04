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

/// Whether two spellings name the same path, by the platform's rules.
pub fn same_path(a: &Path, b: &Path) -> bool {
    identity(a) == identity(b)
}
