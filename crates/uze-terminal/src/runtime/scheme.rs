//! Colour-scheme change notifications (DEC private mode 2031): which panes asked, and what they are told.

use super::*;
use alacritty_terminal::vte::ansi::{Handler, PrivateMode};
use std::sync::atomic::{AtomicBool, Ordering};

/// The DEC private mode a program sets to be told when the terminal's
/// colours change (`CSI ? 2031 h`). A program following the terminal's
/// scheme sets it, then asks its colours again (OSC 10/11) on each report.
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

/// Watches a pane's output for the mode being set and reset, and keeps the
/// pane's shared flag current. Beside the emulator rather than through it:
/// alacritty drops a private mode it does not know without a word, and its
/// handler is seventy methods to forward for the two this needs, so the
/// output is parsed a second time by a handler that knows only these two.
pub(super) struct SchemeSubscription {
    parser: Processor,
    wanted: Wanted,
    subscribed: Arc<AtomicBool>,
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
    /// Keeps `subscribed` saying whether the program currently asks to be
    /// told; the pane reads it when a palette changes.
    pub(super) fn new(subscribed: Arc<AtomicBool>) -> Self {
        Self {
            parser: Processor::new(),
            wanted: Wanted::default(),
            subscribed,
        }
    }

    /// Reads one chunk of the pane's output.
    pub(super) fn observe(&mut self, output: &[u8]) {
        self.parser.advance(&mut self.wanted, output);
        self.subscribed.store(self.wanted.0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_subscribed_between_setting_and_resetting_the_mode() {
        let subscribed = Arc::new(AtomicBool::new(false));
        let mut subscription = SchemeSubscription::new(Arc::clone(&subscribed));
        subscription.observe(b"hello \x1b[?2004h");
        assert!(
            !subscribed.load(Ordering::Relaxed),
            "another mode is not this one"
        );
        // Split across two reads, as a PTY may hand it over.
        subscription.observe(b"\x1b[?20");
        subscription.observe(b"31h");
        assert!(subscribed.load(Ordering::Relaxed));
        subscription.observe(b"\x1b[?1000;2031l");
        assert!(
            !subscribed.load(Ordering::Relaxed),
            "reset among other modes"
        );
    }
}
