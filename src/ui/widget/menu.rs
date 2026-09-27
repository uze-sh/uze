//! A menu: a short list of actions, opened on the thing they act on.
//!
//! It lived in the workspace's own renderer as the tab and space menu,
//! which meant a surface an extension draws could not offer one — the
//! second list of actions opened on a row would have been drawn a second
//! way.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{Clear, Paragraph},
};
use uze_theme::Token;

use super::{POPUP_H_PAD, Surface};
use crate::ui::theme;

/// Narrowest a menu is drawn, so a menu of one short word still reads as a
/// box rather than as a label that lost its row.
const MIN_WIDTH: u16 = 14;

/// Draws `entries` in a card just under `anchor`, kept inside `area`, with
/// `highlighted` filled, and answers each entry's rect in order for the
/// caller to register.
///
/// No entry gets a colour of its own, a destructive one included: the menu
/// reads as one list, and what an entry does is in its words.
pub(crate) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    anchor: Rect,
    entries: &[&str],
    highlighted: usize,
) -> Vec<Rect> {
    let content_width = entries
        .iter()
        .map(|entry| Span::raw(*entry).width())
        .max()
        .unwrap_or(0) as u16;
    let width = (content_width + 2 * POPUP_H_PAD + 2)
        .max(MIN_WIDTH)
        .min(area.width);
    let height = (entries.len() as u16 + 2).min(area.height);
    let popup = Rect::new(
        anchor.x.min((area.x + area.width).saturating_sub(width)),
        (anchor.y + anchor.height).min((area.y + area.height).saturating_sub(height)),
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let inner = Surface::card().render(frame, popup);

    let mut rects = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if index as u16 >= inner.height {
            break;
        }
        let row = Rect::new(inner.x, inner.y + index as u16, inner.width, 1);
        let style = if index == highlighted {
            Style::default()
                .bg(theme::color(Token::Accent))
                .fg(theme::color(Token::SurfaceBackground))
                .add_modifier(Modifier::BOLD)
        } else {
            theme::fg(Token::TextInactive)
        };
        let label = format!("{:pad$}{entry}", "", pad = POPUP_H_PAD as usize);
        let text = format!("{label:<width$}", width = inner.width as usize);
        frame.render_widget(Paragraph::new(Span::styled(text, style)), row);
        rects.push(row);
    }
    rects
}
