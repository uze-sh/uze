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

pub use uze_platform::{
    fs_name::file_name_for,
    path::{canonical, identity, is_anchored, same_path, strip_verbatim},
};

/// A relative path as every platform spells it: components joined by `/`.
/// What names a file inside a package — a digest, a resource's identity —
/// is the same on every machine, whichever separator measured it.
pub fn portable(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
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
