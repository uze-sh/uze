//! The tools the operating system itself ships, and how a person gets the
//! ones it does not.

use std::process::Command;

/// `name` as the operating system ships it, never another program of the
/// same name earlier on `PATH`.
pub fn system(name: &str) -> Command {
    Command::new(system_program(name))
}

/// The program [`system`] runs for `name`, for a caller that describes a
/// process rather than spawning it.
pub fn system_program(name: &str) -> std::path::PathBuf {
    imp::system_program(name)
}

/// How a person installs Git here, as a command they can type.
pub const GIT_INSTALL_HINT: &str = imp::GIT_INSTALL_HINT;

/// The Git settings under which a checkout holds a repository's files as
/// they were committed, on this platform: no line-ending conversion
/// anywhere; on Windows also no links Git cannot make without a privilege
/// (each becomes a file holding its target), and paths longer than the
/// Win32 limit. Pushed rather than left to a Git build's defaults, which
/// differ between builds.
pub const GIT_FAITHFUL_CHECKOUT: &[(&str, &str)] = imp::GIT_FAITHFUL_CHECKOUT;

#[cfg(unix)]
mod imp {
    use std::path::PathBuf;

    /// `curl` and `tar` are the system's own wherever they are found.
    pub(super) fn system_program(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    pub(super) const GIT_INSTALL_HINT: &str = "install it with your package manager";

    pub(super) const GIT_FAITHFUL_CHECKOUT: &[(&str, &str)] =
        &[("core.autocrlf", "false"), ("core.eol", "lf")];
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    /// System32, by path: under Windows PowerShell `curl` is an alias for
    /// something else, and a `tar` earlier on `PATH` (Git's GNU tar) cannot
    /// read a zip.
    pub(super) fn system_program(name: &str) -> PathBuf {
        let root = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        root.join("System32").join(format!("{name}.exe"))
    }

    pub(super) const GIT_INSTALL_HINT: &str = "winget install Git.Git";

    pub(super) const GIT_FAITHFUL_CHECKOUT: &[(&str, &str)] = &[
        ("core.autocrlf", "false"),
        ("core.eol", "lf"),
        ("core.symlinks", "false"),
        ("core.longpaths", "true"),
    ];
}
