//! What a pane and the server ask of the platform they run on, beyond the
//! transport: the shell a pane opens, the environment it starts with, which
//! of its processes is in the foreground, where the server stands, and how
//! a running `uze` is recognized. Every answer is `uze-platform`'s; what is
//! here is what the terminal adds to it.

use std::{env, path::PathBuf};

use portable_pty::CommandBuilder;
use uze_platform::process::pane::{Group, Member};

/// What a pane runs when it is given nothing to run: `UZE_SHELL` when set,
/// else the person's own interactive shell.
pub(super) fn default_shell() -> String {
    env::var("UZE_SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(uze_platform::shell::interactive)
}

/// What a pane's environment needs from the platform before the pane's own
/// launch is applied over it.
pub(super) fn prepare_pane(command: &mut CommandBuilder) {
    for (key, value) in uze_platform::shell::terminal_environment() {
        command.env(key, value);
    }
}

/// The process in the foreground of a pane whose program is `leader` (see
/// [`uze_platform::process::pane::foreground`]). The console passes on through a
/// shell running a command for a person, and through UZE's launcher
/// running a harness.
pub(super) fn foreground(leader: u32, group: Option<&Group>) -> Option<u32> {
    uze_platform::process::pane::foreground(leader, group, passes_on)
}

fn passes_on(holder: &Member, child: &Member) -> bool {
    crate::state::is_plain_shell(&holder.name)
        || holder.name.eq_ignore_ascii_case("uze")
        || super::pane::shim_launched(child.pid)
}

/// What a pane's program is started through where it can only join its
/// group from inside ([`uze_platform::process::pane::grouped`]): the serving
/// binary's `terminal host-pane`, once one was named
/// ([`super::endpoint::host_panes_with`]).
pub(super) fn pane_host() -> Option<Vec<std::ffi::OsString>> {
    let executable = super::endpoint::PANE_HOST.get()?;
    Some(vec![
        executable.into(),
        "terminal".into(),
        "host-pane".into(),
    ])
}

/// Where the server stands. Every pane starts in a directory of its own,
/// so the server needs none, and must not hold the checkout a `uze` was
/// first run from.
pub(super) fn server_directory() -> PathBuf {
    uze_platform::home::unclaimed_directory()
}

/// Whether an image's file name is `uze`: the path is expected to differ
/// (an upgrade moves the binary), so only the name is compared.
pub(super) fn is_uze_image(file_name: &str) -> bool {
    uze_platform::executable::is_image_of(file_name, "uze")
}

/// Whether the process `pid` runs confined while this one does not: a
/// server, or something answering in its place, started from inside a
/// pane. A client in a sandboxed terminal runs confined too, and is not
/// kept from a server it started there.
pub(super) fn runs_more_confined_than_this(pid: u32) -> bool {
    !uze_platform::seal::this_process_runs_confined()
        && uze_platform::seal::runs_confined(pid) == Some(true)
}

/// How a person finds a server this build cannot reach, to end it by hand.
pub(super) fn find_server() -> String {
    uze_platform::process::finding_command("uze", "terminal serve")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, pid: u32) -> Member {
        Member {
            pid,
            parent: None,
            name: name.to_owned(),
        }
    }

    /// No process has this pid, so nothing launched it.
    const NOBODY: u32 = u32::MAX;

    #[test]
    fn the_console_passes_on_through_a_shell_and_through_uze_only() {
        let child = member("claude", NOBODY);
        assert!(passes_on(&member("bash", 1), &child), "a shell");
        assert!(passes_on(&member("uze", 1), &child), "UZE's launcher");
        assert!(
            !passes_on(&member("vim", 1), &child),
            "a program keeps the console for itself"
        );
    }

    #[test]
    fn an_empty_uze_shell_is_the_persons_own_shell() {
        let mut env = uze_testkit::env::scope();
        // Only ever set to empty: a pane another test starts meanwhile
        // reads it too, and empty is what it would get anyway.
        env.set("UZE_SHELL", "");
        assert_eq!(default_shell(), uze_platform::shell::interactive());
        assert!(!default_shell().is_empty());
    }

    /// A server started with no terminal of its own still tells a pane's
    /// programs what terminal they are on.
    #[test]
    fn a_pane_is_told_what_terminal_it_is_on() {
        let mut env = uze_testkit::env::scope();
        env.remove("TERM");
        let terminal = uze_platform::shell::terminal_environment();
        assert!(!terminal.is_empty());
        let mut command = CommandBuilder::new("program");
        prepare_pane(&mut command);
        for (name, value) in terminal {
            assert_eq!(
                command.get_env(name).and_then(|value| value.to_str()),
                Some(value),
                "{name}"
            );
        }
    }

    /// Where a program joins its group only from inside, it is started
    /// through the serving binary's `terminal host-pane` once one is named.
    // Unix only: Unix starts a pane's program as it is whatever host is
    // named, so naming one here changes no pane another test starts; on
    // Windows every pane after it would be started through this name.
    #[cfg(unix)]
    #[test]
    fn a_named_host_starts_a_pane_through_terminal_host_pane() {
        let executable = std::env::current_exe().unwrap();
        crate::runtime::host_panes_with(executable.clone());
        assert_eq!(
            pane_host(),
            Some(vec![
                executable.into_os_string(),
                "terminal".into(),
                "host-pane".into()
            ])
        );
    }
}
