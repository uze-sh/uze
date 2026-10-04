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

/// Where the system keeps its own tools: `/usr/bin` and `/bin`; on Windows
/// `System32` and Windows PowerShell's directory. A `PATH` of only these
/// finds nothing a person installed.
pub fn system_directories() -> Vec<std::path::PathBuf> {
    imp::system_directories()
}

/// How a person installs Git here, as a command they can type.
pub const GIT_INSTALL_HINT: &str = imp::GIT_INSTALL_HINT;

/// The Git settings under which a checkout holds a repository's files as
/// they were committed, on this platform: no line-ending conversion
/// anywhere; on Windows also no links Git cannot make without a privilege
/// (each becomes a file holding its target), paths longer than the Win32
/// limit, and no executable bit read from a file system that keeps none. Pushed rather than left to a Git build's defaults, which
/// differ between builds.
pub const GIT_FAITHFUL_CHECKOUT: &[(&str, &str)] = imp::GIT_FAITHFUL_CHECKOUT;

/// What a checkout UZE adds to a person's repository needs from Git on this
/// platform, and nothing about how the person's own files are held: on
/// Windows, paths past the Win32 limit, which `.worktrees/<id>/` brings
/// closer to every file of the repository.
pub const GIT_ADDED_CHECKOUT: &[(&str, &str)] = imp::GIT_ADDED_CHECKOUT;

#[cfg(unix)]
mod imp {
    use std::path::PathBuf;

    /// `curl` and `tar` are the system's own wherever they are found.
    pub(super) fn system_program(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    pub(super) fn system_directories() -> Vec<PathBuf> {
        vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")]
    }

    pub(super) const GIT_INSTALL_HINT: &str = "install it with your package manager";

    pub(super) const GIT_FAITHFUL_CHECKOUT: &[(&str, &str)] =
        &[("core.autocrlf", "false"), ("core.eol", "lf")];

    pub(super) const GIT_ADDED_CHECKOUT: &[(&str, &str)] = &[];
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    /// System32, by path: under Windows PowerShell `curl` is an alias for
    /// something else, and a `tar` earlier on `PATH` (Git's GNU tar) cannot
    /// read a zip.
    pub(super) fn system_program(name: &str) -> PathBuf {
        system32().join(format!("{name}.exe"))
    }

    pub(super) fn system_directories() -> Vec<PathBuf> {
        let system = system32();
        vec![system.join("WindowsPowerShell").join("v1.0"), system]
    }

    fn system32() -> PathBuf {
        std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
            .join("System32")
    }

    pub(super) const GIT_INSTALL_HINT: &str = "winget install Git.Git";

    pub(super) const GIT_FAITHFUL_CHECKOUT: &[(&str, &str)] = &[
        ("core.autocrlf", "false"),
        ("core.eol", "lf"),
        ("core.symlinks", "false"),
        ("core.longpaths", "true"),
        // The file system keeps no executable bit for Git to compare.
        ("core.fileMode", "false"),
    ];

    pub(super) const GIT_ADDED_CHECKOUT: &[(&str, &str)] = &[("core.longpaths", "true")];
}
