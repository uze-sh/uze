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
use uze_theme::{Symbol, Token};

use super::{POPUP_H_PAD, Surface};
use crate::ui::theme;

/// Narrowest a menu is drawn, so a menu of one short word still reads as a
/// box rather than as a label that lost its row.
const MIN_WIDTH: u16 = 14;

/// One entry of a menu: its words, and whether it opens a further menu
/// rather than acting.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry<'a> {
    pub(crate) label: &'a str,
    pub(crate) opens: bool,
}

impl<'a> Entry<'a> {
    pub(crate) fn action(label: &'a str) -> Self {
        Self {
            label,
            opens: false,
        }
    }
}

/// Draws `entries` in a card just under `anchor`, kept inside `area`, with
/// `highlighted` filled, and answers each entry's rect in order for the
/// caller to register.
///
/// No entry gets a colour of its own, a destructive one included: the menu
/// reads as one list, and what an entry does is in its words. An entry that
/// opens a further menu ends in an arrow against the card's right edge, the
/// way a nested menu is marked everywhere; an ellipsis stays the mark of an
/// entry that asks before acting.
pub(crate) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    anchor: Rect,
    entries: &[Entry<'_>],
    highlighted: usize,
) -> Vec<Rect> {
    let arrow = theme::glyph(Symbol::Submenu);
    let arrow_width = Span::raw(arrow.as_str()).width();
    let content_width = entries
        .iter()
        .map(|entry| {
            let opens = if entry.opens { 1 + arrow_width } else { 0 };
            Span::raw(entry.label).width() + opens
        })
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
        let pad = POPUP_H_PAD as usize;
        let label = format!("{:pad$}{}", "", entry.label);
        let text = if entry.opens {
            let room = (inner.width as usize).saturating_sub(pad + arrow_width);
            format!("{label:<room$}{arrow}{:pad$}", "")
        } else {
            format!("{label:<width$}", width = inner.width as usize)
        };
        frame.render_widget(Paragraph::new(Span::styled(text, style)), row);
        rects.push(row);
    }
    rects
}
