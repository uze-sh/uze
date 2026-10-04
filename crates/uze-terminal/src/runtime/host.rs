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
/// group on Unix; on Windows, where ConPTY keeps no such thing, the process
/// the pane's shell or UZE's launcher handed the console to, the shell when
/// it handed it to nobody.
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
        let members: Vec<Member> = group?
            .members()
            .into_iter()
            .filter_map(|pid| {
                let name = uze_platform::probe::command_name_of(pid)?;
                let parent = uze_platform::probe::parent_of(pid);
                (!name.eq_ignore_ascii_case("conhost")).then_some(Member { pid, parent, name })
            })
            .collect();
        in_front(&members, |holder, child| {
            // A shell runs a command for a person by starting it and
            // waiting, handing it the console.
            super::super::process::PLAIN_SHELL_PROCESS_NAMES
                .iter()
                .any(|shell| holder.name.eq_ignore_ascii_case(shell))
                || super::super::pane::shim_launched_name(child.pid).is_some()
        })
    }

    /// One live process of a pane's group.
    #[derive(Debug)]
    struct Member {
        pid: u32,
        parent: Option<u32>,
        name: String,
    }

    /// The process holding the pane's console. Unix answers this with the
    /// foreground group, which a shell gives each command it runs and which
    /// the command's own children share; so an agent stays in front while
    /// it runs `git` or a language server. Here it is found the way the
    /// console was passed: from the process the pane started, down through
    /// each one that `hands_over` (a shell running a command, the launcher
    /// running a harness) to the first that keeps it. The newest process in
    /// the group, which this used to answer, is whatever the agent started
    /// last, and the pane came and went from the sidebar with every one.
    fn in_front(members: &[Member], hands_over: impl Fn(&Member, &Member) -> bool) -> Option<u32> {
        let is_member = |pid: u32| members.iter().any(|member| member.pid == pid);
        let mut holder = members
            .iter()
            .find(|member| member.parent.is_none_or(|parent| !is_member(parent)))?;
        while let Some(child) = members
            .iter()
            .rev()
            .find(|child| child.parent == Some(holder.pid) && hands_over(holder, child))
        {
            holder = child;
        }
        Some(holder.pid)
    }

    /// Windows has no directory every user may stand in that is nobody's
    /// checkout the way `/` is; the person's home is theirs alone.
    pub(super) fn server_directory() -> PathBuf {
        uze_platform::home::user_home().unwrap_or_else(env::temp_dir)
    }

    /// `uze.exe` in any case; an upgrade leaves the running image renamed
    /// aside as `uze.exe.old-<pid>`.
    pub(super) fn is_uze_image(file_name: &str) -> bool {
        let name = file_name.to_ascii_lowercase();
        name == "uze.exe" || name.starts_with("uze.exe.old-")
    }

    #[cfg(test)]
    mod tests {
        use super::{Member, in_front};

        fn member(pid: u32, parent: u32, name: &str) -> Member {
            Member {
                pid,
                parent: Some(parent),
                name: name.to_owned(),
            }
        }

        fn shells_and_launchers(holder: &Member, child: &Member) -> bool {
            holder.name == "pwsh" || child.name == "opencode" && holder.name == "shim"
        }

        /// What an agent starts while it works is its own, and the agent
        /// stays in front of the pane through every one of them.
        #[test]
        fn an_agent_stays_in_front_while_its_children_come_and_go() {
            let pane = [
                member(10, 1, "pwsh"),
                member(11, 10, "opencode"),
                member(12, 11, "git"),
                member(13, 11, "rg"),
            ];
            assert_eq!(in_front(&pane, shells_and_launchers), Some(11));
            assert_eq!(in_front(&pane[..2], shells_and_launchers), Some(11));
        }

        /// A harness run through UZE's launcher is in front, not the
        /// launcher waiting on it; a shell with nothing running is.
        #[test]
        fn the_console_is_followed_through_shells_and_the_launcher_only() {
            let launched = [
                member(10, 1, "pwsh"),
                member(11, 10, "shim"),
                member(12, 11, "opencode"),
                member(13, 12, "node"),
            ];
            assert_eq!(in_front(&launched, shells_and_launchers), Some(12));
            assert_eq!(in_front(&launched[..1], shells_and_launchers), Some(10));
            assert_eq!(in_front(&[], shells_and_launchers), None);
        }
    }
}
