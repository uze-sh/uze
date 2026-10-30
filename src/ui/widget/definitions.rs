//! A list of definitions: each entry a name in a column, and what it says
//! folded beside it, hanging under its own first row.
//!
//! It joined when a panel's key/value fields (the Marketplace's), a
//! drawer's tables of grants and programs and a consent's grants had each
//! written the same layout by hand, the consent and the drawer listing the
//! very same grants. The name column is either fixed, so a panel's keys
//! line up with the next panel's, or as wide as the longest name.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::text;

/// How wide the name column is.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Column {
    /// This many columns, whatever the names are.
    Fixed(usize),
    /// As wide as the longest name with two columns of air, at most half
    /// the width.
    Fit,
}

/// One entry: its name and what it says, each in its own style.
pub(crate) struct Entry<'a> {
    pub(crate) name: &'a str,
    pub(crate) said: &'a str,
    pub(crate) name_style: Style,
    pub(crate) said_style: Style,
}

/// Each entry as its lines, `width` columns wide: the name on the first,
/// cut to its column, and what it says folded beside it. One list of
/// lines per entry, so a caller knows the rows each takes.
pub(crate) fn lines(
    entries: &[Entry<'_>],
    column: Column,
    width: usize,
) -> Vec<Vec<Line<'static>>> {
    let column = match column {
        Column::Fixed(columns) => columns,
        Column::Fit => entries
            .iter()
            .map(|entry| text::columns(entry.name) + 2)
            .max()
            .unwrap_or(0)
            .min(width / 2),
    };
    let room = width.saturating_sub(column).max(1);
    entries
        .iter()
        .map(|entry| {
            let name = text::elide(entry.name, column.saturating_sub(1));
            text::fold(entry.said, room)
                .into_iter()
                .enumerate()
                .map(|(index, row)| {
                    let cell = if index == 0 { name.as_str() } else { "" };
                    Line::from(vec![
                        Span::styled(cell.to_owned(), entry.name_style),
                        Span::raw(" ".repeat(column.saturating_sub(text::columns(cell)))),
                        Span::styled(row, entry.said_style),
                    ])
                })
                .collect()
        })
        .collect()
}
