//! A sequence diagram, painted.
//!
//! The one notation here that needs no layout search: participants are
//! columns, time is rows, and the only decision is how far apart the
//! columns must stand for the longest message between them to fit.

use crate::view::Role;

use crate::shared::canvas::{
    Canvas, Corners, EAST, Frame, Glyphs, NORTH, SOUTH, WEST, arrow_glyph, text_width,
};

use super::model::{Sequence, SequenceStep, Stroke};

const MARGIN: i32 = 2;
const HEAD_HEIGHT: i32 = 3;
const LOOP_WIDTH: i32 = 4;

pub fn paint(sequence: &Sequence, glyphs: Glyphs) -> Canvas {
    let centres = centres(sequence);
    let head = |index: usize| text_width(&sequence.participants[index].title) + 4;
    let reach = sequence
        .steps
        .iter()
        .map(|step| match step {
            SequenceStep::Message { from, to, text, .. } if from == to => {
                centres[*from] + LOOP_WIDTH + 2 + text_width(text)
            }
            SequenceStep::Note { over, text } => centres[*over] + text_width(text) / 2 + 3,
            _ => 0,
        })
        .chain((0..centres.len()).map(|index| centres[index] + head(index) / 2 + 1))
        .max()
        .unwrap_or(0);
    let rows: i32 = sequence.steps.iter().map(step_height).sum();
    let width = reach + MARGIN;
    let mut canvas = Canvas::new(width, HEAD_HEIGHT + rows + 2, glyphs);

    for (index, participant) in sequence.participants.iter().enumerate() {
        let w = head(index);
        let frame = Frame {
            x: centres[index] - w / 2,
            y: 0,
            w,
            h: HEAD_HEIGHT,
        };
        canvas.frame(frame, Corners::Rounded, Role::Muted);
        canvas.text(frame.x + 2, 1, &participant.title, Role::Bright, true);
        for y in HEAD_HEIGHT..canvas.height {
            canvas.line(
                centres[index],
                y,
                NORTH | SOUTH,
                Stroke::Dotted,
                Role::Faint,
            );
        }
    }

    let mut y = HEAD_HEIGHT + 1;
    for step in &sequence.steps {
        match step {
            SequenceStep::Message {
                from,
                to,
                text,
                stroke,
            } if from == to => self_message(&mut canvas, centres[*from], y, text, *stroke),
            SequenceStep::Message {
                from,
                to,
                text,
                stroke,
            } => message(&mut canvas, centres[*from], centres[*to], y, text, *stroke),
            SequenceStep::Note { over, text } => {
                let w = text_width(text) + 4;
                let frame = Frame {
                    x: (centres[*over] - w / 2).max(0),
                    y,
                    w,
                    h: 3,
                };
                canvas.frame(frame, Corners::Square, Role::Faint);
                canvas.text(frame.x + 1, y + 1, &format!(" {text} "), Role::Muted, false);
            }
            SequenceStep::Divider(text) => {
                for x in MARGIN..width - MARGIN {
                    canvas.line(x, y, EAST | WEST, Stroke::Dotted, Role::Faint);
                }
                canvas.text(MARGIN + 2, y, &format!(" {text} "), Role::Secondary, false);
            }
        }
        y += step_height(step);
    }
    canvas
}

fn step_height(step: &SequenceStep) -> i32 {
    match step {
        SequenceStep::Message { from, to, .. } if from == to => 4,
        SequenceStep::Message { .. } => 3,
        SequenceStep::Note { .. } => 4,
        SequenceStep::Divider(_) => 2,
    }
}

/// Where each lifeline stands: packed by the participants' own widths,
/// then pushed apart wherever a message between two of them needs more.
fn centres(sequence: &Sequence) -> Vec<i32> {
    let count = sequence.participants.len();
    let half = |index: usize| (text_width(&sequence.participants[index].title) + 4) / 2;
    let mut gaps: Vec<i32> = (1..count)
        .map(|index| half(index - 1) + half(index) + 4)
        .collect();
    for step in &sequence.steps {
        let SequenceStep::Message { from, to, text, .. } = step else {
            continue;
        };
        let (low, high) = (*from.min(to), *from.max(to));
        let needed = if low == high {
            LOOP_WIDTH + 4 + text_width(text)
        } else {
            text_width(text) + 6
        };
        if low == high {
            if let Some(gap) = gaps.get_mut(low) {
                *gap = (*gap).max(needed);
            }
            continue;
        }
        let spanned: i32 = gaps[low..high].iter().sum();
        if spanned < needed {
            gaps[high - 1] += needed - spanned;
        }
    }
    let mut centre = MARGIN + if count > 0 { half(0) } else { 0 };
    let mut centres = Vec::with_capacity(count);
    for index in 0..count {
        centres.push(centre);
        centre += gaps.get(index).copied().unwrap_or(0);
    }
    centres
}

fn message(canvas: &mut Canvas, from: i32, to: i32, y: i32, text: &str, stroke: Stroke) {
    let (left, right) = (from.min(to), from.max(to));
    // Padded, so a lifeline the label passes over is cleared on both
    // sides of it rather than butting into its first letter.
    let label = left + (right - left - text_width(text)) / 2;
    canvas.text(label, y, &format!(" {text} "), Role::Secondary, false);
    for x in left + 1..right {
        canvas.line(x, y + 1, EAST | WEST, stroke, Role::Dim);
    }
    let (tip, heading, stem) = if to > from {
        (to - 1, EAST, NORTH | SOUTH | EAST)
    } else {
        (to + 1, WEST, NORTH | SOUTH | WEST)
    };
    canvas.line(from, y + 1, stem, Stroke::Solid, Role::Dim);
    canvas.put(
        tip,
        y + 1,
        arrow_glyph(heading, canvas.glyphs),
        Role::Muted,
        false,
    );
}

fn self_message(canvas: &mut Canvas, at: i32, y: i32, text: &str, stroke: Stroke) {
    let far = at + LOOP_WIDTH;
    canvas.line(at, y, NORTH | SOUTH | EAST, Stroke::Solid, Role::Dim);
    for x in at + 1..far {
        canvas.line(x, y, EAST | WEST, stroke, Role::Dim);
        canvas.line(x, y + 2, EAST | WEST, stroke, Role::Dim);
    }
    canvas.line(far, y, SOUTH | WEST, stroke, Role::Dim);
    canvas.line(far, y + 1, NORTH | SOUTH, stroke, Role::Dim);
    canvas.line(far, y + 2, NORTH | WEST, stroke, Role::Dim);
    canvas.put(
        at + 1,
        y + 2,
        arrow_glyph(WEST, canvas.glyphs),
        Role::Muted,
        false,
    );
    canvas.text(far + 2, y + 1, text, Role::Secondary, false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architect::{mermaid, model::Diagram};

    fn drawn(source: &str) -> Canvas {
        match mermaid::parse(source).expect("the diagram parses") {
            Diagram::Sequence(sequence) => paint(&sequence, Glyphs::Unicode),
            Diagram::Graph(_) => panic!("expected a sequence"),
        }
    }

    /// The drawing as rows of glyphs, every column kept.
    fn grid(canvas: &Canvas) -> Vec<Vec<char>> {
        (0..canvas.height)
            .map(|y| {
                (0..canvas.width)
                    .map(|x| canvas.cell(x, y).map_or(' ', |cell| cell.glyph))
                    .collect()
            })
            .collect()
    }

    fn row_text(grid: &[Vec<char>], y: usize) -> String {
        grid[y].iter().collect()
    }

    /// Where `text` first appears, as (column, row).
    fn find(grid: &[Vec<char>], text: &str) -> (usize, usize) {
        let wanted: Vec<char> = text.chars().collect();
        for (y, row) in grid.iter().enumerate() {
            if let Some(x) = row
                .windows(wanted.len())
                .position(|w| w == wanted.as_slice())
            {
                return (x, y);
            }
        }
        panic!("`{text}` is not drawn");
    }

    /// The column a participant's lifeline runs down: the middle of the
    /// head its title is framed in.
    fn lifeline(grid: &[Vec<char>], title: &str) -> usize {
        let framed = format!("│ {title} │");
        let (x, y) = find(grid, &framed);
        assert_eq!(y, 1, "a head is on top");
        x + framed.chars().count() / 2
    }

    const TWO: &str = "sequenceDiagram\n participant A as Alice\n participant B as Bob\n";

    #[test]
    fn every_participant_heads_a_column_and_its_lifeline_runs_to_the_foot() {
        let canvas = drawn(&format!("{TWO} A->>B: hello"));
        let grid = grid(&canvas);
        for title in ["Alice", "Bob"] {
            let framed = format!("│ {title} │");
            let (left, _) = find(&grid, &framed);
            let right = left + framed.chars().count() - 1;
            assert_eq!(grid[0][left], '╭', "{title}");
            assert_eq!(grid[0][right], '╮', "{title}");
            assert_eq!(grid[2][left], '╰', "{title}");
            assert_eq!(grid[2][right], '╯', "{title}");
            let column = lifeline(&grid, title);
            assert_eq!(grid[3][column], '┆', "{title}: under the head");
            assert_eq!(grid[grid.len() - 1][column], '┆', "{title}: at the foot");
        }
        let (alice, bob) = (lifeline(&grid, "Alice"), lifeline(&grid, "Bob"));
        assert!(alice < bob, "columns in the order declared");
        assert!(
            find(&grid, "│ Alice │").0 + 9 < find(&grid, "│ Bob │").0,
            "heads stand apart"
        );
    }

    #[test]
    fn a_message_runs_from_its_senders_lifeline_to_a_head_beside_its_receivers() {
        let grid = grid(&drawn(&format!("{TWO} A->>B: hello\n B-->>A: back")));
        let (alice, bob) = (lifeline(&grid, "Alice"), lifeline(&grid, "Bob"));
        let (_, out) = find(&grid, "hello");
        let arrow = &grid[out + 1];
        assert_eq!(arrow[alice], '├', "{}", row_text(&grid, out + 1));
        assert!(arrow[alice + 1..bob - 1].iter().all(|&c| c == '─'));
        assert_eq!(arrow[bob - 1], '►');
        assert_eq!(arrow[bob], '┆', "the receiver's lifeline goes on");

        let (_, back) = find(&grid, "back");
        assert!(back > out + 1, "time runs down the page");
        let arrow = &grid[back + 1];
        assert_eq!(arrow[bob], '┤', "{}", row_text(&grid, back + 1));
        assert!(
            arrow[alice + 2..bob].iter().all(|&c| c == '┄'),
            "a reply is dotted"
        );
        assert_eq!(arrow[alice + 1], '◄');
        assert_eq!(arrow[alice], '┆');
    }

    #[test]
    fn a_message_label_sits_over_its_line_between_the_two_lifelines() {
        let label = "a long message between the two of them";
        let grid = grid(&drawn(&format!("{TWO} A->>B: {label}")));
        let (alice, bob) = (lifeline(&grid, "Alice"), lifeline(&grid, "Bob"));
        let (x, y) = find(&grid, label);
        let end = x + label.chars().count();
        assert!(alice + 1 < x && end < bob, "{}", row_text(&grid, y));
        assert!(
            (x - alice - 1).abs_diff(bob - end) <= 1,
            "centred over the line: {}",
            row_text(&grid, y)
        );
    }

    #[test]
    fn columns_part_wherever_a_message_needs_the_room() {
        let three = "sequenceDiagram\n participant A as Ann\n participant B as Bob\n participant C as Cid\n";
        let label = "a message far longer than any head";
        for (source, from, to) in [
            (format!("{three} A->>B: {label}"), "Ann", "Bob"),
            (format!("{three} B->>C: {label}"), "Bob", "Cid"),
            (format!("{three} A->>C: {label}"), "Ann", "Cid"),
            (format!("{three} C->>A: {label}"), "Ann", "Cid"),
        ] {
            let grid = grid(&drawn(&source));
            let (left, right) = (lifeline(&grid, from), lifeline(&grid, to));
            let (x, _) = find(&grid, label);
            assert!(left < x && x + label.chars().count() < right, "{source}");
        }
        let grid = grid(&drawn(&format!("{three} A->>B: hi")));
        let (ann, bob, cid) = (
            lifeline(&grid, "Ann"),
            lifeline(&grid, "Bob"),
            lifeline(&grid, "Cid"),
        );
        assert_eq!(
            bob - ann,
            cid - bob,
            "a short message leaves the columns packed"
        );
    }

    #[test]
    fn a_message_to_itself_loops_out_and_back_with_its_words_beside_the_loop() {
        let grid = grid(&drawn(&format!("{TWO} A->>A: think\n A->>B: next")));
        let alice = lifeline(&grid, "Alice");
        let (x, y) = find(&grid, "think");
        let top = y - 1;
        assert_eq!(grid[top][alice], '├', "{}", row_text(&grid, top));
        let far = (alice + 1..x)
            .find(|&column| grid[top][column] == '╮')
            .expect("the loop turns down");
        assert!(grid[top][alice + 1..far].iter().all(|&c| c == '─'));
        assert_eq!(grid[y][far], '│');
        assert_eq!(grid[y + 1][far], '╯');
        assert_eq!(
            grid[y + 1][alice + 1],
            '◄',
            "the head points back at the lifeline"
        );
        assert!(grid[y + 1][alice + 2..far].iter().all(|&c| c == '─'));
        assert!(x > far + 1, "the words stand clear of the loop");
        let (_, next) = find(&grid, "next");
        assert!(next > y + 1, "the loop takes its own rows");
        assert!(
            (top..=y + 1).all(|row| grid[row][alice] != ' '),
            "the lifeline is unbroken"
        );
    }

    #[test]
    fn a_reply_to_itself_is_drawn_dotted() {
        let grid = grid(&drawn(&format!("{TWO} A-->>A: wait")));
        let alice = lifeline(&grid, "Alice");
        let (_, y) = find(&grid, "wait");
        assert_eq!(grid[y - 1][alice + 1], '┄', "{}", row_text(&grid, y - 1));
        assert_eq!(grid[y][alice + 4], '┆');
    }

    #[test]
    fn a_note_is_framed_over_its_participants_lifeline() {
        let grid = grid(&drawn(&format!("{TWO} Note over B: a note\n A->>B: after")));
        let bob = lifeline(&grid, "Bob");
        let (x, y) = find(&grid, "│ a note │");
        let right = x + "│ a note │".chars().count() - 1;
        assert!(x < bob && bob < right, "{}", row_text(&grid, y));
        assert!((bob - x).abs_diff(right - bob) <= 1, "centred over it");
        assert_eq!(grid[y - 1][x], '┌');
        assert_eq!(grid[y - 1][right], '┐');
        assert_eq!(grid[y + 1][x], '└');
        assert_eq!(grid[y + 1][right], '┘');
        let (_, after) = find(&grid, "after");
        assert!(after > y + 1, "the next step comes below the note");
    }

    #[test]
    fn a_note_on_the_last_lifeline_is_drawn_whole() {
        let text = "a note wider than any head here";
        let grid = grid(&drawn(&format!(
            "sequenceDiagram\n participant A as A participant with a long title\n \
             participant B as Bob\n Note over B: {text}"
        )));
        let framed = format!("│ {text} │");
        let (x, _) = find(&grid, &framed);
        assert!(x + framed.chars().count() < grid[0].len(), "with a margin");
    }

    #[test]
    fn a_divider_is_a_rule_across_every_lifeline_naming_its_block() {
        let grid = grid(&drawn(&format!("{TWO} loop\n B->>A: tick\n end")));
        let (alice, bob) = (lifeline(&grid, "Alice"), lifeline(&grid, "Bob"));
        let (x, y) = find(&grid, " loop ");
        let (_, tick) = find(&grid, "tick");
        let (_, end) = find(&grid, " end ");
        assert!(
            y < tick && tick < end,
            "the block opens above and closes below"
        );
        for row in [y, end] {
            assert!(
                grid[row][..x.min(alice)]
                    .iter()
                    .all(|&c| c == ' ' || c == '┄')
            );
            assert_eq!(grid[row][bob], '┄', "{}", row_text(&grid, row));
            assert_eq!(grid[row][grid[row].len() - 1], ' ', "short of the edge");
        }
        assert!(x < alice, "named at the left: {}", row_text(&grid, y));
    }

    #[test]
    fn the_drawing_is_wide_enough_for_whatever_reaches_furthest() {
        let words = "a self message with a great many words";
        let grid = grid(&drawn(&format!("{TWO} B->>B: {words}")));
        let (x, _) = find(&grid, words);
        assert!(x + words.chars().count() < grid[0].len(), "with a margin");
        let long = "Participant with a long name";
        let grid = self::grid(&drawn(&format!(
            "sequenceDiagram\n participant A as Al\n participant B as {long}\n"
        )));
        let (x, _) = find(&grid, &format!("│ {long} │"));
        assert!(x + long.chars().count() + 4 < grid[0].len());
    }
}
