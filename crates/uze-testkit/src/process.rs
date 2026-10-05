//! Executables a test runs without ever having held a descriptor to them.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Every variable a user's home is read from, each with what it names
/// under a test's `home`, the directories made: a test's home is all of
/// them, or the machine's own leaks in through the one left out.
///
/// `HOME` on Unix (and by Git everywhere), `USERPROFILE` on Windows. A
/// Windows profile is also its per-user application directories, and they
/// must exist: .NET answers an empty path for a known folder that does
/// not, and PowerShell then writes its module cache relative to wherever it
/// was started — into the checkout a pane opened in.
pub fn profile(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut variables = vec![
        ("HOME", home.to_path_buf()),
        ("USERPROFILE", home.to_path_buf()),
    ];
    for (variable, directory) in uze_platform::home::PROFILE_DIRECTORIES {
        let directory = home.join(directory);
        let _ = std::fs::create_dir_all(&directory);
        variables.push((variable, directory));
    }
    variables
}

/// Held by every test that installs a Ctrl+C watch or raises an interrupt:
/// the watch is one per process, so a test raising its interrupt would
/// otherwise end the child a concurrent test is waiting on.
pub fn interrupts() -> std::sync::MutexGuard<'static, ()> {
    static INTERRUPTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    INTERRUPTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A relative path written with `/` as this platform spells it, for
/// comparing against what the product prints or records.
pub fn native(relative: &str) -> String {
    relative
        .split('/')
        .collect::<PathBuf>()
        .display()
        .to_string()
}

/// A `PATH` holding the system's own tools and Git, and nothing a developer
/// installed: what a test that must not find a harness by accident runs on.
pub fn system_path() -> OsString {
    std::env::join_paths(system_directories())
        .unwrap_or_else(|error| panic!("system PATH does not join: {error}"))
}

/// `first`, then [`system_path`]: a test's own stand-ins ahead of the
/// system's tools.
pub fn path_with(first: &[&Path]) -> OsString {
    let directories = first
        .iter()
        .map(|directory| directory.to_path_buf())
        .chain(system_directories());
    std::env::join_paths(directories).unwrap_or_else(|error| panic!("PATH does not join: {error}"))
}

/// The system's own tools, after the directory Git is in where the
/// ambient `PATH` has it: a test runs Git, and a system without it in its
/// own directories (Windows) still finds it.
fn system_directories() -> Vec<PathBuf> {
    uze_platform::executable::on_path("git")
        .and_then(|git| git.parent().map(Path::to_path_buf))
        .into_iter()
        .chain(uze_platform::tools::system_directories())
        .collect()
}

/// The base directories a harness derives from `HOME` unless they are set.
/// A test's `HOME` isolates nothing while these still name the machine's
/// own: a CI runner sets `XDG_CONFIG_HOME`, and OpenCode's skills, agents
/// and `opencode.json` follow it there.
pub const XDG_BASE_DIRS: [&str; 4] = [
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
];

/// What points Git past `HOME` at a global or system configuration. A
/// sibling test holding a [`crate::git::Repository`] has these set for the
/// whole process, and a child that inherited them read that fixture's
/// configuration instead of the `.gitconfig` in the home it was given.
pub const GIT_CONFIG_REDIRECTS: [&str; 2] = ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"];

/// Keeps Git off the machine's system configuration, which a test's home
/// does not replace: Git for Windows ships one setting `core.autocrlf`, so
/// a file an agent wrote with `\n` would read back with `\r\n`.
pub const GIT_CONFIG_NOSYSTEM: (&str, &str) = ("GIT_CONFIG_NOSYSTEM", "1");

/// Points a child process at a test's own `HOME`, and every XDG base
/// directory and Git configuration with it: the machine's is never read.
pub trait IsolatedHome {
    fn isolated_home(&mut self, home: impl AsRef<OsStr>) -> &mut Self;
}

impl IsolatedHome for Command {
    fn isolated_home(&mut self, home: impl AsRef<OsStr>) -> &mut Self {
        for (key, value) in profile(Path::new(home.as_ref())) {
            self.env(key, value);
        }
        for key in XDG_BASE_DIRS.into_iter().chain(GIT_CONFIG_REDIRECTS) {
            self.env_remove(key);
        }
        self.env(GIT_CONFIG_NOSYSTEM.0, GIT_CONFIG_NOSYSTEM.1)
    }
}

/// Writes `bytes` to `path` as an executable, through a process that has
/// exited before this returns (see the Unix half for why).
pub fn install_executable(path: &Path, bytes: &[u8]) {
    uze_platform::executable::install(path, bytes)
        .unwrap_or_else(|error| panic!("installing {}: {error}", path.display()));
}
