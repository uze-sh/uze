//! The read-only grant the workspace client makes, against the real disk
//! and the real Git, for the tests about what a surface *reads*. A fake is
//! the right tool for testing a view; these test what it is handed.
//!
//! One for every extension, answering what `src/ui/extension_host.rs`
//! answers for an ordinary checkout: two copies had already begun to drift
//! from it, and a double that disagrees with production proves the wrong
//! thing. Its refusals are not here — no size cap on a file read, no
//! refusal of a checkout Git does not anchor, no shared `status` — so a
//! test that needs one of those belongs beside the production host.

use std::path::{Path, PathBuf};

use crate::{DirEntry, Host, Unreadable, shared::highlight::FALLBACK_SYNTAX_THEME};

pub(crate) struct DiskHost;

impl Host for DiskHost {
    fn git(&self, root: &Path, args: &[&str], answers: &[i32]) -> Result<String, String> {
        uze_git::read(root, args)
            .map_err(|error| error.to_string())?
            .or_exit(answers)
    }

    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        uze_git::repository::root(path)
    }

    fn read_file(&self, path: &Path) -> Result<String, Unreadable> {
        std::fs::read_to_string(path).map_err(|error| match error.kind() {
            std::io::ErrorKind::InvalidData => Unreadable::NotText,
            _ => Unreadable::Failed(error.to_string()),
        })
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, String> {
        let mut entries: Vec<DirEntry> = std::fs::read_dir(path)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| DirEntry {
                directory: entry.file_type().is_ok_and(|kind| kind.is_dir()),
                name: entry.file_name().to_string_lossy().into_owned(),
            })
            .collect();
        entries.sort();
        Ok(entries)
    }

    fn write_file(&self, _root: &Path, _path: &Path, _contents: &str) -> Result<(), String> {
        unreachable!("a test of what is read writes nothing")
    }

    fn delete_file(&self, _root: &Path, _path: &Path) -> Result<(), String> {
        unreachable!("a test of what is read deletes nothing")
    }

    fn restore_to_head(&self, _root: &Path, _paths: &[PathBuf]) -> Result<(), String> {
        unreachable!("a test of what is read restores nothing")
    }

    fn syntax_theme(&self) -> String {
        FALLBACK_SYNTAX_THEME.to_owned()
    }
}
