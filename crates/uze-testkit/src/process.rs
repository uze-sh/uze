//! Executables a test runs without ever having held a descriptor to them.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};

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

/// Points a child process at a test's own `HOME`, and every XDG base
/// directory and Git configuration with it.
pub trait IsolatedHome {
    fn isolated_home(&mut self, home: impl AsRef<OsStr>) -> &mut Self;
}

impl IsolatedHome for Command {
    fn isolated_home(&mut self, home: impl AsRef<OsStr>) -> &mut Self {
        self.env("HOME", home);
        for key in XDG_BASE_DIRS.into_iter().chain(GIT_CONFIG_REDIRECTS) {
            self.env_remove(key);
        }
        self
    }
}

/// Writes `bytes` to `path` as an executable, through a process that has
/// exited before this returns.
///
/// A file cannot be `exec`ed while any descriptor to it is open for
/// writing, and a test binary spawns from several threads at once: a
/// descriptor this process opens is copied into every child a sibling test
/// forks, until that child's own `exec` closes it. That instant is the
/// kernel's `ETXTBSY`, and it belongs to the harness, not to the test.
/// Writing through a child of our own leaves the descriptor in a process
/// that is gone — waited on — by the time anything runs the file.
pub fn install_executable(path: &Path, bytes: &[u8]) {
    let mut writer = Command::new("sh")
        .args(["-c", r#"cat > "$1" && chmod 0755 "$1""#, "sh"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("a POSIX shell is on PATH");
    let mut stdin = writer.stdin.take().expect("stdin is piped");
    std::io::Write::write_all(&mut stdin, bytes).expect("the writer reads its whole input");
    drop(stdin);
    let status = writer.wait().expect("the writer is waited on");
    assert!(status.success(), "installing {}: {status}", path.display());
}
