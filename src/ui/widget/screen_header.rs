//! A screen's heading: its name, the line saying what it is for, and the
//! rect left under them.
//!
//! It takes the two strings rather than the caller's own `Route`, because
//! a widget that named one client's model could not be drawn by the other
//! — and the rows it consumes are the same either way.

use ratatui::{layout::Rect, text::Span, widgets::Paragraph};
use uze_theme::Token;

use super::hint::Hints;
use crate::ui::theme;

/// What a heading left: the area under it, and where the keys of its
/// trailer landed.
pub(crate) struct Heading {
    pub(crate) below: Rect,
    pub(crate) offers: Vec<(Rect, uze_keys::Action)>,
}

/// The management screens' header, one row: the name in bold, then what
/// the screen holds in muted words beside it — a count, a summary, or the
/// screen's own subtitle when it reports nothing — and an optional row of
/// hints pinned to the right: what can be done here that is about no row.
pub(crate) fn inline(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    note: Span<'static>,
    trailer: Option<Hints<'_>>,
) -> Heading {
    let trailer_width = trailer
        .as_ref()
        .map_or(0, |hints| hints.width().min(area.width));
    let offers = trailer.map_or_else(Vec::new, |hints| {
        hints.render(
            frame,
            Rect::new(
                area.right() - trailer_width,
                area.y,
                trailer_width,
                1.min(area.height),
            ),
        )
    });
    let room = area
        .width
        .saturating_sub(trailer_width + u16::from(trailer_width > 0) * 2);
    let mut heading = ratatui::text::Line::from(vec![
        Span::styled(title.to_owned(), theme::fg_bold(Token::TextBright)),
        Span::raw("  "),
        note,
    ]);
    super::text::clip(&mut heading, room.into());
    frame.render_widget(
        Paragraph::new(heading),
        Rect::new(area.x, area.y, room, 1.min(area.height)),
    );
    let consumed = 2.min(area.height);
    Heading {
        below: Rect::new(
            area.x,
            area.y + consumed,
            area.width,
            area.height.saturating_sub(consumed),
        ),
        offers,
    }
}
