//! What a pane and the server ask of the platform they run on, beyond the
//! transport: the shell a pane opens, the environment it starts with, which
//! of its processes is in the foreground, where the server stands, and how
//! a running `uze` is recognized.

use std::path::PathBuf;

use portable_pty::{CommandBuilder, MasterPty};
use uze_platform::process::Group;

/// What a pane runs when it is given nothing to run: the person's shell.
pub(super) fn default_shell() -> String {
    imp::default_shell()
}

/// What a pane's environment needs from the platform before the pane's own
/// launch is applied over it.
pub(super) fn prepare_pane(command: &mut CommandBuilder) {
    imp::prepare_pane(command)
}

/// The process in the foreground of a pane: the terminal's foreground
/// group on Unix; on Windows, where ConPTY keeps no such thing, the newest
/// process in the pane's group — what a person typed last, the shell when
/// they typed nothing.
pub(super) fn foreground(master: &dyn MasterPty, group: Option<&Group>) -> Option<u32> {
    imp::foreground(master, group)
}

/// Where the server stands. Every pane starts in a directory of its own,
/// so the server needs none, and must not hold the checkout a `uze` was
/// first run from.
pub(super) fn server_directory() -> PathBuf {
    imp::server_directory()
}

/// Whether an image's file name is `uze`: the path is expected to differ
/// (an upgrade moves the binary), so only the name is compared, in the
/// forms a replaced binary takes on this platform.
pub(super) fn is_uze_image(file_name: &str) -> bool {
    imp::is_uze_image(file_name)
}

/// How a person finds a server this build cannot reach, to end it by hand.
pub(super) const FIND_SERVER: &str = imp::FIND_SERVER;

#[cfg(unix)]
mod imp {
    use std::{env, path::PathBuf};

    use portable_pty::{CommandBuilder, MasterPty};
    use uze_platform::process::Group;

    pub(super) const FIND_SERVER: &str = "`pgrep -fa 'uze terminal serve'`";

    pub(super) fn default_shell() -> String {
        env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }

    pub(super) fn prepare_pane(command: &mut CommandBuilder) {
        if env::var_os("TERM").is_none() {
            command.env("TERM", "xterm-256color");
        }
    }

    pub(super) fn foreground(master: &dyn MasterPty, _group: Option<&Group>) -> Option<u32> {
        u32::try_from(master.process_group_leader()?).ok()
    }

    pub(super) fn server_directory() -> PathBuf {
        PathBuf::from("/")
    }

    /// Linux marks a binary replaced under a live process `(deleted)`.
    pub(super) fn is_uze_image(file_name: &str) -> bool {
        file_name.strip_suffix(" (deleted)").unwrap_or(file_name) == "uze"
    }
}

#[cfg(windows)]
mod imp {
    use std::{env, path::PathBuf};

    use portable_pty::{CommandBuilder, MasterPty};
    use uze_platform::process::Group;

    pub(super) const FIND_SERVER: &str = "`Get-Process uze`";

    /// `UZE_SHELL` when set, else PowerShell 7 when it is installed, else
    /// the Windows PowerShell every supported Windows carries. A person's
    /// interactive shell is a preference, so the newer one is preferred
    /// here; a project's commands always run in the one that is always
    /// there.
    pub(super) fn default_shell() -> String {
        if let Some(shell) = env::var_os("UZE_SHELL") {
            return shell.to_string_lossy().into_owned();
        }
        let installed = env::var_os("PATH").is_some_and(|path| {
            env::split_paths(&path).any(|directory| directory.join("pwsh.exe").is_file())
        });
        if installed {
            "pwsh.exe"
        } else {
            "powershell.exe"
        }
        .to_owned()
    }

    /// Windows programs do not read `TERM`, and a few change behaviour when
    /// it names a Unix terminal; what they do read is `COLORTERM`.
    pub(super) fn prepare_pane(command: &mut CommandBuilder) {
        command.env("COLORTERM", "truecolor");
    }

    pub(super) fn foreground(_master: &dyn MasterPty, group: Option<&Group>) -> Option<u32> {
        group?.members().into_iter().rev().find(|pid| {
            uze_platform::probe::command_name_of(*pid)
                .is_none_or(|name| !name.eq_ignore_ascii_case("conhost"))
        })
    }

    pub(super) fn server_directory() -> PathBuf {
        env::home_dir().unwrap_or_else(env::temp_dir)
    }

    /// `uze.exe` in any case; an upgrade leaves the running image renamed
    /// aside as `uze.exe.old-<pid>`.
    pub(super) fn is_uze_image(file_name: &str) -> bool {
        let name = file_name.to_ascii_lowercase();
        name == "uze.exe" || name.starts_with("uze.exe.old-")
    }
}
