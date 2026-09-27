//! A modal's footer: a hairline over one row, what can be done here on
//! the left and a trailer of the caller's on the right.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

use super::{Edge, Rule, text};

/// Draws the footer into `area` and answers with the rect the trailer
/// took, for a caller whose trailer answers a click.
pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    mut line: Line<'static>,
    trailer: Option<Span<'static>>,
) -> Option<Rect> {
    let inner = Rule::new(Edge::Top)
        .padding(Padding::new(1, 1, 0, 0))
        .render(frame, area);
    let trailer_width = trailer.as_ref().map_or(0, |span| span.width() as u16);
    let [hints, _, trailer_rect] = Layout::horizontal([
        Constraint::Min(10),
        Constraint::Length(if trailer.is_some() { 2 } else { 0 }),
        Constraint::Length(trailer_width),
    ])
    .areas(inner);
    // One row: a line that does not fit is elided rather than wrapped into
    // a second row the footer does not have.
    text::clip(&mut line, hints.width as usize);
    frame.render_widget(Paragraph::new(line), hints);
    let trailer = trailer?;
    frame.render_widget(
        Paragraph::new(trailer).alignment(Alignment::Right),
        trailer_rect,
    );
    Some(trailer_rect)
}
