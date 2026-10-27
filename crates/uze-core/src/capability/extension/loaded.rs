//! An extension's directory read once into memory, and the digest of
//! exactly those bytes.
//!
//! Consent is given over a digest (`extension-platform` D5), so what
//! compiles and runs has to be what was digested: a second read of disk
//! could find a file swapped in between. The directory is read whole,
//! bounded, and everything after works from what this holds.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::Diagnostic;

/// The most files an extension's directory may hold. An extension is a
/// manifest, a few views, their scripts and fixtures.
pub const LOAD_FILES: usize = 256;
/// The most bytes they may hold together: room for fixtures of 1 MiB each
/// and the views beside them.
pub const LOAD_BYTES: u64 = 8 * 1024 * 1024;
/// How deep directories may nest beneath the extension's own.
pub const LOAD_DEPTH: usize = 4;

/// An extension's files as they were read, by path relative to its
/// directory in `/` spelling, and the digest of exactly these bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Loaded {
    files: BTreeMap<String, Vec<u8>>,
    digest: String,
}

impl Loaded {
    /// Reads every file beneath `dir`, refusing a directory past
    /// [`LOAD_FILES`], [`LOAD_BYTES`] or [`LOAD_DEPTH`] rather than
    /// digesting part of it. A link is never followed nor loaded: a file
    /// an extension names must be its own.
    pub fn read(dir: &Path) -> Result<Self, Diagnostic> {
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for (relative, path) in walk(dir)? {
            let mut bytes = Vec::new();
            // No further than the bound allows: one file of gigabytes is
            // past it after its first eight mebibytes, not after all of it.
            std::fs::File::open(&path)
                .and_then(|file| file.take(LOAD_BYTES - total + 1).read_to_end(&mut bytes))
                .map_err(|error| {
                    Diagnostic::whole(&relative, format!("cannot be read: {error}"), None)
                })?;
            total += bytes.len() as u64;
            if total > LOAD_BYTES {
                return Err(too_large());
            }
            files.insert(relative, bytes);
        }
        Ok(Self::from_files(files))
    }

    /// Files already in memory, as an editor or a test holds them.
    pub fn from_files(files: BTreeMap<String, Vec<u8>>) -> Self {
        let digest = crate::digest::files_sha256(
            files
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
        );
        Self { files, digest }
    }

    /// `sha256:<hex>` over the sorted paths and their bytes.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The bytes of `file`, relative to the extension's directory.
    pub fn bytes(&self, file: &str) -> Option<&[u8]> {
        self.files.get(&normalized(file)).map(Vec::as_slice)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

/// `./views/a.kdl` and `views/a.kdl` are one file.
fn normalized(file: &str) -> String {
    file.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn too_large() -> Diagnostic {
    Diagnostic::whole(
        ".",
        format!(
            "the extension's directory holds more than {LOAD_FILES} files or {} MiB, \
             nested at most {LOAD_DEPTH} deep",
            LOAD_BYTES / (1024 * 1024)
        ),
        Some("keep only the extension's own files in its directory".to_owned()),
    )
}

/// The files beneath `dir` by relative path, links skipped.
fn walk(dir: &Path) -> Result<Vec<(String, PathBuf)>, Diagnostic> {
    let mut files = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), String::new(), 0usize)];
    while let Some((directory, prefix, depth)) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            Diagnostic::whole(
                if prefix.is_empty() { "." } else { &prefix },
                format!("cannot be read: {error}"),
                None,
            )
        })?;
        for entry in entries.filter_map(Result::ok) {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                if depth >= LOAD_DEPTH {
                    return Err(too_large());
                }
                pending.push((entry.path(), relative, depth + 1));
            } else if kind.is_file() {
                files.push((relative, entry.path()));
                if files.len() > LOAD_FILES {
                    return Err(too_large());
                }
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn extension(label: &str) -> PathBuf {
        let dir = uze_testkit::temp::scratch(label);
        fs::create_dir_all(dir.join("views")).unwrap();
        fs::write(dir.join("extension.kdl"), "protocol 1\n").unwrap();
        fs::write(dir.join("views/board.kdl"), "Text \"a\"").unwrap();
        dir
    }

    #[test]
    fn the_digest_is_of_the_bytes_read() {
        let dir = extension("loaded-digest");
        let first = Loaded::read(&dir).unwrap();
        assert_eq!(first.bytes("views/board.kdl"), Some(&b"Text \"a\""[..]));
        assert_eq!(
            first.bytes("./views/board.kdl"),
            first.bytes("views/board.kdl")
        );
        assert_eq!(first.digest(), Loaded::read(&dir).unwrap().digest());

        fs::write(dir.join("views/board.kdl"), "Text \"b\"").unwrap();
        let saved = Loaded::read(&dir).unwrap();
        assert_ne!(first.digest(), saved.digest(), "a saved file moves it");
        assert_eq!(
            first.bytes("views/board.kdl"),
            Some(&b"Text \"a\""[..]),
            "what was read stays what was read"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_directory_past_its_bounds_is_refused_whole() {
        let dir = extension("loaded-bounds");
        fs::create_dir_all(dir.join("a/b/c/d/e")).unwrap();
        fs::write(dir.join("a/b/c/d/e/deep.kdl"), "").unwrap();
        assert!(Loaded::read(&dir).is_err(), "too deep");
        fs::remove_dir_all(dir.join("a")).unwrap();

        for n in 0..=LOAD_FILES {
            fs::write(dir.join(format!("f{n}")), "").unwrap();
        }
        assert!(Loaded::read(&dir).is_err(), "too many files");
        for n in 0..=LOAD_FILES {
            fs::remove_file(dir.join(format!("f{n}"))).unwrap();
        }

        let blob = fs::File::create(dir.join("blob.bin")).unwrap();
        blob.set_len(LOAD_BYTES + 1).unwrap();
        let refusal = Loaded::read(&dir).unwrap_err();
        assert!(refusal.message.contains("8 MiB"), "{refusal}");
        fs::remove_dir_all(dir).unwrap();
    }

    // A symbolic link needs a privilege on Windows that a test run may
    // lack; Unix makes one freely.
    #[cfg(unix)]
    #[test]
    fn a_link_is_neither_followed_nor_loaded() {
        let dir = extension("loaded-link");
        let outside = dir.parent().unwrap().join("loaded-link-outside.kdl");
        fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("views/linked.kdl")).unwrap();
        let loaded = Loaded::read(&dir).unwrap();
        assert_eq!(loaded.bytes("views/linked.kdl"), None);
        assert_eq!(
            loaded.paths().collect::<Vec<_>>(),
            ["extension.kdl", "views/board.kdl"]
        );
        fs::remove_file(outside).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
