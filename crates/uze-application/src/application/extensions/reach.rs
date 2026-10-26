//! `read:checkout` answered over one checkout: what a script's
//! `read_checkout` and `list_checkout` reach, and no further.
//!
//! `uze-script` has already refused an absolute path and a `..`. What is
//! left is the filesystem's half: a link inside the checkout may point
//! anywhere, so every path is resolved through its links and must still
//! land beneath the root, and what is read or listed is bounded before it
//! reaches the engine.

use std::io::Read as _;
use std::path::PathBuf;

use uze_script::{CheckoutPath, Entry, EntryKind, Reach, ReachError};

/// The most of one file a script is handed: the engine's own bound on a
/// string, so a read never hands over what the script could not hold.
pub const CHECKOUT_READ_LIMIT: u64 = 1024 * 1024;

/// The most entries one listing hands a script: the engine's bound on an
/// array.
pub const CHECKOUT_LIST_LIMIT: usize = 10_000;

/// A checkout's files, as a script under `read:checkout` may reach them.
#[derive(Clone, Debug)]
pub struct CheckoutReach {
    /// Canonical, as [`super::ExtensionCheckout`] resolved it.
    root: PathBuf,
}

impl CheckoutReach {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Where `path` lands once its links are followed, refused when that
    /// is not beneath the root.
    fn resolve(&self, path: &CheckoutPath) -> Result<PathBuf, ReachError> {
        let asked = path.as_str();
        let joined = if asked.is_empty() {
            self.root.clone()
        } else {
            self.root.join(asked)
        };
        let landed = uze_platform::path::canonical(&joined)
            .map_err(|error| ReachError(format!("`{asked}`: {}", plain(&error))))?;
        if !uze_platform::path::is_within(&landed, &self.root) {
            return Err(ReachError(format!(
                "`{asked}` leads outside the checkout, and is not read"
            )));
        }
        Ok(landed)
    }
}

impl Reach for CheckoutReach {
    fn read_checkout(&self, path: &CheckoutPath) -> Result<String, ReachError> {
        let asked = path.as_str();
        let landed = self.resolve(path)?;
        if !landed.is_file() {
            return Err(ReachError(format!("`{asked}` is not a file")));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&landed)
            .and_then(|file| file.take(CHECKOUT_READ_LIMIT + 1).read_to_end(&mut bytes))
            .map_err(|error| ReachError(format!("`{asked}`: {}", plain(&error))))?;
        if bytes.len() as u64 > CHECKOUT_READ_LIMIT {
            return Err(ReachError(format!("`{asked}` is larger than 1 MiB")));
        }
        String::from_utf8(bytes).map_err(|_| ReachError(format!("`{asked}` is not text")))
    }

    fn list_checkout(&self, path: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        let asked = path.as_str();
        let landed = self.resolve(path)?;
        let entries = std::fs::read_dir(&landed)
            .map_err(|error| ReachError(format!("`{asked}`: {}", plain(&error))))?;
        let mut listed = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            if listed.len() == CHECKOUT_LIST_LIMIT {
                return Err(ReachError(format!(
                    "`{asked}` holds more than {CHECKOUT_LIST_LIMIT} entries"
                )));
            }
            let kind = match entry.file_type() {
                Ok(kind) if kind.is_symlink() => EntryKind::Link,
                Ok(kind) if kind.is_dir() => EntryKind::Directory,
                Ok(kind) if kind.is_file() => EntryKind::File,
                _ => EntryKind::Other,
            };
            listed.push(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                kind,
            });
        }
        listed.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(listed)
    }
}

/// An I/O error as a script reads it: what happened, never the absolute
/// path the host resolved.
fn plain(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "not found".to_owned(),
        std::io::ErrorKind::PermissionDenied => "not permitted".to_owned(),
        _ => "cannot be read".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn path(text: &str) -> CheckoutPath {
        CheckoutPath::parse(text).unwrap()
    }

    fn checkout(label: &str) -> (PathBuf, CheckoutReach) {
        let scratch = uze_platform::path::canonical(&uze_testkit::temp::scratch(label)).unwrap();
        let root = scratch.join("checkout");
        fs::create_dir_all(root.join(".planning/phases")).unwrap();
        fs::write(root.join(".planning/STATE.md"), "one\ntwo\n").unwrap();
        fs::write(scratch.join("secret"), "outside").unwrap();
        (scratch, CheckoutReach::new(root))
    }

    #[test]
    fn a_file_beneath_the_root_is_read_and_listed() {
        let (scratch, reach) = checkout("reach-read");
        assert_eq!(
            reach.read_checkout(&path(".planning/STATE.md")).unwrap(),
            "one\ntwo\n"
        );
        let names: Vec<_> = reach
            .list_checkout(&path(".planning"))
            .unwrap()
            .into_iter()
            .map(|entry| (entry.name, entry.kind))
            .collect();
        assert_eq!(
            names,
            [
                ("STATE.md".to_owned(), EntryKind::File),
                ("phases".to_owned(), EntryKind::Directory)
            ]
        );
        assert!(reach.read_checkout(&path("missing.md")).is_err());
        assert!(reach.read_checkout(&path(".planning")).is_err());
        fs::remove_dir_all(scratch).unwrap();
    }

    // Making a symbolic link takes no privilege on Unix; on Windows it
    // needs developer mode, which a test machine may not have.
    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_root_is_not_followed() {
        let (scratch, reach) = checkout("reach-link");
        std::os::unix::fs::symlink(scratch.join("secret"), reach.root.join("notes")).unwrap();
        std::os::unix::fs::symlink(&scratch, reach.root.join("up")).unwrap();
        std::os::unix::fs::symlink(".planning/STATE.md", reach.root.join("state")).unwrap();
        let refused = reach.read_checkout(&path("notes")).unwrap_err();
        assert!(refused.0.contains("outside the checkout"), "{refused}");
        assert!(!refused.0.contains("outside\""), "nothing of the target");
        assert!(reach.list_checkout(&path("up")).is_err());
        assert_eq!(reach.read_checkout(&path("state")).unwrap(), "one\ntwo\n");
        let listed = reach.list_checkout(&path("")).unwrap();
        assert!(
            listed
                .iter()
                .any(|entry| entry.name == "notes" && entry.kind == EntryKind::Link)
        );
        fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn a_file_past_the_bound_or_not_text_is_refused() {
        let (scratch, reach) = checkout("reach-bounds");
        fs::write(
            reach.root.join("large"),
            vec![b'a'; CHECKOUT_READ_LIMIT as usize + 1],
        )
        .unwrap();
        fs::write(reach.root.join("binary"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(
            reach
                .read_checkout(&path("large"))
                .unwrap_err()
                .0
                .contains("larger than 1 MiB")
        );
        assert!(
            reach
                .read_checkout(&path("binary"))
                .unwrap_err()
                .0
                .contains("not text")
        );
        fs::remove_dir_all(scratch).unwrap();
    }
}
