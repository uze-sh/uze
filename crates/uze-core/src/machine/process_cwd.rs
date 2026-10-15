//! Where this user's processes are working.
//!
//! A checkout somebody is working in must not be reset or removed, and
//! "somebody" is not only a pane UZE drew: a subagent's shell, an editor or
//! a build started from a terminal UZE never saw are all a process whose
//! working directory is inside it. Somebody, though, is a person: a process
//! that holds a controlling terminal, in a session a keyboard reaches. A
//! daemon a harness leaves behind in a session of its own — handed to
//! `init`, standing in the checkout it was first started from for the rest
//! of its life — is nobody at work there, and counting it held the checkout
//! for as long as the daemon ran. The answer is read from the process
//! table, once per decision, and fails closed: when the table cannot be
//! read at all, every directory is taken to be in use, and a platform that
//! cannot say whether anybody is behind a process counts it.

use crate::path::Canonical as _;
use std::path::{Path, PathBuf};

/// Where this user's processes were working when observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Presence {
    Known(Vec<Occupant>),
    /// The process table could not be read.
    Unknown,
}

/// A directory one of this user's processes was working in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Occupant {
    pub path: PathBuf,
    /// Whether somebody is behind the process. False only for one the
    /// platform says holds no terminal: one it cannot say about counts.
    pub attended: bool,
}

impl Occupant {
    /// A process somebody is behind: a pane's shell, an editor, a build.
    pub fn somebody(path: PathBuf) -> Self {
        Self {
            path,
            attended: true,
        }
    }

    /// A process nobody is behind: a daemon in a session of its own.
    pub fn daemon(path: PathBuf) -> Self {
        Self {
            path,
            attended: false,
        }
    }
}

impl Presence {
    /// Reads the process table. This process and the ones it started are
    /// left out: the `uze` deciding, and the Git it runs inside the very
    /// checkouts it is deciding about, are not somebody at work there.
    pub fn observe() -> Self {
        match working_directories() {
            Some(directories) => Self::Known(
                directories
                    .into_iter()
                    .map(|directory| Occupant {
                        path: directory.path,
                        attended: directory.attended != Some(false),
                    })
                    .collect(),
            ),
            None => Self::Unknown,
        }
    }

    /// The process table, plus directories the caller already knows
    /// somebody is in — a pane the terminal drew, before its shell is in
    /// the table.
    pub fn observe_with(known: &[PathBuf]) -> Self {
        match Self::observe() {
            Self::Known(mut occupants) => {
                occupants.extend(known.iter().cloned().map(Occupant::somebody));
                Self::Known(occupants)
            }
            Self::Unknown => Self::Unknown,
        }
    }

    /// Whether somebody works in `directory` or below it. A process with
    /// no terminal behind it is nobody.
    pub fn inside(&self, directory: &Path) -> bool {
        self.any_inside(directory, |occupant| occupant.attended)
    }

    /// Whether any process at all works in `directory` or below it,
    /// attended or not: for a decision that moves a directory, which a
    /// daemon standing in it would follow without a word.
    pub fn anyone_inside(&self, directory: &Path) -> bool {
        self.any_inside(directory, |_| true)
    }

    fn any_inside(&self, directory: &Path, counts: impl Fn(&Occupant) -> bool) -> bool {
        let Self::Known(occupants) = self else {
            return true;
        };
        let canonical = directory
            .canonical()
            .unwrap_or_else(|_| directory.to_path_buf());
        occupants
            .iter()
            .filter(|occupant| counts(occupant))
            .any(|occupant| {
                occupant.path.starts_with(&canonical) || occupant.path.starts_with(directory)
            })
    }
}

/// Every working directory of this user's other processes, or `None` when
/// the table could not be enumerated. A process that exits while being
/// read, or whose directory the platform withholds, is skipped: those are
/// ordinary on a desktop, and failing closed on them would hold every
/// checkout forever.
fn working_directories() -> Option<Vec<uze_platform::probe::WorkingDirectory>> {
    uze_platform::probe::working_directories()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Starts `/bin/sleep` in `directory` the way somebody else's process
    /// looks: forked off by a shell, so it is no child of this one. `at`
    /// says what terminal the shell's session gets — a pseudoterminal of
    /// its own as its controlling terminal, the way a pane's shell holds
    /// one, or none at all, the way a daemon's does. The shell stays until
    /// the sleeper is gone: a session leader leaving takes the terminal
    /// from everything left in the session, which is the hangup this is
    /// not about. `sleep` is named by its path: other tests in this binary
    /// point `PATH` at a directory of shims while they run, and a `sleep`
    /// looked up then is not found, exits at once, and is never there to
    /// be seen.
    // A session of its own and a controlling terminal, as only Unix has.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn start_sleeper(
        directory: &Path,
        at: Terminal,
    ) -> (
        libc::pid_t,
        std::process::Child,
        Option<uze_testkit::process::Terminal>,
    ) {
        use std::os::unix::process::CommandExt as _;

        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("/bin/sleep 30 >/dev/null 2>&1 & echo $!; wait")
            .current_dir(directory)
            .stdout(std::process::Stdio::piped());
        let terminal = match at {
            Terminal::Own => {
                let terminal = uze_testkit::process::Terminal::open();
                terminal.seat(&mut command);
                Some(terminal)
            }
            Terminal::None => {
                // SAFETY: runs in the child between fork and exec; `setsid`
                // is async-signal-safe.
                unsafe {
                    command.pre_exec(|| {
                        if libc::setsid() < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
                None
            }
        };
        let mut shell = command.spawn().unwrap();
        let mut spelled = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(shell.stdout.as_mut().unwrap()),
            &mut spelled,
        )
        .unwrap();
        (spelled.trim().parse().unwrap(), shell, terminal)
    }

    // What a Unix session holds, or does not.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[derive(Clone, Copy)]
    enum Terminal {
        Own,
        None,
    }

    // Reads `/proc`, which Linux has and macOS answers an error for.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn describe(sleeper: libc::pid_t, directory: &Path) -> String {
        let proc = std::path::Path::new("/proc").join(sleeper.to_string());
        format!(
            "pid {sleeper}: cwd {:?}, stat {:?}, directory {:?}",
            std::fs::read_link(proc.join("cwd")),
            std::fs::read_to_string(proc.join("stat")).map(|stat| stat.trim().to_owned()),
            directory,
        )
    }

    // Signals a process, as only Unix does.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_process_working_in_a_directory_is_seen_there() {
        let directory = uze_testkit::temp::scratch("process-cwd");
        std::fs::create_dir_all(&directory).unwrap();
        let (sleeper, mut shell, _terminal) = start_sleeper(&directory, Terminal::Own);

        let seen = Presence::observe().inside(&directory);
        let state = describe(sleeper, &directory);
        // SAFETY: `sleeper` is the positive pid of the process started above.
        unsafe { libc::kill(sleeper, libc::SIGKILL) };
        shell.wait().unwrap();
        assert!(
            seen,
            "a process working in the directory was not seen: {state}"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    /// What a harness's daemon looks like once its pane is gone: a session
    /// of its own, no terminal, `init` for a parent, and the checkout it
    /// was started from as its working directory for the rest of its life.
    // Signals a process, as only Unix does.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_daemon_with_no_terminal_behind_it_holds_nothing() {
        let directory = uze_testkit::temp::scratch("process-cwd-daemon");
        std::fs::create_dir_all(&directory).unwrap();
        let (sleeper, mut shell, _) = start_sleeper(&directory, Terminal::None);

        let presence = Presence::observe();
        let state = describe(sleeper, &directory);
        // SAFETY: `sleeper` is the positive pid of the process started above.
        unsafe { libc::kill(sleeper, libc::SIGKILL) };
        shell.wait().unwrap();
        assert!(
            !presence.inside(&directory),
            "a daemon nobody is behind held the directory: {state}"
        );
        assert!(
            presence.anyone_inside(&directory),
            "yet a move of the directory would carry it along: {state}"
        );

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
