//! A notice laid over a surface's content: something is wrong with what is
//! drawn, said where it is drawn, without taking it away.
//!
//! A toast says what happened and leaves; a message stands in for content
//! there is none of. This is the third case: content that is still worth
//! reading — the last good draw of a plugin's view — and a problem with
//! the newer version of it the author has to see while they fix it. It
//! sits along the bottom of the content, on a raised ground so it reads as
//! over it rather than part of it, and it is as tall as its lines and no
//! taller than half the room.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use uze_theme::{Symbol, Token};

use super::{Edge, Rule, fill, text};
use crate::ui::theme;

/// Draws `title` and `lines` over the bottom of `area`, each line cut at
/// its end to fit; answers the rect it took.
pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    lines: &[String],
) -> Rect {
    if area.height < 2 || area.width < 4 {
        return Rect::new(area.x, area.y, 0, 0);
    }
    let wanted = u16::try_from(lines.len() + 1).unwrap_or(u16::MAX);
    let height = wanted.min((area.height / 2).max(2));
    let rect = Rect::new(area.x, area.bottom() - height, area.width, height);
    frame.render_widget(Clear, rect);
    fill(frame, rect, Token::SurfaceRaised);
    let inner = Rule::new(Edge::Left)
        .tone(Token::StateDanger)
        .ground(Token::SurfaceRaised)
        .render(frame, rect);
    let measure = usize::from(inner.width.saturating_sub(2));
    let mut drawn = vec![Line::from(vec![
        Span::raw(" "),
        Span::styled(
            text::elide(
                &format!("{} {title}", theme::glyph(Symbol::MarkAttention)),
                measure,
            ),
            theme::fg(Token::StateDanger),
        ),
    ])];
    let room = usize::from(inner.height.saturating_sub(1));
    let shown = if lines.len() > room {
        room.saturating_sub(1)
    } else {
        lines.len()
    };
    drawn.extend(lines.iter().take(shown).map(|line| {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(text::elide(line, measure), theme::fg(Token::TextSecondary)),
        ])
    }));
    if shown < lines.len() {
        drawn.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{} more", lines.len() - shown),
                theme::fg(Token::TextMuted),
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(drawn), inner);
    rect
}
