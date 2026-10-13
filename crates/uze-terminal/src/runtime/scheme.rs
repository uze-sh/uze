//! Colour-scheme change notifications (DEC private mode 2031): which panes asked, and what they are told.

use super::*;
use alacritty_terminal::vte::ansi::{Handler, PrivateMode};
use std::sync::atomic::{AtomicBool, Ordering};

/// The DEC private mode a program sets to be told when the terminal's
/// colours change (`CSI ? 2031 h`). Claude Code and OpenCode set it, then
/// ask their colours again (OSC 10/11) on each notification.
const COLOR_SCHEME_NOTIFICATIONS: u16 = 2031;

impl Scheme {
    /// The report the terminal sends to a program that asked
    /// (`CSI ? 997 ; 1 n` for dark, `; 2 n` for light).
    pub(super) fn notification(self) -> &'static [u8] {
        match self {
            Self::Dark => b"\x1b[?997;1n",
            Self::Light => b"\x1b[?997;2n",
        }
    }
}

/// Watches a pane's output for the mode being set and reset, beside the
/// emulator rather than through it: alacritty drops a private mode it does
/// not know without a word, and its handler is seventy methods to forward
/// for the two this needs. Parsing the output twice costs less than the
/// emulator's own pass over it.
pub(super) struct SchemeSubscription {
    parser: Processor,
    wanted: Wanted,
}

#[derive(Default)]
struct Wanted(bool);

impl Handler for Wanted {
    fn set_private_mode(&mut self, mode: PrivateMode) {
        if mode == PrivateMode::Unknown(COLOR_SCHEME_NOTIFICATIONS) {
            self.0 = true;
        }
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        if mode == PrivateMode::Unknown(COLOR_SCHEME_NOTIFICATIONS) {
            self.0 = false;
        }
    }
}

impl SchemeSubscription {
    pub(super) fn new() -> Self {
        Self {
            parser: Processor::new(),
            wanted: Wanted::default(),
        }
    }

    /// Reads one chunk of the pane's output and records into `subscribed`
    /// whether the program currently asks to be told.
    pub(super) fn observe(&mut self, output: &[u8], subscribed: &AtomicBool) {
        self.parser.advance(&mut self.wanted, output);
        subscribed.store(self.wanted.0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_subscribed_between_setting_and_resetting_the_mode() {
        let subscribed = AtomicBool::new(false);
        let mut subscription = SchemeSubscription::new();
        subscription.observe(b"hello \x1b[?2004h", &subscribed);
        assert!(
            !subscribed.load(Ordering::Relaxed),
            "another mode is not this one"
        );
        // Split across two reads, as a PTY may hand it over.
        subscription.observe(b"\x1b[?20", &subscribed);
        subscription.observe(b"31h", &subscribed);
        assert!(subscribed.load(Ordering::Relaxed));
        subscription.observe(b"\x1b[?1000;2031l", &subscribed);
        assert!(
            !subscribed.load(Ordering::Relaxed),
            "reset among other modes"
        );
    }
}
