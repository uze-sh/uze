//! Asking the terminal what it draws behind UZE.
//!
//! The question is OSC 11, which a terminal answers with its background
//! colour, followed by DA1, which every terminal answers. Replies come back
//! in the order asked, so the DA1 reply is what ends the wait: a terminal
//! that does not know OSC 11 sends it alone, and nothing sits out a timeout
//! to find that out.

use std::time::Duration;

use uze_application::Background;

const QUESTION: &[u8] = b"\x1b]11;?\x1b\\\x1b[c";

/// Long enough for a reply to cross an SSH connection. Only a terminal that
/// answers nothing at all, DA1 included, makes anyone wait this long.
const PATIENCE: Duration = Duration::from_millis(500);

/// The terminal's background, or `None` when it would not say.
pub(super) fn ask() -> Option<Background> {
    let reply = uze_platform::stdio::ask_terminal(QUESTION, &device_attributes_arrived, PATIENCE)?;
    background_in(&reply)
}

/// `ESC [ ? <parameters> c`, the DA1 reply.
fn device_attributes_arrived(reply: &[u8]) -> bool {
    const LEAD: &[u8] = b"\x1b[?";
    reply
        .windows(LEAD.len())
        .enumerate()
        .any(|(start, window)| {
            if window != LEAD {
                return false;
            }
            let tail = &reply[start + LEAD.len()..];
            let parameters = tail
                .iter()
                .take_while(|byte| byte.is_ascii_digit() || **byte == b';')
                .count();
            tail.get(parameters) == Some(&b'c')
        })
}

/// `ESC ] 11 ; rgb:R/G/B` ended by BEL or ST, wherever it sits in the reply.
fn background_in(reply: &[u8]) -> Option<Background> {
    const LEAD: &[u8] = b"\x1b]11;";
    let start = reply
        .windows(LEAD.len())
        .position(|window| window == LEAD)?
        + LEAD.len();
    let body = &reply[start..];
    let end = body
        .iter()
        .position(|byte| *byte == 0x07 || *byte == 0x1b)?;
    let value = std::str::from_utf8(&body[..end]).ok()?;
    let channels = value
        .strip_prefix("rgb:")
        .or_else(|| value.strip_prefix("rgba:"))?;
    let mut channels = channels.split('/').map(channel);
    let colour = uze_theme::Rgb(channels.next()??, channels.next()??, channels.next()??);
    Some(if colour.is_light() {
        Background::Light
    } else {
        Background::Dark
    })
}

/// One to four hex digits, scaled by the width they were written in:
/// `f`, `ff` and `ffff` are all full intensity.
fn channel(digits: &str) -> Option<u8> {
    if digits.is_empty() || digits.len() > 4 {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    let full = (1u32 << (4 * digits.len())) - 1;
    u8::try_from((value * 255 + full / 2) / full).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pale_background_reads_light_and_a_deep_one_dark() {
        assert_eq!(
            background_in(b"\x1b]11;rgb:fafa/fafa/f8f8\x1b\\"),
            Some(Background::Light)
        );
        assert_eq!(
            background_in(b"\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\"),
            Some(Background::Dark)
        );
    }

    #[test]
    fn either_terminator_ends_the_reply() {
        assert_eq!(
            background_in(b"\x1b]11;rgb:ffff/ffff/ffff\x07"),
            Some(Background::Light)
        );
        assert_eq!(
            background_in(b"\x1b]11;rgb:0000/0000/0000\x1b\\"),
            Some(Background::Dark)
        );
    }

    #[test]
    fn every_channel_width_a_terminal_writes_is_scaled_to_the_same_colour() {
        for reply in [
            &b"\x1b]11;rgb:f/f/f\x07"[..],
            b"\x1b]11;rgb:ff/ff/ff\x07",
            b"\x1b]11;rgb:fff/fff/fff\x07",
            b"\x1b]11;rgb:ffff/ffff/ffff\x07",
            b"\x1b]11;rgba:ffff/ffff/ffff/ffff\x07",
        ] {
            assert_eq!(background_in(reply), Some(Background::Light), "{reply:?}");
        }
        assert_eq!(channel("8"), Some(136));
        assert_eq!(channel("80"), Some(128));
        assert_eq!(channel("8080"), Some(128));
    }

    #[test]
    fn the_colour_is_found_ahead_of_the_device_attributes() {
        let reply = b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[?62;22c";
        assert!(device_attributes_arrived(reply));
        assert_eq!(background_in(reply), Some(Background::Light));
    }

    #[test]
    fn a_terminal_that_only_answers_device_attributes_says_nothing() {
        let reply = b"\x1b[?1;2c";
        assert!(device_attributes_arrived(reply));
        assert_eq!(background_in(reply), None);
    }

    #[test]
    fn a_reply_cut_short_is_not_yet_answered() {
        assert!(!device_attributes_arrived(b"\x1b]11;rgb:ffff/ff"));
        assert!(!device_attributes_arrived(
            b"\x1b]11;rgb:ffff/ffff/ffff\x07\x1b[?62"
        ));
        assert_eq!(background_in(b"\x1b]11;rgb:ffff/ff"), None);
    }

    #[test]
    fn a_malformed_colour_is_no_answer() {
        for reply in [
            &b"\x1b]11;cmyk:0/0/0/0\x07"[..],
            b"\x1b]11;rgb:ffff/ffff\x07",
            b"\x1b]11;rgb:fffff/ffff/ffff\x07",
            b"\x1b]11;rgb:zz/zz/zz\x07",
            b"\x1b]11;rgb://\x07",
        ] {
            assert_eq!(background_in(reply), None, "{reply:?}");
        }
    }
}
