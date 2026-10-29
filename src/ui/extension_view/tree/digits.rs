//! A `Digits` node: a figure five rows tall, in a block font the host
//! keeps, inked with the theme's fullest chart level so the operator's
//! glyph set draws it too.

use super::*;

/// How many rows a figure in the block font takes.
pub(super) const ROWS: usize = 5;

/// The cells between one glyph and the next.
const GLYPH_GAP: usize = 1;

/// Each character the font draws, a row at a time, `#` where it is
/// inked.
fn glyph(ch: char) -> Option<[&'static str; ROWS]> {
    Some(match ch {
        '0' => ["###", "# #", "# #", "# #", "###"],
        '1' => [" # ", "## ", " # ", " # ", "###"],
        '2' => ["###", "  #", "###", "#  ", "###"],
        '3' => ["###", "  #", "###", "  #", "###"],
        '4' => ["# #", "# #", "###", "  #", "  #"],
        '5' => ["###", "#  ", "###", "  #", "###"],
        '6' => ["###", "#  ", "###", "# #", "###"],
        '7' => ["###", "  #", "  #", "  #", "  #"],
        '8' => ["###", "# #", "###", "# #", "###"],
        '9' => ["###", "# #", "###", "  #", "###"],
        ':' => [" ", "#", " ", "#", " "],
        '.' => [" ", " ", " ", " ", "#"],
        '-' => ["   ", "   ", "###", "   ", "   "],
        ' ' => [" ", " ", " ", " ", " "],
        _ => return None,
    })
}

/// The figure's glyphs, or `None` when the font cannot draw all of it:
/// such a figure is drawn as one line of text instead.
fn glyphs(figure: &str) -> Option<Vec<[&'static str; ROWS]>> {
    figure.chars().map(glyph).collect()
}

/// The rows the figure takes: five in the font, one as text, none when
/// there is nothing to say.
pub(super) fn height(figure: &str) -> usize {
    match (figure.is_empty(), glyphs(figure)) {
        (true, _) => 0,
        (false, Some(_)) => ROWS,
        (false, None) => 1,
    }
}

/// The cells the figure takes, in the font or as text.
pub(super) fn width(figure: &str) -> u16 {
    match glyphs(figure) {
        Some(glyphs) => cells_of(
            glyphs
                .iter()
                .map(|glyph| glyph[0].len() + GLYPH_GAP)
                .sum::<usize>()
                .saturating_sub(GLYPH_GAP),
        ),
        None => cells_of(text::columns(figure)),
    }
}

/// The figure's rows, top first, cut at `width`.
pub(super) fn rows(figure: &str, role: Role, width: u16) -> Vec<Line<'static>> {
    let ink = Style::default().fg(color(role));
    if figure.is_empty() {
        return Vec::new();
    }
    let Some(glyphs) = glyphs(figure) else {
        return vec![Line::from(TextSpan::styled(
            text::elide(figure, usize::from(width)),
            ink,
        ))];
    };
    let block = theme::frames(Symbol::ChartLevel)
        .last()
        .cloned()
        .unwrap_or_else(|| "#".to_owned());
    let gap = " ".repeat(GLYPH_GAP);
    (0..ROWS)
        .map(|row| {
            let drawn = glyphs
                .iter()
                .map(|glyph| glyph[row].replace('#', &block))
                .collect::<Vec<_>>()
                .join(&gap);
            let cut: String = drawn.chars().take(usize::from(width)).collect();
            Line::from(TextSpan::styled(cut, ink))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(figure: &str, width: u16) -> Vec<String> {
        uze_theme::with_active(uze_theme::default_theme().clone(), || {
            rows(figure, Role::Info, width)
                .iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect()
                })
                .collect()
        })
    }

    #[test]
    fn a_clock_is_drawn_five_rows_tall_in_blocks_with_its_colon() {
        assert_eq!(
            drawn("25:00", 40),
            [
                "███ ███   ███ ███",
                "  █ █   █ █ █ █ █",
                "███ ███   █ █ █ █",
                "█     █ █ █ █ █ █",
                "███ ███   ███ ███",
            ]
        );
        assert_eq!(width("25:00"), 17);
        assert_eq!(height("25:00"), 5);
    }

    #[test]
    fn a_figure_the_font_cannot_draw_is_one_line_of_text() {
        assert_eq!(drawn("12 min", 40), ["12 min"]);
        assert_eq!((width("12 min"), height("12 min")), (6, 1));
        assert_eq!(height(""), 0);
        assert!(drawn("", 40).is_empty());
    }

    #[test]
    fn a_figure_wider_than_its_room_is_cut_at_its_end() {
        assert_eq!(drawn("88", 5)[0], "███ █");
    }
}
