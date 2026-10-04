//! The tools the operating system itself ships, found where it keeps them.

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
}
