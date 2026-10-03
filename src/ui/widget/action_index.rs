//! The open index of everything: every action reachable from where the
//! reader is, with the key that runs it.
//!
//! A composite rather than a primitive — it is assembled from
//! the [`dialog`](super::dialog) shell, the search row and a row per action
//! — and it is here for the same reason a primitive is: both clients draw
//! it, and until now both *built* it.
//!
//! The two copies had already drifted where it matters. One dimmed the key
//! column for an action that has no chord, so the blank reads as "nothing
//! to press"; the other left it in the accent, which says a key is there
//! and it is empty. Nobody chose that difference, and no reader of either
//! file could have seen it.
//!
//! What is *not* shared is which rows there are: the management client
//! folds in the offers of whatever its screen has selected, and the
//! workspace client has no such selection. Only the narrowing is common,
//! and it has to be — the same query answering differently in two places
//! is the bug a shared filter prevents.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_theme::Token;

use super::{Field, Scrollbar};
use crate::ui::theme::{self, Symbol};

/// One line of the index: an action, and the chord that runs it where one
/// is bound.
pub(crate) type Row = (uze_keys::Action, Option<uze_keys::Chord>);

/// The rows whose label or description contains `filter`, or all of them
/// when nothing is typed.
///
/// Both the label and the description, because the reader is searching for
/// what they want to *do* and the word for it is as often in one as in the
/// other.
pub(crate) fn narrowed(rows: Vec<Row>, filter: &str) -> Vec<Row> {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return rows;
    }
    rows.into_iter()
        .filter(|(action, _)| {
            action.label().to_lowercase().contains(&needle)
                || action.description().to_lowercase().contains(&needle)
        })
        .collect()
}

/// The narrowest and widest the index is drawn. Narrow enough to sit on a
/// small terminal, and capped so a long description does not stretch the
/// whole screen into one unreadable line.
const MIN_WIDTH: u16 = 30;
const MAX_WIDTH: u16 = 72;

/// The most actions the index shows at once.
///
/// A cap rather than the whole list, for the same reason there is a cap on
/// the width: the index grows with every action the product gains, and a
/// surface whose size is a function of that is one nobody chose. Past this
/// it scrolls, which is what a list does.
const MAX_ROWS: usize = 12;

/// Rows the body spends on something other than actions: the search row
/// and the row of air under it.
const FIELD_ROWS: u16 = 2;

/// Draws the index over `area`, answering with the rect each row took.
///
/// `entry` turns a row's position into the caller's own hit, because the
/// two clients have different hit enums and neither is this module's
/// business — the same arrangement [`button_row`](super::button_row) uses.
/// The caller registers the rects: they belong on *top* of whatever is
/// underneath, which is the caller's list to splice into.
///
/// `hint` is the keys that answer it, which the caller's keymap scopes
/// decide, for the bottom border.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render<H>(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    rows: &[Row],
    reachable: usize,
    filter: &str,
    selected: usize,
    hint: Line<'static>,
    entry: impl Fn(usize) -> H,
) -> Vec<(Rect, H)> {
    let key_width = rows
        .iter()
        .filter_map(|(_, chord)| chord.map(|chord| chord.to_string().chars().count()))
        .max()
        .unwrap_or(0)
        .max(4);
    let width = area.width.saturating_sub(8).clamp(MIN_WIDTH, MAX_WIDTH);
    // Sized by what is reachable here, not by what the filter left: a
    // surface that resizes under the typing that narrows it moves the rows
    // the reader is aiming at, and the first keystroke is when they are
    // least able to follow.
    let shown = reachable.min(MAX_ROWS) as u16;
    let shell = super::dialog::shell(frame, area, width, shown + FIELD_ROWS, "Help", hint);
    let inner = shell.body;

    // Always focused: the index is open, so every key reaches this field.
    // The same search row every list in the product heads itself with.
    Field::new(filter, "type to narrow").render_search(frame, Rect { height: 1, ..inner });

    let mut list = Rect::new(
        inner.x,
        inner.y + FIELD_ROWS,
        inner.width,
        inner.height.saturating_sub(FIELD_ROWS),
    );
    let visible = usize::from(list.height);
    // The window is derived from the selection rather than kept as an
    // offset of its own — moving with the arrows is the only way this list
    // scrolls — so it holds the chosen row a little in from the edge it is
    // approaching rather than pinned to it.
    let first = selected
        .saturating_sub(visible / 2)
        .min(rows.len().saturating_sub(visible));
    let bar = Scrollbar::measure(
        Rect::new(
            list.right().saturating_sub(Scrollbar::width()),
            list.y,
            Scrollbar::width(),
            list.height,
        ),
        visible,
        rows.len(),
    );
    if bar.is_some() {
        list.width = list.width.saturating_sub(Scrollbar::width());
    }
    let mut entries = Vec::with_capacity(visible);
    for (position, (action, chord)) in rows.iter().enumerate().skip(first).take(visible) {
        let y = list.y + (position - first) as u16;
        if y >= list.bottom() {
            break;
        }
        let row = Rect::new(list.x, y, list.width, 1);
        let chosen = position == selected;
        // An action with no key is a finished design, not a gap — it is
        // reached by pointer and from here — so the column it would have
        // filled is dimmed rather than left reading as an empty binding.
        let key = match chord {
            Some(chord) => chord.to_string(),
            None => String::new(),
        };
        let mut label = Style::default().fg(theme::color(if chosen {
            Token::TextBright
        } else if action.destructive() {
            Token::StateDanger
        } else {
            Token::TextPrimary
        }));
        if chosen {
            label = label.add_modifier(Modifier::BOLD);
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                // The cursor every list in the product points with.
                Span::styled(
                    if chosen {
                        format!("{} ", theme::glyph(Symbol::Prompt))
                    } else {
                        " ".repeat(usize::from(theme::width(Symbol::Prompt)) + 1)
                    },
                    theme::fg_bold(Token::Accent),
                ),
                Span::styled(
                    format!("{key:<key_width$}  "),
                    theme::fg(if chord.is_some() {
                        Token::Accent
                    } else {
                        Token::TextDim
                    }),
                ),
                Span::styled(action.label(), label),
                Span::styled(
                    format!(
                        "  {} {}",
                        theme::glyph(Symbol::EmDash),
                        action.description()
                    ),
                    theme::fg(Token::TextMuted),
                ),
            ])),
            row,
        );
        entries.push((row, entry(position)));
    }
    if let Some(bar) = bar {
        bar.render(frame, first);
    }
    entries
}
