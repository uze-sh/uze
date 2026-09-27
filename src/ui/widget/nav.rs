//! A navigation entry: one item of a modal's sidebar — its name with a
//! count pinned right, the caption beneath it, and the bar at its edge
//! that lights up on the one in front.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_theme::Token;

use super::{fill, text};
use crate::ui::theme::{self, Symbol};

/// The style an entry's name is drawn in. The caller builds the name's
/// line from it, since a name may carry a badge of its own beside it.
pub(crate) fn label_style(selected: bool) -> Style {
    if selected {
        theme::fg_bold(Token::TextBright).bg(theme::color(Token::SurfaceRaised))
    } else {
        theme::fg(Token::TextInactive)
    }
}

/// Draws one entry into `rect`: a bar at its edge, `label` with `count`
/// pinned right, and — on a rect two tall — `caption` beneath. The
/// selected entry is a raised band the bar lights up on.
pub(crate) fn entry(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    label: Line<'static>,
    caption: &str,
    selected: bool,
    count: Option<usize>,
) {
    let ground = if selected {
        Token::SurfaceRaised
    } else {
        Token::SurfaceBackground
    };
    if selected {
        fill(frame, rect, ground);
    }
    let bar_hue = if selected {
        Token::Accent
    } else {
        Token::SurfaceBackground
    };
    for dy in 0..rect.height {
        frame.render_widget(
            Paragraph::new(Span::styled(
                theme::glyph(Symbol::BarMedium),
                theme::on(bar_hue, ground),
            )),
            Rect::new(rect.x, rect.y + dy, 1.min(rect.width), 1),
        );
    }

    let text_x = rect.x + 2.min(rect.width);
    let text_width = rect.width.saturating_sub(3);
    let raised = |style: Style| {
        if selected {
            style.bg(theme::color(Token::SurfaceRaised))
        } else {
            style
        }
    };
    let label_rect = Rect::new(text_x, rect.y, text_width, 1);
    let label_rect = match count {
        Some(count) => {
            let count = text::small_digits(count);
            let [label, count_rect] = Layout::horizontal([
                Constraint::Min(1),
                Constraint::Length(text::columns(&count) as u16),
            ])
            .areas(label_rect);
            frame.render_widget(
                Paragraph::new(Span::styled(count, raised(theme::fg(Token::Accent))))
                    .alignment(Alignment::Right),
                count_rect,
            );
            label
        }
        None => label_rect,
    };
    frame.render_widget(Paragraph::new(label), label_rect);
    if rect.height > 1 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                text::elide(caption, usize::from(text_width)),
                raised(theme::fg(Token::TextDim)),
            )),
            Rect::new(text_x, rect.y + 1, text_width, 1),
        );
    }
}
