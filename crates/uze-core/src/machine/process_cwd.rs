//! Where this user's processes are working.
//!
//! A checkout somebody is working in must not be reset or removed, and
//! "somebody" is not only a pane UZE drew: a subagent's shell, an editor or
//! a build started from a terminal UZE never saw are all a process whose
//! working directory is inside it. The answer is read from the process
//! table, once per decision, and fails closed: when the table cannot be
//! read at all, every directory is taken to be in use.

use crate::path::Canonical as _;
use std::path::{Path, PathBuf};

/// The directories this user's processes were working in when observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Presence {
    Known(Vec<PathBuf>),
    /// The process table could not be read.
    Unknown,
}

impl Presence {
    /// Reads the process table. This process and the ones it started are
    /// left out: the `uze` deciding, and the Git it runs inside the very
    /// checkouts it is deciding about, are not somebody at work there.
    pub fn observe() -> Self {
        match working_directories() {
            Some(directories) => Self::Known(directories),
            None => Self::Unknown,
        }
    }

    /// The process table, plus directories the caller already knows
    /// somebody is in — a pane the terminal drew, before its shell is in
    /// the table.
    pub fn observe_with(known: &[PathBuf]) -> Self {
        match Self::observe() {
            Self::Known(mut directories) => {
                directories.extend_from_slice(known);
                Self::Known(directories)
            }
            Self::Unknown => Self::Unknown,
        }
    }

    /// Whether any observed process works in `directory` or below it.
    pub fn inside(&self, directory: &Path) -> bool {
        let Self::Known(directories) = self else {
            return true;
        };
        let canonical = directory
            .canonical()
            .unwrap_or_else(|_| directory.to_path_buf());
        directories
            .iter()
            .any(|working| working.starts_with(&canonical) || working.starts_with(directory))
    }
}

/// Every working directory of this user's other processes, or `None` when
/// the table could not be enumerated. A process that exits while being
/// read, or whose directory the platform withholds, is skipped: those are
/// ordinary on a desktop, and failing closed on them would hold every
/// checkout forever.
fn working_directories() -> Option<Vec<PathBuf>> {
    uze_platform::probe::working_directories()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_process_working_in_a_directory_is_seen_there() {
        let directory = uze_testkit::temp::scratch("process-cwd");
        std::fs::create_dir_all(&directory).unwrap();
        // Not a child of this process once `sh` has forked it off and
        // exited, which is what somebody else's process looks like.
        let mut starter = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 30 >/dev/null 2>&1 & echo $!")
            .current_dir(&directory)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut spelled = String::new();
        std::io::Read::read_to_string(starter.stdout.as_mut().unwrap(), &mut spelled).unwrap();
        starter.wait().unwrap();
        let sleeper: libc::pid_t = spelled.trim().parse().unwrap();

        let seen = Presence::observe().inside(&directory);
        // SAFETY: `sleeper` is the positive pid of the process started above.
        unsafe { libc::kill(sleeper, libc::SIGKILL) };
        assert!(seen, "a process working in the directory was not seen");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_directory_nobody_works_in_is_free() {
        let directory = uze_testkit::temp::scratch("process-cwd-empty");
        std::fs::create_dir_all(&directory).unwrap();
        if Presence::observe() != Presence::Unknown {
            assert!(!Presence::observe().inside(&directory));
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_unreadable_process_table_holds_everything() {
        assert!(Presence::Unknown.inside(Path::new("/anywhere")));
    }
}
