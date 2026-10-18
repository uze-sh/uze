//! What an empty surface says: what is the matter, and — when there is
//! something to do about it — what to do, centred where its content would
//! stand. The extension surfaces and the space overview say it the same way.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::ui::theme::{self, Token};

use super::text;

/// An empty surface, or one that failed: what is the matter, and — when
/// there is something to do about it — what to do.
///
/// Set a third of the way down rather than pinned to the top edge. A line
/// of text against the top-left corner reads as a document that got cut
/// off, which is the one thing an empty surface must not look like; the
/// same words with space above and below read as the state they are.
pub(crate) fn render_message(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    text: &str,
    hint: Option<&str>,
    colour: Color,
) {
    if area.height == 0 {
        return;
    }
    let lines = message_lines(text, hint, area.width, colour);
    // Centred on the block's real height, wrapped lines included, so a
    // long hint does not push the message off the middle it was aimed at.
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .min(area.height);
    let top = area.y + (area.height - height) / 2;
    frame.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        Rect::new(area.x, top, area.width, height),
    );
}

/// The widest a message's lines run: a sentence across a whole wide pane
/// is read as a strip, not as a sentence.
pub(crate) const MESSAGE_MEASURE: u16 = 48;

/// A message as the lines it is drawn in. A message that carries a hint is
/// a heading over it, so it reads as one in capitals; one on its own — an
/// error, "reading…" — is a sentence and keeps its case.
pub(crate) fn message_lines(
    text: &str,
    hint: Option<&str>,
    width: u16,
    colour: Color,
) -> Vec<Line<'static>> {
    let measure = usize::from(width.clamp(1, MESSAGE_MEASURE));
    let title = if hint.is_some() {
        text.to_uppercase()
    } else {
        text.to_owned()
    };
    let mut lines: Vec<Line<'static>> = text::fold(&title, measure)
        .into_iter()
        .map(|line| {
            Line::from(Span::styled(
                line,
                Style::default().fg(colour).add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    if let Some(hint) = hint {
        lines.push(Line::from(""));
        let mut quoted = false;
        lines.extend(
            hint_rows(hint, measure)
                .iter()
                .map(|row| hint_line(row, &mut quoted)),
        );
    }
    lines
}

/// One row of a hint, with what it quotes in backticks a step brighter and
/// the backticks themselves dropped: a quoted span is something to type,
/// and it is the one part of a hint a reader looks for. A step, not a hue:
/// the hint is still the quiet half of the message. `quoted` carries an
/// open quote across a fold, so a span split over two rows stays lit.
pub(crate) fn hint_line(row: &str, quoted: &mut bool) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, piece) in row.split('`').enumerate() {
        if index > 0 {
            *quoted = !*quoted;
        }
        if piece.is_empty() {
            continue;
        }
        let style = if *quoted {
            theme::fg(Token::TextPrimary)
        } else {
            theme::fg(Token::TextMuted)
        };
        spans.push(Span::styled(piece.to_owned(), style));
    }
    Line::from(spans)
}

/// A hint's rows: each of its lines on a row of its own, kept as written
/// where it fits — so a list the extension lined up in columns stays lined
/// up — and folded between words where it does not.
pub(crate) fn hint_rows(hint: &str, measure: usize) -> Vec<String> {
    hint.lines()
        .flat_map(|line| {
            if text::columns(line) <= measure {
                vec![line.to_owned()]
            } else {
                text::fold(line, measure)
            }
        })
        .collect()
}
