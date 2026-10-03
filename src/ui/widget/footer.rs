//! A modal's footer: a hairline over one row, what can be done here on
//! the left and the caller's trailers on the right.

use ratatui::{
    layout::{Alignment, Rect},
    text::Line,
    widgets::{Padding, Paragraph},
};

use super::{Edge, Rule, text};

/// Draws the footer into `area` and answers with the rect each trailer
/// took, in the order given, for a caller whose trailers answer a click.
/// Trailers are laid from the right edge inward, two columns apart, the
/// first one rightmost. `inset` is how far the row's text sits inside the
/// hairline's ends, so it can line up with whatever the modal heads its
/// content with.
pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    inset: u16,
    mut line: Line<'static>,
    trailers: Vec<Line<'static>>,
) -> Vec<Rect> {
    const GAP: u16 = 2;
    let inner = Rule::new(Edge::Top)
        .padding(Padding::new(inset, inset, 0, 0))
        .render(frame, area);
    let mut right = inner.right();
    let mut rects = Vec::with_capacity(trailers.len());
    for trailer in trailers {
        let width = (trailer.width() as u16).min(right.saturating_sub(inner.x));
        let rect = Rect::new(
            right.saturating_sub(width),
            inner.y,
            width,
            inner.height.min(1),
        );
        frame.render_widget(Paragraph::new(trailer).alignment(Alignment::Right), rect);
        rects.push(rect);
        right = rect.x.saturating_sub(GAP);
    }
    let hints = Rect::new(
        inner.x,
        inner.y,
        right.saturating_sub(inner.x).max(10.min(inner.width)),
        inner.height.min(1),
    );
    // One row: a line that does not fit is elided rather than wrapped into
    // a second row the footer does not have.
    text::clip(&mut line, hints.width as usize);
    frame.render_widget(Paragraph::new(line), hints);
    rects
}
