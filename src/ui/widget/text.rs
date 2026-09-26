//! Fitting text to the room there is.
//!
//! A terminal has no reflow and no tooltip: text that does not fit is text
//! the reader loses, so every column in the product cuts, and cutting is
//! one decision — where, and with what mark — not a dozen.
//!
//! The two halves were split by accident rather than by design. [`elide`]
//! sat in `ui.rs` and nine files reached for it; [`clip`] sat in
//! `management.rs`, a *screen*, and four other screens reached across for
//! it as `super::super::management::clip_line`. A primitive a screen
//! exports is a primitive in the wrong place: nothing about cutting a line
//! belongs to the screen that happened to need it first.

use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

use crate::ui::theme::{self, Symbol};

/// The cells `text` takes on a terminal, which is what every cut here is
/// measured in: a CJK character or an emoji takes two, and counting it as
/// one is how a column overflows into the next.
pub(crate) fn columns(text: &str) -> usize {
    text.chars().map(cell_width).sum()
}

/// A control character has no width of its own; it is counted as the one
/// cell it was always counted as, so nothing already fitted moves.
fn cell_width(character: char) -> usize {
    character.width().unwrap_or(1)
}

/// The longest start of `text` that fits in `room` cells.
fn head_within(text: &str, room: usize) -> &str {
    let mut used = 0;
    for (index, character) in text.char_indices() {
        used += cell_width(character);
        if used > room {
            return &text[..index];
        }
    }
    text
}

/// The longest end of `text` that fits in `room` cells.
fn tail_within(text: &str, room: usize) -> &str {
    let mut used = 0;
    for (index, character) in text.char_indices().rev() {
        used += cell_width(character);
        if used > room {
            return &text[index + character.len_utf8()..];
        }
    }
    text
}

/// `text`, cut to `width` columns with the ellipsis mark when it does not
/// fit.
///
/// The mark's own width comes from the theme: a glyph set that spells the
/// ellipsis with three periods takes three columns where `…` takes one,
/// and a cut measured against the wrong one overflows the column it was
/// meant to fit.
pub(crate) fn elide(text: &str, width: usize) -> String {
    if columns(text) <= width {
        return text.to_owned();
    }
    let Some(room) = width.checked_sub(theme::width(Symbol::Ellipsis) as usize) else {
        return String::new();
    };
    let mut kept = head_within(text, room).to_owned();
    kept.push_str(&theme::glyph(Symbol::Ellipsis));
    kept
}

/// `text` shortened from the left to `width` columns, keeping its tail —
/// the end of a path is what says where you are; its beginning is what
/// you can afford to lose.
pub(crate) fn elide_head(text: &str, width: usize) -> String {
    if columns(text) <= width {
        return text.to_owned();
    }
    let room = width.saturating_sub(theme::width(Symbol::Ellipsis) as usize);
    let mut kept = theme::glyph(Symbol::Ellipsis);
    kept.push_str(tail_within(text, room));
    kept
}

/// Cuts `line` to `max` columns in place, keeping every span that fits
/// whole and eliding the one that straddles the edge.
///
/// Spans rather than characters because a line is styled: cutting the
/// string would lose which part of it was the key and which the
/// description, and the last visible span has to keep its own style right
/// up to the mark.
pub(crate) fn clip(line: &mut Line<'static>, max: usize) {
    let mut used = 0usize;
    let mut cut = None;
    for (index, span) in line.spans.iter().enumerate() {
        let width = span.width();
        if used + width <= max {
            used += width;
        } else {
            cut = Some(index);
            break;
        }
    }
    let Some(index) = cut else {
        return;
    };
    line.spans[index].content =
        std::borrow::Cow::Owned(elide(&line.spans[index].content, max - used));
    line.spans.truncate(index + 1);
}

/// `text` broken between words into rows of at most `width` columns, with
/// a word wider than a row broken across rows — the one wrapper every
/// surface folds prose with. Empty text is one empty row.
///
/// Folding *before* a paragraph is authored is what keeps a row index a
/// screen row: the plugin drawer anchors its two clickable rows (the
/// marketplace name, the address under it) by counting authored lines, and
/// a description that ratatui's own `Wrap` folded afterwards pushed the
/// drawn rows down and left the targets sitting above them.
pub(crate) fn fold(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_owned()];
    }
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in text.split_whitespace() {
        // A word wider than the row is broken across rows rather than
        // left to overflow — the paragraph's own wrapper does the same,
        // and a bare URL in a description is exactly that word.
        let mut word = word;
        while columns(word) > width {
            if !row.is_empty() {
                rows.push(std::mem::take(&mut row));
            }
            // At least one character a row, or a character wider than the
            // whole row would never leave the word.
            let head = match head_within(word, width) {
                "" => word
                    .chars()
                    .next()
                    .map_or(word, |first| &word[..first.len_utf8()]),
                head => head,
            };
            rows.push(head.to_owned());
            word = &word[head.len()..];
        }
        let projected = columns(&row) + usize::from(!row.is_empty()) + columns(word);
        if projected > width && !row.is_empty() {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(word);
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// `n` in subscript digits (`12` -> `₁₂`): a count that sits beside a
/// label without competing with it for weight — the route counts in the
/// management sidebar, the pull/push counts under an agent's branch.
pub(crate) fn small_digits(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            _ => c,
        })
        .collect()
}

/// Renders a label as Unicode small capitals (`Beta` -> `ʙᴇᴛᴀ`) — the
/// weight of capitals without their full height, for a mark that has to be
/// noticed beside a name without shouting over it. The input is lowercased
/// first so a mixed-case label reads as one even run rather than as a
/// full-height initial followed by small ones. Unicode has no small
/// capital for `q` or `x`, so those stay lowercase — closer in weight to
/// their neighbours than the one full-height letter in the run would be —
/// and anything outside the ASCII alphabet passes through rather than
/// being dropped.
///
/// Every letter maps to exactly one character, and all 24 are East Asian
/// width *neutral*, so a label keeps both its length and its cell count:
/// the padded columns it sits in are unaffected.
///
/// What this depends on is the reader's font, which is why it is spent
/// sparingly — the workspace tab's agent alias and the sidebar's badge for
/// an unsettled route, both short and both read once. The glyphs are
/// scattered across three blocks (IPA Extensions, Phonetic Extensions,
/// and `ꜰ`/`ꜱ` alone in Latin Extended-D) and monospace coverage is thin:
/// measured against the patched Nerd Fonts, Fira Code, JetBrains Mono and
/// Hack carry none of the 24, DejaVu Sans Mono and Meslo 8, Consolas and
/// Liberation Mono 22 — missing exactly `ꜰ` and `ꜱ` — and only Iosevka and
/// Noto Sans Mono all 24. A terminal missing a glyph substitutes a
/// proportional fallback face, which still occupies its one cell but is
/// drawn at another size and optical width, so the run looks unevenly
/// spaced rather than misaligned. That is a font to install rather than a
/// bug to fix here, but it is the reason a status a reader must be able to
/// scan across a list is left in ordinary case.
pub(crate) fn small_caps(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'a' => 'ᴀ',
            'b' => 'ʙ',
            'c' => 'ᴄ',
            'd' => 'ᴅ',
            'e' => 'ᴇ',
            'f' => 'ꜰ',
            'g' => 'ɢ',
            'h' => 'ʜ',
            'i' => 'ɪ',
            'j' => 'ᴊ',
            'k' => 'ᴋ',
            'l' => 'ʟ',
            'm' => 'ᴍ',
            'n' => 'ɴ',
            'o' => 'ᴏ',
            'p' => 'ᴘ',
            'r' => 'ʀ',
            's' => 'ꜱ',
            't' => 'ᴛ',
            'u' => 'ᴜ',
            'v' => 'ᴠ',
            'w' => 'ᴡ',
            'y' => 'ʏ',
            'z' => 'ᴢ',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_character_is_measured_in_the_cells_it_takes() {
        assert_eq!(columns("abc"), 3);
        assert_eq!(columns("日本"), 4);
        assert_eq!(columns("a🙂"), 3);
    }

    #[test]
    fn an_elided_line_never_takes_more_cells_than_it_was_given() {
        let ellipsis = theme::glyph(Symbol::Ellipsis);
        let cut = elide("日本語のテキスト", 7);
        assert!(columns(&cut) <= 7, "{cut}");
        assert_eq!(cut, format!("日本語{ellipsis}"));
        assert_eq!(elide("hello world", 8), format!("hello w{ellipsis}"));
        assert_eq!(elide("hello", 5), "hello");
    }

    #[test]
    fn a_head_elided_path_keeps_the_tail_that_fits() {
        let ellipsis = theme::glyph(Symbol::Ellipsis);
        let cut = elide_head("~/プロジェクト/src", 9);
        assert!(columns(&cut) <= 9, "{cut}");
        assert_eq!(cut, format!("{ellipsis}クト/src"));
        assert_eq!(elide_head("~/code/uze", 6), format!("{ellipsis}e/uze"));
        assert_eq!(elide_head("~/uze", 6), "~/uze");
    }

    #[test]
    fn folded_rows_fit_their_width_in_cells() {
        for row in fold("漢字漢字漢字 and more", 5) {
            assert!(columns(&row) <= 5, "{row}");
        }
        assert_eq!(fold("one two three", 7), vec!["one two", "three"]);
        assert_eq!(fold("abcdefgh", 3), vec!["abc", "def", "gh"]);
    }

    #[test]
    fn a_character_wider_than_the_row_still_folds() {
        assert_eq!(fold("日本", 1), vec!["日", "本"]);
    }

    #[test]
    fn a_clipped_line_fits_its_width_in_cells() {
        let mut line = Line::from(vec!["key ".into(), "日本語のテキスト".into()]);
        clip(&mut line, 9);
        assert!(line.width() <= 9, "{line:?}");
    }
}
