//! The one way a string an extension supplied becomes something UZE draws.
//!
//! Everything a manifest, a view or an extension's data says reaches a
//! terminal, and a terminal acts on some of what it is sent: an escape
//! sequence can move the cursor, set the title, write the clipboard (OSC 52)
//! or make a link say one thing and go to another (OSC 8). A bidirectional
//! override can make text read backwards; zero-width and tag characters can
//! carry words nobody sees. So [`Clean`] is built only here, and the engine
//! builds what it hands the host only from [`Clean`] strings (D7, S14).
//!
//! A length is counted in the cells a terminal gives each character, not in
//! characters or bytes: a bound in characters lets a wide script take twice
//! the room a label was given.

use serde::{Deserialize, Deserializer, Serialize};
use unicode_width::UnicodeWidthChar;

/// The widest a one-line label may be, in terminal cells.
pub const LABEL_MAX: usize = 256;
/// The widest a block of text (a log line, a message) may be, in terminal
/// cells, a line break counting as one.
pub const TEXT_MAX: usize = 4096;
/// How many combining marks may stack on one base character before the rest
/// are dropped: enough for every script that needs them, too few to smear a
/// row.
const COMBINING_MAX: usize = 2;

/// A string stripped of everything a terminal would act on, and bounded.
///
/// There is no way to make one but [`Clean::label`], [`Clean::text`] and
/// [`Deserialize`], which cleans what it reads as [`Clean::text`]: a
/// `Clean` read from an answer or a manifest is as clean as one made here.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize)]
pub struct Clean(String);

impl<'de> Deserialize<'de> for Clean {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = std::borrow::Cow::<'de, str>::deserialize(deserializer)?;
        Ok(Self::text(&raw))
    }
}

impl Clean {
    /// A one-line label: line breaks become spaces.
    pub fn label(source: &str) -> Self {
        Self(clean(source, false, LABEL_MAX))
    }

    /// Text that may span lines: `\n` is kept, every other break becomes one.
    pub fn text(source: &str) -> Self {
        Self(clean(source, true, TEXT_MAX))
    }

    /// The same text with its line breaks read as spaces: a paragraph the
    /// host wraps to the room it has, still bounded as text.
    pub fn one_line(self) -> Self {
        Self(self.0.replace('\n', " "))
    }

    /// Its lines, each as clean as the whole.
    pub fn lines(&self) -> impl Iterator<Item = Clean> + '_ {
        self.0.split('\n').map(|line| Self(line.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for Clean {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Clean {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn clean(source: &str, multiline: bool, max: usize) -> String {
    let mut out = String::with_capacity(source.len().min(max * 4));
    let mut cells = 0;
    let mut combining = 0;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        let width = match ch {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' | '\u{85}' | '\t' => 1,
            ch => ch.width().unwrap_or(0),
        };
        if cells + width > max {
            break;
        }
        match ch {
            '\u{1b}' => {
                skip_escape(&mut chars);
                continue;
            }
            '\u{9b}' => {
                skip_csi(&mut chars);
                continue;
            }
            '\u{9d}' | '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => {
                skip_string(&mut chars);
                continue;
            }
            '\n' | '\r' | '\u{2028}' | '\u{2029}' | '\u{85}' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    continue;
                }
                out.push(if multiline { '\n' } else { ' ' });
            }
            '\t' => out.push(' '),
            ch if ch.is_control() || is_invisible(ch) => continue,
            ch if is_combining(ch) => {
                combining += 1;
                if combining > COMBINING_MAX {
                    continue;
                }
                out.push(ch);
                continue;
            }
            ch => out.push(ch),
        }
        combining = 0;
        cells += width;
    }
    out
}

/// After ESC: a CSI (`[`), a string (`]`, `P`, `X`, `^`, `_`) or a single
/// character.
fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.next() {
        Some('[') => skip_csi(chars),
        Some(']' | 'P' | 'X' | '^' | '_') => skip_string(chars),
        _ => {}
    }
}

/// Parameters and intermediates, up to the final byte (`@` to `~`).
fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for ch in chars.by_ref() {
        if ('\u{40}'..='\u{7e}').contains(&ch) {
            break;
        }
    }
}

/// An OSC, DCS or similar string, up to BEL, ST (`ESC \`) or the C1 ST.
fn skip_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(ch) = chars.next() {
        match ch {
            '\u{7}' | '\u{9c}' => break,
            '\u{1b}' => {
                if chars.peek() == Some(&'\\') {
                    chars.next();
                }
                break;
            }
            _ => {}
        }
    }
}

/// Format characters that change how text reads without being seen:
/// zero-width spaces and joiners outside an emoji, bidirectional marks,
/// overrides and isolates, the byte-order mark, interlinear annotation and
/// Unicode tags.
fn is_invisible(ch: char) -> bool {
    matches!(ch,
        '\u{ad}'
        | '\u{61c}'
        | '\u{180e}'
        | '\u{200b}'..='\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{206f}'
        | '\u{feff}'
        | '\u{fff9}'..='\u{fffb}'
        | '\u{e0000}'..='\u{e007f}'
    )
}

fn is_combining(ch: char) -> bool {
    matches!(ch,
        '\u{300}'..='\u{36f}'
        | '\u{1ab0}'..='\u{1aff}'
        | '\u{1dc0}'..='\u{1dff}'
        | '\u{20d0}'..='\u{20ff}'
        | '\u{fe20}'..='\u{fe2f}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_osc_52_clipboard_write_is_drawn_inert() {
        assert_eq!(
            Clean::label("\u{1b}]52;c;ZXZpbA==\u{7}build").as_str(),
            "build"
        );
        assert_eq!(
            Clean::label("\u{1b}]52;c;ZXZpbA==\u{1b}\\build").as_str(),
            "build"
        );
    }

    #[test]
    fn a_link_and_cursor_moves_are_removed() {
        assert_eq!(
            Clean::label("\u{1b}]8;;https://evil\u{7}docs\u{1b}]8;;\u{7}").as_str(),
            "docs"
        );
        assert_eq!(Clean::label("a\u{1b}[2J\u{1b}[1;1Hb").as_str(), "ab");
        assert_eq!(Clean::label("a\u{9b}31mb").as_str(), "ab");
    }

    #[test]
    fn bidi_overrides_zero_widths_and_tags_are_removed() {
        assert_eq!(Clean::label("\u{202e}evil").as_str(), "evil");
        assert_eq!(Clean::label("a\u{200b}b\u{2066}c").as_str(), "abc");
        assert_eq!(Clean::label("ok\u{e0041}\u{e0042}").as_str(), "ok");
    }

    #[test]
    fn breaks_become_spaces_in_a_label_and_stay_in_text() {
        assert_eq!(Clean::label("a\nb\r\nc\u{2028}d").as_str(), "a b c d");
        assert_eq!(Clean::text("a\r\nb\u{2029}c").as_str(), "a\nb\nc");
        assert_eq!(Clean::label("a\tb").as_str(), "a b");
    }

    #[test]
    fn stacked_combining_marks_are_capped_and_length_is_bounded() {
        let zalgo = format!("a{}", "\u{301}".repeat(20));
        assert_eq!(
            Clean::label(&zalgo).as_str().chars().count(),
            1 + COMBINING_MAX
        );
        assert_eq!(
            Clean::label(&"x".repeat(LABEL_MAX * 2)).as_str().len(),
            LABEL_MAX
        );
    }

    #[test]
    fn length_is_bounded_in_the_cells_a_terminal_gives_it() {
        let wide = Clean::label(&"日".repeat(LABEL_MAX));
        assert_eq!(wide.as_str().chars().count(), LABEL_MAX / 2);
        let mixed = Clean::label(&format!("{}日", "x".repeat(LABEL_MAX - 1)));
        assert_eq!(
            mixed.as_str(),
            "x".repeat(LABEL_MAX - 1),
            "a wide character that would overflow by a cell is left out whole"
        );
        let text = Clean::text(&"ab\n".repeat(TEXT_MAX));
        assert_eq!(text.as_str().chars().count(), TEXT_MAX);
    }

    #[test]
    fn what_is_deserialized_is_cleaned() {
        let read: Clean =
            serde_json::from_str(r#""\u001b]52;c;ZXZpbA==\u0007a\u202eb\r\nc""#).unwrap();
        assert_eq!(read.as_str(), "ab\nc");
        let long: Clean =
            serde_json::from_value(serde_json::json!("x".repeat(TEXT_MAX + 9))).unwrap();
        assert_eq!(long.as_str().len(), TEXT_MAX);
        assert!(
            serde_json::from_str::<Clean>("3").is_err(),
            "only a string is text"
        );
        assert_eq!(serde_json::to_value(&read).unwrap(), "ab\nc");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        for text in [
            "Fase 12 · Consulta",
            "▶ executar fase 12",
            "café",
            "日本語",
            "👍🏽",
        ] {
            assert_eq!(Clean::label(text).as_str(), text);
        }
    }
}
