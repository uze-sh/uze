//! A grid of cells a diagram is painted onto, and the glyphs it is
//! painted with.
//!
//! A cell is the only unit every terminal agrees on: it survives SSH, a
//! multiplexer and a terminal with no graphics protocol, and it is what
//! the host already knows how to colour. Lines are kept as *which sides
//! of the cell they leave by* until the very end, so two lines meeting
//! become a junction rather than whichever was drawn last.

use unicode_width::UnicodeWidthChar;

use crate::view::{ContentLine, LineTone, Role, Span};

/// How a line is drawn. A property of the line rather than of whatever
/// the line stands for, which is why it lives beside the drawing and not
/// beside the model.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Stroke {
    #[default]
    Solid,
    Dotted,
    Thick,
}

pub const NORTH: u8 = 1;
pub const EAST: u8 = 2;
pub const SOUTH: u8 = 4;
pub const WEST: u8 = 8;

/// The second half of a double-width glyph: occupied, and never emitted.
const CONTINUATION: char = '\0';

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Glyphs {
    #[default]
    Unicode,
    Ascii,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Corners {
    Square,
    Rounded,
    Marked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub glyph: char,
    pub role: Role,
    /// The role tinting this cell's ground — see [`Span::ground`]. Set
    /// over a region rather than per glyph, because what it marks is an
    /// area, and the glyphs inside it keep saying whatever they said.
    pub ground: Option<Role>,
    pub bold: bool,
    /// Written to, even if with a space: the inside of a box and the gap
    /// between two words are the drawing's, not the board showing through.
    pub solid: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            glyph: ' ',
            role: Role::Default,
            ground: None,
            bold: false,
            solid: false,
        }
    }
}

pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub glyphs: Glyphs,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(width: i32, height: i32, glyphs: Glyphs) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            glyphs,
            cells: vec![Cell::default(); (width * height) as usize],
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height)
            .then(|| (y * self.width + x) as usize)
    }

    pub fn put(&mut self, x: i32, y: i32, glyph: char, role: Role, bold: bool) {
        if let Some(index) = self.index(x, y) {
            // The ground survives what is written over it: a region is
            // marked once and then drawn into, not the other way round.
            let ground = self.cells[index].ground;
            self.cells[index] = Cell {
                glyph,
                role,
                ground,
                bold,
                solid: true,
            };
        }
    }

    pub fn is_blank(&self, x: i32, y: i32) -> bool {
        self.index(x, y)
            .is_some_and(|index| !self.cells[index].solid)
    }

    /// Writes `text` from `x`, one cell per column it occupies, and
    /// answers the column after it.
    pub fn text(&mut self, x: i32, y: i32, text: &str, role: Role, bold: bool) -> i32 {
        let mut column = x;
        for glyph in text.chars() {
            let width = glyph.width().unwrap_or(0) as i32;
            if width == 0 {
                continue;
            }
            self.put(column, y, glyph, role, bold);
            if width == 2 {
                self.put(column + 1, y, CONTINUATION, role, bold);
            }
            column += width;
        }
        column
    }

    pub fn line(&mut self, x: i32, y: i32, sides: u8, stroke: Stroke, role: Role) {
        let glyph = line_glyph(sides, stroke, self.glyphs, Corners::Rounded);
        self.put(x, y, glyph, role, false);
    }

    pub fn frame(&mut self, frame: Frame, corners: Corners, role: Role) {
        let Frame { x, y, w, h } = frame;
        let (right, bottom) = (x + w - 1, y + h - 1);
        for column in x + 1..right {
            for row in [y, bottom] {
                let glyph = line_glyph(EAST | WEST, Stroke::Solid, self.glyphs, corners);
                self.put(column, row, glyph, role, false);
            }
        }
        for row in y + 1..bottom {
            for column in [x, right] {
                let glyph = line_glyph(NORTH | SOUTH, Stroke::Solid, self.glyphs, corners);
                self.put(column, row, glyph, role, false);
            }
        }
        for (column, row, sides) in [
            (x, y, EAST | SOUTH),
            (right, y, SOUTH | WEST),
            (x, bottom, NORTH | EAST),
            (right, bottom, NORTH | WEST),
        ] {
            let glyph = line_glyph(sides, Stroke::Solid, self.glyphs, corners);
            self.put(column, row, glyph, role, false);
        }
    }

    /// Tints every cell of `frame` with `role`'s hue, leaving what is
    /// drawn there alone.
    ///
    /// The way an area is marked when recolouring it would cost the
    /// reader something: on the map every tile's colour already says how
    /// hot it is, so a selection that repainted it would answer "which is
    /// selected" by erasing "how hot is it".
    pub fn ground(&mut self, frame: Frame, role: Role) {
        for y in frame.y..frame.y + frame.h {
            for x in frame.x..frame.x + frame.w {
                if let Some(index) = self.index(x, y) {
                    self.cells[index].ground = Some(role);
                    self.cells[index].solid = true;
                }
            }
        }
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<Cell> {
        self.index(x, y).map(|index| self.cells[index])
    }

    pub fn set(&mut self, x: i32, y: i32, cell: Cell) {
        if let Some(index) = self.index(x, y) {
            self.cells[index] = cell;
        }
    }

    /// Every row, as lines the host can draw as they are.
    pub fn lines(&self) -> Vec<ContentLine> {
        (0..self.height)
            .map(|row| ContentLine {
                gutter: String::new(),
                number: String::new(),
                tone: LineTone::Neutral,
                spans: self.spans(row, 0, self.width),
                marked: false,
            })
            .collect()
    }

    fn spans(&self, row: i32, pan: i32, columns: i32) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        for column in pan.max(0)..(pan + columns).min(self.width) {
            let Some(index) = self.index(column, row) else {
                continue;
            };
            let cell = self.cells[index];
            if cell.glyph == CONTINUATION {
                continue;
            }
            match spans.last_mut() {
                Some(last)
                    if last.role == cell.role
                        && last.bold == cell.bold
                        && last.ground == cell.ground =>
                {
                    last.text.push(cell.glyph);
                }
                _ => {
                    let mut span = Span::new(cell.glyph.to_string(), cell.role);
                    span.bold = cell.bold;
                    span.ground = cell.ground;
                    spans.push(span);
                }
            }
        }
        if let Some(last) = spans.last_mut() {
            let trimmed = last.text.trim_end().len();
            last.text.truncate(trimmed);
        }
        spans
    }

    #[cfg(test)]
    pub fn to_text(&self) -> String {
        (0..self.height)
            .map(|row| {
                self.spans(row, 0, self.width)
                    .into_iter()
                    .map(|span| span.text)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Frame {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

/// What a box leads to, written into its border: further into the
/// drawing, or out of it to the code it stands for.
pub fn leads_glyph(out_of_the_board: bool, glyphs: Glyphs) -> &'static str {
    match (glyphs, out_of_the_board) {
        (Glyphs::Unicode, false) => "»",
        (Glyphs::Unicode, true) => "↗",
        (Glyphs::Ascii, false) => ">>",
        (Glyphs::Ascii, true) => "^",
    }
}

/// The mark on something that differs from what was last committed.
pub fn changed_glyph(glyphs: Glyphs) -> char {
    match glyphs {
        Glyphs::Unicode => '●',
        Glyphs::Ascii => '*',
    }
}

/// `text` cut to `width` columns, saying so when it was cut.
pub fn fitted(text: &str, width: i32, glyphs: Glyphs) -> String {
    if text_width(text) <= width {
        return text.to_owned();
    }
    let cut = match glyphs {
        Glyphs::Unicode => '…',
        Glyphs::Ascii => '~',
    };
    let mut fitted = String::new();
    let mut used = 1;
    for glyph in text.chars() {
        used += glyph.width().unwrap_or(0) as i32;
        if used > width {
            break;
        }
        fitted.push(glyph);
    }
    if width >= 1 {
        fitted.push(cut);
    }
    fitted
}

pub fn arrow_glyph(heading: u8, glyphs: Glyphs) -> char {
    match (glyphs, heading) {
        (Glyphs::Unicode, NORTH) => '▲',
        (Glyphs::Unicode, EAST) => '►',
        (Glyphs::Unicode, SOUTH) => '▼',
        (Glyphs::Unicode, _) => '◄',
        (Glyphs::Ascii, NORTH) => '^',
        (Glyphs::Ascii, EAST) => '>',
        (Glyphs::Ascii, SOUTH) => 'v',
        (Glyphs::Ascii, _) => '<',
    }
}

/// The glyph for a line leaving a cell by `sides`. Only a straight run
/// carries its stroke: box-drawing has no dotted corner, and a junction
/// drawn half-heavy reads as a rendering fault rather than as emphasis.
pub fn line_glyph(sides: u8, stroke: Stroke, glyphs: Glyphs, corners: Corners) -> char {
    let vertical = sides & (EAST | WEST) == 0;
    let horizontal = sides & (NORTH | SOUTH) == 0;
    if sides == 0 {
        return ' ';
    }
    if glyphs == Glyphs::Ascii {
        return match (vertical, horizontal, stroke) {
            (true, _, Stroke::Dotted) => ':',
            (true, _, _) => '|',
            (_, true, Stroke::Dotted) => '.',
            (_, true, Stroke::Thick) => '=',
            (_, true, Stroke::Solid) => '-',
            _ if corners == Corners::Marked => '*',
            _ => '+',
        };
    }
    if vertical {
        return match stroke {
            Stroke::Solid => '│',
            Stroke::Dotted => '┆',
            Stroke::Thick => '┃',
        };
    }
    if horizontal {
        return match stroke {
            Stroke::Solid => '─',
            Stroke::Dotted => '┄',
            Stroke::Thick => '━',
        };
    }
    let corner = |square: char, rounded: char| match corners {
        Corners::Square => square,
        Corners::Rounded => rounded,
        Corners::Marked => '◇',
    };
    match sides {
        s if s == NORTH | EAST => corner('└', '╰'),
        s if s == EAST | SOUTH => corner('┌', '╭'),
        s if s == SOUTH | WEST => corner('┐', '╮'),
        s if s == NORTH | WEST => corner('┘', '╯'),
        s if s == NORTH | EAST | SOUTH => '├',
        s if s == NORTH | SOUTH | WEST => '┤',
        s if s == EAST | SOUTH | WEST => '┬',
        s if s == NORTH | EAST | WEST => '┴',
        _ => '┼',
    }
}

pub fn text_width(text: &str) -> i32 {
    unicode_width::UnicodeWidthStr::width(text) as i32
}

/// Greedy word wrap to `width` columns; a word longer than that keeps a
/// line to itself rather than being cut.
pub fn wrapped(text: &str, width: i32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if text_width(line) + 1 + text_width(word) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_blank_until_something_is_written_to_it() {
        let mut canvas = Canvas::new(3, 1, Glyphs::Unicode);
        assert!(canvas.is_blank(1, 0));
        canvas.put(1, 0, ' ', Role::Default, false);
        assert!(!canvas.is_blank(1, 0), "a space written is the drawing's");
        assert!(
            !canvas.is_blank(3, 0),
            "outside the board is not a free cell"
        );
    }

    /// A double-width glyph occupies the cell after it, and what follows
    /// is written past both.
    #[test]
    fn text_takes_one_cell_per_column_it_occupies() {
        let mut canvas = Canvas::new(8, 1, Glyphs::Unicode);
        let after = canvas.text(2, 0, "\u{6f22}a", Role::Default, false);
        assert_eq!(after, 5);
        assert_eq!(canvas.to_text(), "  \u{6f22}a");
        assert!(canvas.is_blank(1, 0), "nothing is written before the text");
        assert!(!canvas.is_blank(3, 0), "the wide glyph's second column");
        assert_eq!(canvas.cell(4, 0).map(|cell| cell.glyph), Some('a'));
    }

    #[test]
    fn a_cell_reads_back_what_was_written_and_nothing_outside_the_board() {
        let mut canvas = Canvas::new(2, 2, Glyphs::Unicode);
        canvas.put(1, 1, 'x', Role::Accent, true);
        let written = canvas.cell(1, 1).expect("inside the board");
        assert_eq!(
            (written.glyph, written.role, written.bold),
            ('x', Role::Accent, true)
        );
        assert_eq!(canvas.cell(2, 1), None);
        let replaced = Cell {
            glyph: 'y',
            role: Role::Muted,
            ground: Some(Role::Info),
            bold: false,
            solid: true,
        };
        canvas.set(0, 1, replaced);
        assert_eq!(canvas.cell(0, 1), Some(replaced));
    }

    #[test]
    fn a_line_turns_a_corner_rounded() {
        let mut canvas = Canvas::new(1, 1, Glyphs::Unicode);
        canvas.line(0, 0, EAST | SOUTH, Stroke::Solid, Role::Default);
        assert_eq!(canvas.cell(0, 0).map(|cell| cell.glyph), Some('\u{256d}'));
    }

    /// The frame's border is exactly its rectangle: nothing drawn outside
    /// it, every side and corner inside it.
    #[test]
    fn a_frame_borders_exactly_its_rectangle() {
        let mut canvas = Canvas::new(7, 6, Glyphs::Unicode);
        let frame = Frame {
            x: 1,
            y: 1,
            w: 4,
            h: 3,
        };
        canvas.frame(frame, Corners::Square, Role::Faint);
        assert_eq!(
            canvas.to_text(),
            "\n \u{250c}\u{2500}\u{2500}\u{2510}\n \u{2502}  \u{2502}\n \u{2514}\u{2500}\u{2500}\u{2518}\n\n"
        );
        let mut ascii = Canvas::new(7, 6, Glyphs::Ascii);
        ascii.frame(frame, Corners::Rounded, Role::Faint);
        assert_eq!(ascii.to_text(), "\n +--+\n |  |\n +--+\n\n");
    }

    /// A row is said in as few spans as its look allows, and no fewer.
    #[test]
    fn a_row_changes_span_wherever_its_look_changes() {
        let mut canvas = Canvas::new(6, 1, Glyphs::Unicode);
        canvas.text(0, 0, "ab", Role::Accent, false);
        canvas.text(2, 0, "cd", Role::Muted, false);
        canvas.text(4, 0, "ef", Role::Muted, true);
        let spans = &canvas.lines()[0].spans;
        let said: Vec<(&str, Role, bool)> = spans
            .iter()
            .map(|span| (span.text.as_str(), span.role, span.bold))
            .collect();
        assert_eq!(
            said,
            [
                ("ab", Role::Accent, false),
                ("cd", Role::Muted, false),
                ("ef", Role::Muted, true),
            ]
        );
    }

    #[test]
    fn a_frame_contains_its_cells_and_not_the_ones_past_its_edges() {
        let frame = Frame {
            x: 1,
            y: 2,
            w: 4,
            h: 6,
        };
        assert!(frame.contains(1, 2));
        assert!(frame.contains(4, 7), "its last cell");
        assert!(!frame.contains(5, 2), "one past its right edge");
        assert!(!frame.contains(1, 8), "one past its bottom edge");
        assert_eq!(frame.center(), (3, 5));
    }

    /// Further into the drawing and out of it are two marks, each said in
    /// the glyph set the board is drawn in.
    #[test]
    fn the_two_leads_are_told_apart_in_either_glyph_set() {
        for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
            let inward = leads_glyph(false, glyphs);
            let outward = leads_glyph(true, glyphs);
            assert!(!inward.is_empty() && !outward.is_empty(), "{glyphs:?}");
            assert_ne!(inward, outward, "{glyphs:?}");
            assert!(
                text_width(inward) <= 2 && text_width(outward) <= 2,
                "{glyphs:?}"
            );
        }
        assert!(leads_glyph(false, Glyphs::Ascii).is_ascii());
        assert!(leads_glyph(true, Glyphs::Ascii).is_ascii());
    }

    #[test]
    fn text_that_fits_is_kept_whole() {
        assert_eq!(fitted("abcd", 4, Glyphs::Unicode), "abcd");
    }

    /// The cut mark takes a column of the width, so the cut text and its
    /// mark together fill it and no more.
    #[test]
    fn text_too_wide_is_cut_to_the_width_and_says_so() {
        assert_eq!(fitted("abcdef", 4, Glyphs::Unicode), "abc\u{2026}");
        assert_eq!(fitted("abcdef", 4, Glyphs::Ascii), "abc~");
        assert_eq!(
            fitted("\u{6f22}\u{5b57}x", 4, Glyphs::Unicode),
            "\u{6f22}\u{2026}"
        );
        assert_eq!(fitted("abcdef", 1, Glyphs::Unicode), "\u{2026}");
        assert_eq!(fitted("abcdef", 0, Glyphs::Unicode), "");
    }

    /// Each box-drawing glyph, by the sides Unicode names it leaving by.
    const BOX_DRAWING: &[(u8, char)] = &[
        (NORTH | SOUTH, '\u{2502}'),
        (EAST | WEST, '\u{2500}'),
        (NORTH | EAST | SOUTH, '\u{251c}'),
        (NORTH | SOUTH | WEST, '\u{2524}'),
        (EAST | SOUTH | WEST, '\u{252c}'),
        (NORTH | EAST | WEST, '\u{2534}'),
        (NORTH | EAST | SOUTH | WEST, '\u{253c}'),
    ];

    #[test]
    fn a_line_leaving_by_some_sides_is_drawn_with_the_glyph_that_leaves_by_them() {
        for &(sides, glyph) in BOX_DRAWING {
            for corners in [Corners::Square, Corners::Rounded, Corners::Marked] {
                let drawn = line_glyph(sides, Stroke::Solid, Glyphs::Unicode, corners);
                assert_eq!(drawn, glyph, "sides {sides:04b}, {corners:?}");
            }
        }
        assert_eq!(
            line_glyph(0, Stroke::Solid, Glyphs::Unicode, Corners::Square),
            ' '
        );
    }

    #[test]
    fn a_corner_is_drawn_square_rounded_or_marked() {
        let corners: &[(u8, char, char)] = &[
            (NORTH | EAST, '\u{2514}', '\u{2570}'),
            (EAST | SOUTH, '\u{250c}', '\u{256d}'),
            (SOUTH | WEST, '\u{2510}', '\u{256e}'),
            (NORTH | WEST, '\u{2518}', '\u{256f}'),
        ];
        for &(sides, square, rounded) in corners {
            let drawn = |corners| line_glyph(sides, Stroke::Solid, Glyphs::Unicode, corners);
            assert_eq!(drawn(Corners::Square), square, "sides {sides:04b}");
            assert_eq!(drawn(Corners::Rounded), rounded, "sides {sides:04b}");
            assert_eq!(drawn(Corners::Marked), '\u{25c7}', "sides {sides:04b}");
        }
    }

    /// Only a straight run carries its stroke: a corner or a junction is
    /// drawn the same whatever the line's stroke.
    #[test]
    fn only_a_straight_run_carries_its_stroke() {
        let unicode = |sides, stroke| line_glyph(sides, stroke, Glyphs::Unicode, Corners::Square);
        assert_eq!(unicode(NORTH | SOUTH, Stroke::Dotted), '\u{2506}');
        assert_eq!(unicode(NORTH | SOUTH, Stroke::Thick), '\u{2503}');
        assert_eq!(unicode(EAST | WEST, Stroke::Dotted), '\u{2504}');
        assert_eq!(unicode(EAST | WEST, Stroke::Thick), '\u{2501}');
        for sides in [EAST | SOUTH, NORTH | EAST | SOUTH] {
            for stroke in [Stroke::Dotted, Stroke::Thick] {
                assert_eq!(unicode(sides, stroke), unicode(sides, Stroke::Solid));
            }
        }
    }

    #[test]
    fn the_ascii_set_draws_runs_by_stroke_and_every_turn_as_a_junction() {
        let ascii = |sides, stroke, corners| line_glyph(sides, stroke, Glyphs::Ascii, corners);
        let square = Corners::Square;
        assert_eq!(ascii(NORTH | SOUTH, Stroke::Solid, square), '|');
        assert_eq!(ascii(NORTH | SOUTH, Stroke::Thick, square), '|');
        assert_eq!(ascii(NORTH | SOUTH, Stroke::Dotted, square), ':');
        assert_eq!(ascii(EAST | WEST, Stroke::Solid, square), '-');
        assert_eq!(ascii(EAST | WEST, Stroke::Thick, square), '=');
        assert_eq!(ascii(EAST | WEST, Stroke::Dotted, square), '.');
        for sides in [EAST | SOUTH, NORTH | EAST | SOUTH | WEST] {
            assert_eq!(ascii(sides, Stroke::Dotted, Corners::Rounded), '+');
            assert_eq!(ascii(sides, Stroke::Solid, Corners::Marked), '*');
        }
    }

    #[test]
    fn words_wrap_at_the_width_counting_the_space_between_them() {
        assert_eq!(wrapped("ab cd", 5), ["ab cd"]);
        assert_eq!(wrapped("ab cd", 4), ["ab", "cd"]);
        assert_eq!(wrapped("one two three", 7), ["one two", "three"]);
        assert!(wrapped("   ", 4).is_empty());
    }

    #[test]
    fn a_word_wider_than_the_line_keeps_a_line_to_itself() {
        assert_eq!(wrapped("a enormous b", 3), ["a", "enormous", "b"]);
    }
}
