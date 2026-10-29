//! A segmented control: a few views of one place side by side, the one in
//! front on a ground of its own, choosing which the place below shows.
//!
//! Not a [`tabs`](super::tabs) strip: a tab is a screen reached by its
//! number, while a segment is a view of the same thing, two or three of
//! them, named and nothing else.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_theme::Token;

use crate::ui::theme;

/// Between one segment and the next.
const GAP: u16 = 1;

/// One segment: its name, what a click on it answers (`None` for the one
/// in front, which a click leaves where it is), and whether the pointer
/// is on it.
pub(crate) struct Segment<'a, T> {
    pub(crate) label: &'a str,
    pub(crate) target: Option<T>,
    pub(crate) hovered: bool,
}

/// Draws `segments` from `row`'s start, the one at `open` on the raised
/// ground in the brightest ink, the rest held back until the pointer
/// lights them. Answers with the rect of each segment that has a target.
pub(crate) fn segmented<T: Clone>(
    frame: &mut ratatui::Frame<'_>,
    row: Rect,
    segments: &[Segment<'_, T>],
    open: usize,
) -> Vec<(Rect, T)> {
    let mut x = row.x;
    let mut placed = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let text = format!(" {} ", segment.label);
        let width = text.chars().count() as u16;
        if x + width > row.right() {
            break;
        }
        let style = if index == open {
            theme::fg_bold(Token::TextBright).bg(theme::color(Token::SurfaceRaised))
        } else if segment.hovered {
            theme::fg(Token::TextBright)
        } else {
            theme::fg(Token::TextSecondary)
        };
        let rect = Rect::new(x, row.y, width, 1);
        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
        if let Some(target) = &segment.target {
            placed.push((rect, target.clone()));
        }
        x += width + GAP;
    }
    placed
}

/// How many columns `segments` take, drawn by [`segmented`].
pub(crate) fn width<T>(segments: &[Segment<'_, T>]) -> u16 {
    segments
        .iter()
        .map(|segment| segment.label.chars().count() as u16 + 2)
        .sum::<u16>()
        + GAP * segments.len().saturating_sub(1) as u16
}
