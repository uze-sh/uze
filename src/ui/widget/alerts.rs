//! A list of things to check, in a dialog: the management's Health and a
//! space's issues read the same, worst first, each what it is about and
//! what to do folded under it.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_theme::{Symbol, Token};

use super::{dialog, text};
use crate::ui::theme;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    /// The mark and hue an alert of this weight is drawn with.
    pub(crate) fn mark(self) -> (Symbol, Token) {
        match self {
            Self::High => (Symbol::MarkClose, Token::StateDanger),
            Self::Medium => (Symbol::MarkAttention, Token::StateWarning),
            Self::Low => (Symbol::MarkDot, Token::Accent),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Alert {
    pub(crate) severity: Severity,
    pub(crate) label: String,
    pub(crate) detail: String,
}

/// Draws `alerts` in a dialog titled `title`, worst first, or says nothing
/// needs attention; answers the rect the dialog covers.
pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    alerts: &[Alert],
    hint: Line<'static>,
) -> Rect {
    const WIDTH: u16 = 72;
    let measure = usize::from(WIDTH.saturating_sub(dialog::shell_chrome_width()));
    let mut sorted: Vec<_> = alerts.iter().collect();
    sorted.sort_by_key(|alert| alert.severity);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if sorted.is_empty() {
        lines.push(Line::from(Span::styled(
            "Nothing needs attention.",
            theme::fg(Token::TextMuted),
        )));
    }
    for (index, alert) in sorted.into_iter().enumerate() {
        // A row of air between alerts, so each reads as its own.
        if index > 0 {
            lines.push(Line::default());
        }
        let (symbol, hue) = alert.severity.mark();
        let lead = format!("{} ", theme::glyph(symbol));
        let indent = text::columns(&lead);
        // What it is about on the first row, what to do folded under it,
        // past the mark, so every alert starts at the same column.
        lines.push(Line::from(vec![
            Span::styled(lead, theme::fg(hue)),
            Span::styled(alert.label.clone(), theme::fg(Token::TextBright)),
        ]));
        let balanced = alert.detail.matches('`').count().is_multiple_of(2);
        let mut quoted = false;
        for row in text::fold(&alert.detail, measure.saturating_sub(indent).max(1)) {
            let mut spans = vec![Span::raw(" ".repeat(indent))];
            if balanced {
                spans.extend(detail_spans(&row, &mut quoted));
            } else {
                spans.push(Span::styled(row, theme::fg(Token::TextMuted)));
            }
            lines.push(Line::from(spans));
        }
    }
    let shell = dialog::shell(frame, area, WIDTH, lines.len() as u16, title, hint);
    frame.render_widget(Paragraph::new(lines), shell.body);
    shell.popup
}

/// One row of an alert's detail, with what it quotes in backticks drawn as
/// code and the backticks dropped, as a toast draws it: a quoted span is a
/// setting or a command to reach for. `quoted` carries an open quote across
/// a fold, so a span split over two rows stays code.
fn detail_spans(row: &str, quoted: &mut bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, piece) in row.split('`').enumerate() {
        if index > 0 {
            *quoted = !*quoted;
        }
        if piece.is_empty() {
            continue;
        }
        let tone = if *quoted {
            Token::TextCode
        } else {
            Token::TextMuted
        };
        spans.push(Span::styled(piece.to_owned(), theme::fg(tone)));
    }
    spans
}
