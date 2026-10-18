//! A row: one line of a list, and the ground it is drawn on.
//!
//! Every list in the product answers the same two questions on every line
//! — is this the one the keyboard is on, and is the pointer over it — and
//! before this module each of them answered in its own spelling. The
//! ground a selected row takes is the most-copied decision in the UI.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use uze_theme::Token;

use super::{TRAILING_PAD, text};
use crate::ui::theme;

/// Where a row stands relative to the reader.
///
/// Selection and hover are separate questions, not two points on one
/// scale: the keyboard is on exactly one row and the pointer may be on
/// another, and a list that collapses them loses track of where a click
/// would land.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RowState {
    /// Neither the keyboard nor the pointer.
    #[default]
    Resting,
    /// The pointer is over it.
    Hovered,
    /// The keyboard is on it.
    Selected,
}

impl RowState {
    /// The two questions a caller already has the answers to, as the one
    /// value the row is drawn from. Selection wins: a row under the
    /// pointer *and* under the keyboard is where a press would land, and
    /// that is what the stronger ground says.
    pub(crate) fn of(selected: bool, hovered: bool) -> Self {
        match (selected, hovered) {
            (true, _) => Self::Selected,
            (false, true) => Self::Hovered,
            (false, false) => Self::Resting,
        }
    }

    /// The ground this row is drawn on, or `None` where it takes the
    /// surface it already sits on.
    pub(crate) fn ground(self) -> Option<Token> {
        match self {
            Self::Resting => None,
            Self::Hovered => Some(Token::SurfaceHover),
            Self::Selected => Some(Token::SurfaceSelected),
        }
    }
}

/// Gives every span in `spans` the ground `bg`, then extends the line to
/// `width` with it.
///
/// A row's ground has to reach the full width or the fill stops where the
/// text does, which reads as a ragged highlight rather than as a selected
/// row. Spans keep their own foreground; only the ground is imposed.
pub(crate) fn pad_to(spans: &mut Vec<Span<'_>>, width: u16, bg: Color) {
    for span in spans.iter_mut() {
        span.style = span.style.bg(bg);
    }
    let used: usize = spans.iter().map(Span::width).sum();
    let gap = (width as usize).saturating_sub(used);
    spans.push(Span::styled(" ".repeat(gap), Style::default().bg(bg)));
}

/// Fills the row to `width` on the ground its state calls for, and leaves
/// a resting row untouched.
///
/// Untouched rather than padded with the surface behind it: a list draws
/// on whatever ground its column already has, and a resting row that
/// painted its own would have to know which — the coupling that had every
/// list spelling this out for itself.
pub(crate) fn fill(spans: &mut Vec<Span<'_>>, width: u16, state: RowState) {
    let Some(ground) = state.ground() else {
        return;
    };
    pad_to(spans, width, theme::color(ground));
}

/// [`fill`], for a row that holds controls: a span with a ground of its
/// own — a field's box, a button — keeps it, and only the bare text and
/// the air between take the row's. Painted over, a control on the
/// selected row reads as a hole in it.
pub(crate) fn fill_under(spans: &mut Vec<Span<'_>>, width: u16, state: RowState) {
    let Some(ground) = state.ground() else {
        return;
    };
    let bg = theme::color(ground);
    for span in spans.iter_mut().filter(|span| span.style.bg.is_none()) {
        span.style = span.style.bg(bg);
    }
    let used: usize = spans.iter().map(Span::width).sum();
    let gap = (width as usize).saturating_sub(used);
    spans.push(Span::styled(" ".repeat(gap), Style::default().bg(bg)));
}

/// [`push_trailing`], with the text slid sideways by the clock (see
/// [`marquee`]) when it is too long for the room: a caption that cannot be
/// shortened without losing the end of it — a branch name is read from
/// both ends — passes through instead of stopping at an "…".
///
/// Only while something is asking to read it, which is the caller's
/// business: this draws the frame it is given the ticks for and says
/// whether there was anything to slide.
pub(crate) fn push_trailing_marquee<'a>(
    spans: &mut Vec<Span<'a>>,
    width: u16,
    text: String,
    hue: Color,
    ticks: usize,
) -> bool {
    let leading: u16 = spans.iter().map(|span| span.width() as u16).sum();
    let room = usize::from(width.saturating_sub(leading + TRAILING_PAD + 1).max(1));
    let Some(window) = marquee(&[(&text, Style::default().fg(hue))], room, ticks) else {
        push_trailing(spans, width, text, hue);
        return false;
    };
    spans.push(Span::raw(" ".repeat(usize::from(width).saturating_sub(
        usize::from(leading) + room + usize::from(TRAILING_PAD),
    ))));
    spans.extend(window);
    spans.push(Span::raw(" ".repeat(TRAILING_PAD as usize)));
    true
}

/// What `room` columns show of `pieces` `ticks` beats of the workspace
/// clock after the pointer came to rest on them, or `None` when they fit
/// and there is nothing to slide.
///
/// The pieces — a name and the quieter detail after it — slide as one
/// text, each column keeping the style of the piece it came from: sliding
/// only the piece that did not fit would leave the other one cut at an
/// "…" all the same.
///
/// The run is the text, a gap, and the text again, so it leaves on one
/// side as it arrives on the other rather than jumping back to the start.
/// It rests before it moves, and again each time it comes round whole: a
/// pointer crossing a row on its way somewhere else should see nothing
/// move, and a reader needs the start held long enough to begin reading.
pub(crate) fn marquee(
    pieces: &[(&str, Style)],
    room: usize,
    ticks: usize,
) -> Option<Vec<Span<'static>>> {
    /// Blank columns between the end of the run and its start coming
    /// round again, so the two ends are never read as one word.
    const GAP: usize = 3;
    /// Beats the text holds still before each pass — about a second at
    /// the workspace clock's 120 ms.
    const REST: usize = 10;
    /// Beats per column: a pace the eye can follow, rather than the
    /// clock's own, which is a spinner's.
    const PACE: usize = 2;
    let text: Vec<(char, Style)> = pieces
        .iter()
        .flat_map(|(text, style)| text.chars().map(|glyph| (glyph, *style)))
        .collect();
    let length: usize = text.iter().map(|(glyph, _)| text::cell_width(*glyph)).sum();
    if length <= room {
        return None;
    }
    let cycle = length + GAP;
    let offset = (ticks % (REST + cycle * PACE)).saturating_sub(REST) / PACE;
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut push = |glyph: char, style: Style| match spans.last_mut() {
        Some(last) if last.style == style => last.content.to_mut().push(glyph),
        _ => spans.push(Span::styled(glyph.to_string(), style)),
    };
    // Measured in cells, as `room` is: a wide glyph cut by either edge of
    // the window shows as the blank half it leaves rather than overflowing.
    let (mut column, mut used) = (0, 0);
    for (glyph, style) in text
        .iter()
        .copied()
        .chain(std::iter::repeat_n((' ', Style::default()), GAP))
        .chain(text.iter().copied())
    {
        let width = text::cell_width(glyph);
        let start = column;
        column += width;
        if column <= offset {
            continue;
        }
        let shown = if start < offset {
            column - offset
        } else {
            width
        };
        if shown < width || used + width > room {
            for _ in 0..shown.min(room - used) {
                push(' ', style);
            }
            used += shown.min(room - used);
        } else {
            push(glyph, style);
            used += width;
        }
        if used == room {
            break;
        }
    }
    Some(spans)
}

/// Appends `text` pinned to the row's right edge, `TRAILING_PAD` off the
/// divider — the column the agent rows keep their alias in.
///
/// `text` is elided rather than allowed to overflow. It is the row's
/// caption, and a caption that does not fit used to run past the edge and
/// be cut there by the frame — which is how a long branch name on the Git
/// section header became an unreadable fragment with no "…" to say it had
/// been shortened.
pub(crate) fn push_trailing<'a>(spans: &mut Vec<Span<'a>>, width: u16, text: String, hue: Color) {
    let leading: u16 = spans.iter().map(|span| span.width() as u16).sum();
    // One column of gap between the leading spans and the caption, so the
    // two never read as one word.
    let room = width.saturating_sub(leading + TRAILING_PAD + 1).max(1);
    let text = text::elide(&text, room as usize);
    let used = leading + text::columns(&text) as u16 + TRAILING_PAD;
    let gap = width.saturating_sub(used).max(1);
    spans.push(Span::raw(" ".repeat(gap as usize)));
    spans.push(Span::styled(text, Style::default().fg(hue)));
    spans.push(Span::raw(" ".repeat(TRAILING_PAD as usize)));
}

/// The first row of an informational popup: its name, and the key that
/// dismisses it pinned to the right.
pub(crate) fn title_row(name: &str, dismiss: &str, width: usize) -> ratatui::text::Line<'static> {
    let gap = width
        .saturating_sub(text::columns(name) + text::columns(dismiss))
        .max(1);
    Line::from(vec![
        Span::styled(
            name.to_owned(),
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(dismiss.to_owned(), theme::fg(Token::TextMuted)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name or caption in wide characters — a CJK branch, an emoji — takes
    /// two cells a character. Counted as one, the row ran past its column by
    /// the difference and the frame cut the pad or the caption off.
    #[test]
    fn a_row_of_wide_characters_fits_the_columns_it_was_given() {
        use ratatui::{style::Style, text::Span};

        use crate::ui::widget::row;

        let width = |spans: &[Span<'_>]| spans.iter().map(Span::width).sum::<usize>();

        let mut caption = vec![Span::raw("▾ Git")];
        row::push_trailing(
            &mut caption,
            20,
            "功能分支".to_owned(),
            theme::color(Token::TextMuted),
        );
        assert_eq!(width(&caption), 20);

        let title = row::title_row("提交说明", "esc", 30);
        assert_eq!(title.width(), 30);

        let name = "エージェントの作業ブランチ";
        for tick in 0..200 {
            let window = row::marquee(&[(name, Style::default())], 9, tick)
                .expect("a name wider than the room slides");
            assert_eq!(width(&window), 9, "at tick {tick}");
        }
    }

    /// A caption pinned to the right edge is elided to what is left of the
    /// row. It used to be appended whole and cut by the frame, which is how a
    /// long branch name on the Git section header lost both its ending and any
    /// sign that it had one.
    #[test]
    fn a_trailing_caption_is_elided_to_the_room_the_row_has_left() {
        use ratatui::text::Span;

        let mut spans = vec![Span::raw("▾ Git")];
        crate::ui::widget::row::push_trailing(
            &mut spans,
            20,
            "agent/a-very-long-branch-name".to_owned(),
            theme::color(Token::TextMuted),
        );
        let row: String = spans.iter().map(|span| span.content.as_ref()).collect();
        assert!(
            row.ends_with("… "),
            "the caption says it was shortened: {row}"
        );
        assert!(
            Span::raw(&row).width() <= 20,
            "and the row still fits the column: {row}"
        );

        let mut spans = vec![Span::raw("▾ Git")];
        crate::ui::widget::row::push_trailing(
            &mut spans,
            20,
            "main".to_owned(),
            theme::color(Token::TextMuted),
        );
        let row: String = spans.iter().map(|span| span.content.as_ref()).collect();
        assert!(
            row.contains("main"),
            "a caption that fits is left alone: {row}"
        );
        assert!(!row.contains('…'), "{row}");
    }
}
