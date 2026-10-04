//! What a pane and the server ask of the platform they run on, beyond the
//! transport: the shell a pane opens, the environment it starts with, which
//! of its processes is in the foreground, where the server stands, and how
//! a running `uze` is recognized. Every answer is `uze-platform`'s; what is
//! here is what the terminal adds to it.

use std::{env, path::PathBuf};

use portable_pty::CommandBuilder;
use uze_platform::process::{Group, Member};

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
/// [`uze_platform::process::foreground`]). The console passes on through a
/// shell running a command for a person, and through UZE's launcher
/// running a harness.
pub(super) fn foreground(leader: u32, group: Option<&Group>) -> Option<u32> {
    uze_platform::process::foreground(leader, group, passes_on)
}

fn passes_on(holder: &Member, child: &Member) -> bool {
    super::process::PLAIN_SHELL_PROCESS_NAMES
        .iter()
        .any(|shell| holder.name.eq_ignore_ascii_case(shell))
        || super::pane::shim_launched_name(child.pid).is_some()
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

/// How a person finds a server this build cannot reach, to end it by hand.
pub(super) fn find_server() -> String {
    uze_platform::process::finding_command("uze", "terminal serve")
}
