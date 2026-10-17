//! A tab bar: one numbered tab per screen or section, the open one on a
//! ground of its own, saying as much of itself as the room allows.
//!
//! The manage modal's header drew this alone until the space overview's
//! section bar needed the same strip — its second file.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_theme::Token;

use crate::ui::theme;

/// Between one tab and the next.
pub(crate) const GAP: u16 = 2;

/// One tab: its name, an optional mark before it (a state glyph) and an
/// optional badge after it. Its number is its position, which is the key
/// that reaches it, so the caller never spells it.
#[derive(Clone, Debug)]
pub(crate) struct Tab {
    label: String,
    lead: Option<(String, Token)>,
    badge: Option<(String, Token)>,
}

impl Tab {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            lead: None,
            badge: None,
        }
    }

    /// A mark between the number and the name, in `tone`. Kept at every
    /// level of detail: a tab shrunk to its number still says its state.
    pub(crate) fn lead(mut self, mark: impl Into<String>, tone: Token) -> Self {
        self.lead = Some((mark.into(), tone));
        self
    }

    /// A badge after the name, in small caps and `tone` — the first thing
    /// a strip that does not fit gives up.
    pub(crate) fn badge(mut self, badge: Option<&str>, tone: Token) -> Self {
        self.badge = badge.map(|badge| (super::text::small_caps(badge), tone));
        self
    }
}

/// How much of itself a tab says, from all of it down to its number
/// alone: a strip that does not fit gives up the badges first, then the
/// names of the tabs that are not open. The open one keeps its name.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Detail {
    OpenNameOnly,
    Name,
    Badge,
}

/// Draws `tabs` along `row` at the most detail that fits, the one at
/// `open` (if any) on a raised ground — or the selection's, while the
/// keyboard is on the strip itself (`focused`). Answers with the rect each
/// drawn tab took; a tab past the end of the row is not drawn.
pub(crate) fn tab_bar<T: Clone>(
    frame: &mut ratatui::Frame<'_>,
    row: Rect,
    tabs: &[(Tab, T)],
    open: Option<usize>,
    focused: bool,
) -> Vec<(Rect, T)> {
    let lines = fitted(tabs, open, focused, row.width);
    let mut x = row.x;
    let mut placed = Vec::with_capacity(tabs.len());
    for ((_, target), line) in tabs.iter().zip(lines) {
        let width = line.width() as u16;
        if x + width > row.right() {
            break;
        }
        let rect = Rect::new(x, row.y, width, row.height);
        frame.render_widget(Paragraph::new(line), rect);
        placed.push((rect, target.clone()));
        x += width + GAP;
    }
    placed
}

/// The tabs' lines at the most detail that fits in `room` columns.
fn fitted<T>(
    tabs: &[(Tab, T)],
    open: Option<usize>,
    focused: bool,
    room: u16,
) -> Vec<Line<'static>> {
    let lines = |detail| -> Vec<Line<'static>> {
        tabs.iter()
            .enumerate()
            .map(|(index, (tab, _))| line(index + 1, tab, open == Some(index), focused, detail))
            .collect()
    };
    let width = |lines: &[Line<'static>]| -> u16 {
        lines
            .iter()
            .map(|line| line.width() as u16 + GAP)
            .sum::<u16>()
            .saturating_sub(GAP)
    };
    [Detail::Badge, Detail::Name]
        .into_iter()
        .map(lines)
        .find(|candidate| width(candidate) <= room)
        .unwrap_or_else(|| lines(Detail::OpenNameOnly))
}

fn line(position: usize, tab: &Tab, open: bool, focused: bool, detail: Detail) -> Line<'static> {
    let ground = match (open, focused) {
        (true, true) => Some(Token::SurfaceSelected),
        (true, false) => Some(Token::SurfaceRaised),
        (false, _) => None,
    };
    let with_ground = |style: Style| match ground {
        Some(token) => style.bg(theme::color(token)),
        None => style,
    };
    let (number, label) = if open {
        (
            theme::fg_bold(Token::Accent),
            theme::fg_bold(Token::TextBright),
        )
    } else {
        (
            theme::fg_bold(Token::TextDim),
            theme::fg(Token::TextSecondary),
        )
    };
    let mut spans = vec![
        Span::styled(" ", with_ground(Style::default())),
        Span::styled(position.to_string(), with_ground(number)),
    ];
    if let Some((mark, tone)) = &tab.lead {
        spans.push(Span::styled(
            format!(" {mark}"),
            with_ground(theme::fg(*tone)),
        ));
    }
    if open || detail >= Detail::Name {
        spans.push(Span::styled(format!(" {}", tab.label), with_ground(label)));
    }
    if let (Detail::Badge, Some((badge, tone))) = (detail, &tab.badge) {
        spans.push(Span::styled(
            format!(" {badge}"),
            with_ground(theme::fg(*tone)),
        ));
    }
    spans.push(Span::styled(" ", with_ground(Style::default())));
    Line::from(spans)
}
