//! A screen's heading: its name, the line saying what it is for, and the
//! rect left under them.
//!
//! It takes the two strings rather than the caller's own `Route`, because
//! a widget that named one client's model could not be drawn by the other
//! — and the rows it consumes are the same either way.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::Paragraph,
};
use uze_theme::Token;

use crate::ui::theme;

/// Every screen's header: the route's name in bold, its subtitle muted on
/// the next line, and an optional right-aligned trailer on the title's own
/// row (item count, doctor summary, source count — whatever that route
/// reports). Exactly the two-line header shape every route in the design
/// uses. Returns the area still available below the header plus its own
/// blank spacer row.
pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    subtitle: &str,
    trailer: Option<Span<'static>>,
) -> Rect {
    let title = title.to_owned();
    let title_style = Style::default()
        .fg(theme::color(Token::TextBright))
        .add_modifier(Modifier::BOLD);
    let title_row = Rect::new(area.x, area.y, area.width.saturating_sub(1), 1);
    if let Some(trailer) = trailer {
        let trailer_width = trailer.width() as u16;
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(trailer_width)])
            .split(title_row);
        frame.render_widget(Paragraph::new(Span::styled(title, title_style)), columns[0]);
        frame.render_widget(
            Paragraph::new(trailer).alignment(ratatui::layout::Alignment::Right),
            columns[1],
        );
    } else {
        frame.render_widget(Paragraph::new(Span::styled(title, title_style)), title_row);
    }
    if area.height > 1 {
        let subtitle_row = Rect::new(area.x, area.y + 1, area.width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                subtitle.to_owned(),
                theme::fg(Token::TextMuted),
            )),
            subtitle_row,
        );
    }
    let consumed = 3.min(area.height);
    Rect::new(
        area.x,
        area.y + consumed,
        area.width,
        area.height.saturating_sub(consumed),
    )
}

/// The management screens' header, one row: the name in bold, then what
/// the screen holds in muted words beside it — a count, a summary, or the
/// screen's own subtitle when it reports nothing — and an optional trailer
/// pinned to the right. Returns the area under it and the blank row after.
pub(crate) fn inline(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    note: Span<'static>,
    trailer: Option<ratatui::text::Line<'static>>,
) -> Rect {
    let trailer_width = trailer
        .as_ref()
        .map_or(0, |line| (line.width() as u16).min(area.width));
    if let Some(trailer) = trailer {
        frame.render_widget(
            Paragraph::new(trailer),
            Rect::new(
                area.right() - trailer_width,
                area.y,
                trailer_width,
                1.min(area.height),
            ),
        );
    }
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
    Rect::new(
        area.x,
        area.y + consumed,
        area.width,
        area.height.saturating_sub(consumed),
    )
}
