//! The tools the operating system itself ships, and how a person gets the
//! ones it does not.

use std::process::Command;

/// `name` as the operating system ships it, never another program of the
/// same name earlier on `PATH`.
pub fn system(name: &str) -> Command {
    imp::system(name)
}

/// How a person installs Git here, as a command they can type.
pub const GIT_INSTALL_HINT: &str = imp::GIT_INSTALL_HINT;

#[cfg(unix)]
mod imp {
    use std::process::Command;

    /// `curl` and `tar` are the system's own wherever they are found.
    pub(super) fn system(name: &str) -> Command {
        Command::new(name)
    }

    pub(super) const GIT_INSTALL_HINT: &str = "install it with your package manager";
}

#[cfg(windows)]
mod imp {
    use std::{path::PathBuf, process::Command};

    /// System32, by path: under Windows PowerShell `curl` is an alias for
    /// something else, and a `tar` earlier on `PATH` (Git's GNU tar) cannot
    /// read a zip.
    pub(super) fn system(name: &str) -> Command {
        let root = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        Command::new(root.join("System32").join(format!("{name}.exe")))
    }

    pub(super) const GIT_INSTALL_HINT: &str = "winget install Git.Git";
}
