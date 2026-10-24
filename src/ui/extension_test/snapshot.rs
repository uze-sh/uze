//! A snapshot of a surface: the text it drew, then the same grid as one
//! letter per cell naming the colour token of its glyph, with a legend.
//!
//! Tokens, never colour values: a palette tweak would otherwise break every
//! plugin's snapshots, and a token is what the author chose (through a
//! role). A colour two tokens share in one theme reads as the first of them
//! in `Token`'s order, which is the same every run.

use ratatui::buffer::Buffer;
use uze_contract::{PROTOCOL, Slot, ThemeName};
use uze_theme::Token;

use crate::ui::theme;

const LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

pub(super) fn take(
    buffer: &Buffer,
    slot: Slot,
    width: u16,
    height: u16,
    name: ThemeName,
) -> String {
    let area = buffer.area;
    let mut text_rows = Vec::new();
    let mut token_rows = Vec::new();
    let mut legend: Vec<Token> = Vec::new();
    for y in 0..area.height {
        let mut text = String::new();
        let mut tokens = String::new();
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            text.push_str(symbol);
            if symbol.trim().is_empty() {
                tokens.push(' ');
                continue;
            }
            let letter = match token_of(cell.fg) {
                Some(token) => {
                    let index = legend
                        .iter()
                        .position(|known| *known == token)
                        .unwrap_or_else(|| {
                            legend.push(token);
                            legend.len() - 1
                        });
                    LETTERS.chars().nth(index).unwrap_or('?')
                }
                None => '.',
            };
            tokens.push(letter);
            // A wide glyph's second cell is drawn by the first.
            for _ in 1..unicode_width::UnicodeWidthStr::width(symbol) {
                tokens.push(letter);
            }
        }
        text_rows.push(text.trim_end().to_owned());
        token_rows.push(tokens.trim_end().to_owned());
    }
    let mut out = format!(
        "# uze snapshot · protocol {PROTOCOL} · {} {width}x{height} · {}\n",
        slot.name(),
        name.as_str()
    );
    for row in &text_rows {
        out.push_str(row);
        out.push('\n');
    }
    out.push_str("---\n");
    for row in &token_rows {
        out.push_str(row);
        out.push('\n');
    }
    out.push_str("---\n");
    for (index, token) in legend.iter().enumerate() {
        out.push_str(&format!(
            "{} {}\n",
            LETTERS.chars().nth(index).unwrap_or('?'),
            token.name()
        ));
    }
    out
}

fn token_of(color: ratatui::style::Color) -> Option<Token> {
    Token::ALL
        .iter()
        .copied()
        .find(|token| theme::color(*token) == color)
}

/// The rows where `held` and `taken` part, before and after, at most a
/// handful of them.
pub(super) fn difference(held: &str, taken: &str) -> String {
    const SHOWN: usize = 6;
    let held: Vec<&str> = held.lines().collect();
    let taken: Vec<&str> = taken.lines().collect();
    let mut said = String::new();
    let mut shown = 0;
    for index in 0..held.len().max(taken.len()) {
        let (before, after) = (held.get(index).copied(), taken.get(index).copied());
        if before == after {
            continue;
        }
        if shown == SHOWN {
            said.push_str("\n  …");
            break;
        }
        said.push_str(&format!(
            "\n  line {}\n  - {}\n  + {}",
            index + 1,
            before.unwrap_or(""),
            after.unwrap_or("")
        ));
        shown += 1;
    }
    said
}
