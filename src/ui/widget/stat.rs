//! Summary cards: a row of figures, each a muted label over its value,
//! divided from its neighbour by a hairline at its left edge.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Modifier,
    text::Span,
    widgets::Paragraph,
};
use uze_theme::Token;

use super::{Edge, Rule, text};
use crate::ui::theme;

/// One figure: what it counts, the value, and the hue the value says it
/// in.
pub(crate) struct Stat {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) hue: Token,
}

/// Draws `stats` side by side across `area`, each an equal share of it,
/// the label on the first row and the value in bold on the second.
pub(crate) fn cards(frame: &mut ratatui::Frame<'_>, area: Rect, stats: &[Stat]) {
    if stats.is_empty() || area.height == 0 {
        return;
    }
    let cells = Layout::horizontal(
        stats
            .iter()
            .map(|_| Constraint::Ratio(1, stats.len() as u32)),
    )
    .split(Rect {
        height: area.height.min(2),
        ..area
    });
    for (cell, stat) in cells.iter().zip(stats) {
        let inner = Rule::new(Edge::Left).render(frame, *cell);
        let room = usize::from(inner.width.saturating_sub(1));
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" {}", text::elide(&stat.label, room)),
                theme::fg(Token::TextMuted),
            )),
            Rect { height: 1, ..inner },
        );
        if inner.height > 1 {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {}", text::elide(&stat.value, room)),
                    theme::fg(stat.hue).add_modifier(Modifier::BOLD),
                )),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
        }
    }
}
