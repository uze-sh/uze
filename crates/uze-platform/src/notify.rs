//! Telling the person something outside the terminal's cells: a desktop
//! notification where the desktop has a tool for it, a terminal's own
//! notification sequence where the terminal draws one, and its bell
//! everywhere else.
//!
//! Deciding is cheap and spawns nothing; sending through the desktop
//! starts a program, so the caller does it off any thread that draws. A
//! terminal sequence is bytes the caller writes between frames, where the
//! bell already goes.

use std::process::Command;

/// How one notification reaches the person.
#[derive(Debug)]
pub enum Route {
    /// The desktop's own tool, ready to spawn.
    Desktop(Command),
    /// Bytes for the terminal, written between frames.
    Terminal(Vec<u8>),
}

/// The bell, as bytes for the terminal.
pub const BELL: &[u8] = b"\x07";

/// The terminals known to draw OSC 9 as a notification, by the
/// `TERM_PROGRAM` each sets.
const OSC9_TERMINALS: &[&str] = &["WezTerm", "iTerm.app", "ghostty"];

/// How a notification titled `title` saying `body` reaches the person
/// here. `term_program` is the terminal's `TERM_PROGRAM`, which the caller
/// reads once.
pub fn route(title: &str, body: &str, term_program: Option<&str>) -> Route {
    let (title, body) = (plain(title), plain(body));
    if let Some(command) = imp::desktop(&title, &body) {
        return Route::Desktop(command);
    }
    if term_program.is_some_and(|program| OSC9_TERMINALS.contains(&program)) {
        return Route::Terminal(format!("\x1b]9;{title}: {body}\x07").into_bytes());
    }
    Route::Terminal(BELL.to_vec())
}

/// `text` without anything a terminal or a shell would read as more than
/// text: control characters, and at most 200 characters of it.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_control())
        .take(200)
        .collect()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::process::Command;

    /// `osascript` ships with every macOS and needs no permission prompt
    /// for `display notification`. The text is passed as arguments, never
    /// spliced into the script.
    pub(super) fn desktop(title: &str, body: &str) -> Option<Command> {
        let mut command = crate::tools::system("osascript");
        command.args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            title,
            body,
        ]);
        Some(command)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::process::Command;

    /// `notify-send` where a desktop session is there to show it: a
    /// display to draw on and the tool in the system's own directories.
    /// WSL's Linux has neither by default; its notifications go to the
    /// terminal.
    pub(super) fn desktop(title: &str, body: &str) -> Option<Command> {
        let session =
            std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();
        let tool = crate::tools::system_program("notify-send");
        if !session || !tool.is_file() {
            return None;
        }
        let mut command = Command::new(tool);
        command.args(["--app-name=uze", "--", title, body]);
        Some(command)
    }
}

#[cfg(windows)]
mod imp {
    use std::process::Command;

    /// Windows' toasts need an application identity registered with the
    /// shell; until UZE has one, a notification goes to the terminal.
    pub(super) fn desktop(_title: &str, _body: &str) -> Option<Command> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal(route: Route) -> Vec<u8> {
        match route {
            Route::Terminal(bytes) => bytes,
            Route::Desktop(command) => panic!("a desktop tool was chosen: {command:?}"),
        }
    }

    #[test]
    fn a_terminal_that_draws_notifications_gets_osc_9_and_any_other_the_bell() {
        // A desktop session would be chosen first; without one, the
        // terminal decides.
        if imp::desktop("t", "b").is_some() {
            return;
        }
        assert_eq!(
            terminal(route(
                "gpu-meter · gpu@proving",
                "load 92%",
                Some("WezTerm")
            )),
            b"\x1b]9;gpu-meter \xc2\xb7 gpu@proving: load 92%\x07"
        );
        assert_eq!(terminal(route("a", "b", Some("Apple_Terminal"))), BELL);
        assert_eq!(terminal(route("a", "b", None)), BELL);
    }

    #[test]
    fn nothing_but_text_reaches_the_terminal() {
        assert_eq!(plain("done\x1b]0;owned\x07\nnow"), "done]0;ownednow");
        assert_eq!(plain(&"x".repeat(500)).len(), 200);
    }
}
