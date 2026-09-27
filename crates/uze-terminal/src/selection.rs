//! A pane's text selection, anchored to the content it covers rather than
//! to the screen cells it was drawn over.
//!
//! On the primary screen that is the terminal's own selection: the
//! scrollback holds every line, and the terminal moves the selection with
//! them. On the alternate screen there is no scrollback — a full-screen
//! program scrolls by redrawing — so the content has to be followed from
//! the outside: each row of the screen knows which line of content it
//! shows, a redraw that moves rows is recognised by the rows it moved, and
//! every line the selection has seen is kept, so a copy includes what has
//! since scrolled off. Nothing here knows which program is drawing.

use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use alacritty_terminal::{
    Term,
    event::EventListener,
    grid::Dimensions,
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionRange, SelectionType},
    term::{TermMode, cell::Flags, viewport_to_point},
};

use crate::SelectionGesture;

/// What stands in a row for the cell a wide character spills into: it is
/// part of the screen's geometry, but not of the text.
const SPILL: char = '\0';

#[derive(Debug, Default)]
pub(crate) struct PaneSelection {
    /// Whether the primary-screen selection was drawn backwards, so its
    /// anchor is the end it reads last. The terminal keeps that selection,
    /// and moves it with the lines it covers, but never says which end was
    /// pressed.
    reversed: bool,
    /// The alternate screen's selection, which the terminal cannot keep.
    followed: Option<FollowedSelection>,
}

impl PaneSelection {
    /// Applies `gesture`; whether what the selection covers changed.
    pub(crate) fn apply<T: EventListener>(
        &mut self,
        terminal: &mut Term<T>,
        gesture: SelectionGesture,
    ) -> bool {
        if let SelectionGesture::Begin { anchor, head } = gesture {
            self.followed = None;
            if terminal.mode().contains(TermMode::ALT_SCREEN) {
                terminal.selection = None;
                self.followed = Some(FollowedSelection::begin(
                    screen_rows(terminal),
                    anchor,
                    head,
                ));
                return true;
            }
        }
        let Some(followed) = self.followed.as_mut() else {
            return apply_to_terminal(terminal, &mut self.reversed, gesture);
        };
        match gesture {
            SelectionGesture::Extend { head } => followed.extend(head),
            SelectionGesture::Release => {
                followed.pointer = None;
                false
            }
            SelectionGesture::Begin { .. } | SelectionGesture::Clear => {
                self.followed = None;
                true
            }
        }
    }

    /// Follows the content under an alternate-screen selection after the
    /// program drew. Called on every read of its output while one exists.
    pub(crate) fn observe<T: EventListener>(&mut self, terminal: &Term<T>) {
        let Some(followed) = self.followed.as_mut() else {
            return;
        };
        let still_followed = terminal.mode().contains(TermMode::ALT_SCREEN)
            && followed.observe(screen_rows(terminal));
        if !still_followed {
            self.followed = None;
        }
    }

    /// What is selected, asked once per snapshot and then of every cell.
    pub(crate) fn highlight<T: EventListener>(&self, terminal: &Term<T>) -> Highlight<'_> {
        match &self.followed {
            Some(followed) => Highlight::Followed(followed),
            None => Highlight::Range(
                terminal
                    .selection
                    .as_ref()
                    .and_then(|selection| selection.to_range(terminal)),
            ),
        }
    }

    /// The selection as text: each row's trailing blanks and the blank rows
    /// at its end dropped; empty when it covered nothing else.
    pub(crate) fn text<T: EventListener>(&self, terminal: &Term<T>) -> String {
        let text = match &self.followed {
            Some(followed) => followed.text(),
            None => terminal.selection_to_string().unwrap_or_default(),
        };
        text.trim_end_matches('\n').to_owned()
    }
}

pub(crate) enum Highlight<'a> {
    Followed(&'a FollowedSelection),
    Range(Option<SelectionRange>),
}

impl Highlight<'_> {
    /// Whether the cell at `point` of the grid is selected. On the
    /// alternate screen the grid is the view.
    pub(crate) fn contains(&self, point: Point) -> bool {
        match self {
            Self::Followed(followed) => usize::try_from(point.line.0)
                .is_ok_and(|row| followed.contains(row, point.column.0 as u16)),
            Self::Range(range) => range.is_some_and(|range| range.contains(point)),
        }
    }
}

/// An alternate-screen selection, in lines of content rather than rows of
/// the screen. A line is numbered once, when it is first seen, and keeps
/// its number wherever the program moves it.
#[derive(Clone, Debug)]
pub(crate) struct FollowedSelection {
    screen: Vec<Vec<char>>,
    /// Which line of content each row shows; `None` for a row that is not
    /// part of the content that moves (a prompt, a status line), once a
    /// move has shown it is not.
    lines: Vec<Option<i64>>,
    /// Every line the selection has seen, by number.
    kept: BTreeMap<i64, Vec<char>>,
    anchor: (i64, u16),
    head: (i64, u16),
    /// The move the last redraw made. A redraw cut across two reads of
    /// the program's output arrives as two moves, and the second half may
    /// have a single row to recognise it by.
    last_shift: Option<isize>,
    /// Where the held pointer rests, in the view: when the program moves
    /// the content under it, the selection's end is whatever it rests on
    /// now.
    pointer: Option<(u16, u16)>,
}

impl FollowedSelection {
    fn begin(screen: Vec<Vec<char>>, anchor: (u16, u16), head: (u16, u16)) -> Self {
        let rows = screen.len();
        let row =
            |(column, row): (u16, u16)| (i64::from(row).min(rows.saturating_sub(1) as i64), column);
        let mut selection = Self {
            lines: (0..rows as i64).map(Some).collect(),
            screen,
            kept: BTreeMap::new(),
            last_shift: None,
            anchor: row(anchor),
            head: row(head),
            pointer: Some(head),
        };
        selection.keep();
        selection
    }

    /// Over a row that is not part of the content — a prompt, a status
    /// line — the end goes as far as the content does: the end of the
    /// nearest line above it, or the start of the nearest below.
    fn extend(&mut self, (column, row): (u16, u16)) -> bool {
        self.pointer = Some((column, row));
        let row = usize::from(row).min(self.lines.len().saturating_sub(1));
        let above = || {
            self.lines[..row]
                .iter()
                .rev()
                .flatten()
                .next()
                .map(|line| (*line, u16::MAX))
        };
        let below = || {
            self.lines[row..]
                .iter()
                .flatten()
                .next()
                .map(|line| (*line, 0))
        };
        let Some(head) = self.lines[row]
            .map(|line| (line, column))
            .or_else(above)
            .or_else(below)
        else {
            return false;
        };
        let changed = head != self.head;
        self.head = head;
        changed
    }

    /// Takes in what the program drew; false when the content under the
    /// selection is gone rather than moved.
    fn observe(&mut self, screen: Vec<Vec<char>>) -> bool {
        if screen.len() != self.screen.len()
            || screen.first().map(Vec::len) != self.screen.first().map(Vec::len)
        {
            return false;
        }
        let changed: Vec<usize> = (0..screen.len())
            .filter(|&row| screen[row] != self.screen[row])
            .collect();
        if changed.is_empty() {
            return true;
        }
        let shift = self.shift(&screen, &changed);
        self.last_shift = shift;
        match shift {
            Some(shift) => self.follow(&screen, &changed, shift),
            None => {
                // Rows rewritten where they stand are edits to the lines
                // they show; most of the screen rewritten with nothing
                // moved is another screen altogether.
                let rewritten = changed.iter().filter(|&&row| !blank(&screen[row])).count();
                if rewritten * 2 > screen.len() {
                    return false;
                }
            }
        }
        self.screen = screen;
        self.keep();
        if shift.is_some()
            && let Some(pointer) = self.pointer
        {
            self.extend(pointer);
        }
        true
    }

    /// How far the content moved, as the row a changed row's text came
    /// from minus the row it is on now: the offset most changed rows agree
    /// on, when enough of them do that it cannot be a coincidence: two,
    /// or one carrying on the move the last redraw made.
    fn shift(&self, screen: &[Vec<char>], changed: &[usize]) -> Option<isize> {
        let before: Vec<u64> = self.screen.iter().map(|row| fingerprint(row)).collect();
        let mut occurrences: BTreeMap<u64, usize> = BTreeMap::new();
        for was in &before {
            *occurrences.entry(*was).or_default() += 1;
        }
        let mut votes: BTreeMap<isize, usize> = BTreeMap::new();
        let mut voters = 0;
        for &row in changed {
            if blank(&screen[row]) {
                continue;
            }
            voters += 1;
            let now = fingerprint(&screen[row]);
            for (from, was) in before.iter().enumerate() {
                // A row whose text stood in more than one place says
                // nothing about where it came from: two rule lines, two
                // fences, would otherwise agree on a move nobody made.
                if *was == now && from != row && occurrences[was] == 1 && self.lines[from].is_some()
                {
                    *votes.entry(from as isize - row as isize).or_default() += 1;
                }
            }
        }
        votes
            .into_iter()
            .max_by_key(|&(shift, count)| (count, std::cmp::Reverse(shift.unsigned_abs())))
            // The rows a move brings in changed too, and match nothing: a
            // move is judged by what it explains, the rows it carried and
            // the ones it brought, or a jump of half the screen would read
            // as another screen altogether.
            .filter(|&(shift, count)| {
                (count >= 2 || Some(shift) == self.last_shift)
                    && (count + shift.unsigned_abs()) * 2 >= voters
            })
            .map(|(shift, _)| shift)
    }

    /// Renumbers the rows after the content moved by `shift`.
    fn follow(&mut self, screen: &[Vec<char>], changed: &[usize], shift: isize) {
        let rows = screen.len();
        let mut lines = self.lines.clone();
        let mut moved = vec![false; rows];
        for &row in changed {
            let from = row as isize + shift;
            if let Ok(from) = usize::try_from(from)
                && from < rows
                && screen[row] == self.screen[from]
                && let Some(line) = self.lines[from]
            {
                lines[row] = Some(line);
                moved[row] = true;
            }
        }
        // Between the rows it carried, a row that did not change moved all
        // the same: a blank line inside the content is still blank one row
        // up, and is still the content's.
        let first = moved.iter().position(|&moved| moved);
        let last = moved.iter().rposition(|&moved| moved);
        if let (Some(first), Some(last)) = (first, last) {
            for row in first..=last {
                let from = row as isize + shift;
                if !moved[row]
                    && let Ok(from) = usize::try_from(from)
                    && from < rows
                    && screen[row] == self.screen[from]
                    && let Some(line) = self.lines[from]
                {
                    lines[row] = Some(line);
                    moved[row] = true;
                }
            }
        }
        // The rows the move brought in are the lines beside the ones it
        // carried, as many as it moved by — blank or not, since a line
        // that enters blank over a blank row does not change it.
        let carried = moved.clone();
        let entering = shift.unsigned_abs();
        let order: Vec<usize> = if shift > 0 {
            (0..rows).collect()
        } else {
            (0..rows).rev().collect()
        };
        let mut run = 0;
        for pair in order.windows(2) {
            let (previous, row) = (pair[0], pair[1]);
            if carried[row] {
                run = 0;
            } else if moved[previous] && run < entering {
                let step = if shift > 0 { 1 } else { -1 };
                lines[row] = lines[previous].map(|line| line + step);
                moved[row] = true;
                run += 1;
            } else {
                run = entering;
            }
        }
        // A row still claiming a line the content has just brought
        // somewhere else does not move with the content.
        let claimed: BTreeSet<i64> = (0..rows)
            .filter(|&row| moved[row])
            .filter_map(|row| lines[row])
            .collect();
        for row in 0..rows {
            if !moved[row] && lines[row].is_some_and(|line| claimed.contains(&line)) {
                lines[row] = None;
            }
        }
        self.lines = lines;
    }

    /// Keeps the lines on screen, and forgets the ones the selection can no
    /// longer reach: its end is always on screen, so it can cover nothing
    /// outside what runs from its anchor to the screen.
    fn keep(&mut self) {
        for (row, line) in self.lines.iter().enumerate() {
            if let Some(line) = line {
                self.kept.insert(*line, self.screen[row].clone());
            }
        }
        let anchor = self.anchor.0;
        let on_screen = self.lines.iter().flatten().copied();
        let first = on_screen
            .clone()
            .min()
            .map_or(anchor, |line| line.min(anchor));
        let last = on_screen.max().map_or(anchor, |line| line.max(anchor));
        self.kept.retain(|line, _| (first..=last).contains(line));
    }

    fn ordered(&self) -> ((i64, u16), (i64, u16)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    fn contains(&self, row: usize, column: u16) -> bool {
        let (start, end) = self.ordered();
        self.lines
            .get(row)
            .copied()
            .flatten()
            .is_some_and(|line| (line, column) >= start && (line, column) <= end)
    }

    fn text(&self) -> String {
        let (start, end) = self.ordered();
        let mut lines = Vec::new();
        for line in start.0..=end.0 {
            let Some(cells) = self.kept.get(&line) else {
                continue;
            };
            let mut from = usize::from(if line == start.0 { start.1 } else { 0 });
            // Starting on the cell a wide character spills into starts on
            // the character.
            if from > 0 && cells.get(from) == Some(&SPILL) {
                from -= 1;
            }
            let to = if line == end.0 { end.1 } else { u16::MAX };
            let text: String = cells
                .iter()
                .enumerate()
                .filter(|&(column, _)| (from..=usize::from(to)).contains(&column))
                .map(|(_, character)| *character)
                .filter(|character| *character != SPILL)
                .collect();
            lines.push(text.trim_end().to_owned());
        }
        lines.join("\n")
    }
}

fn fingerprint(row: &[char]) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.hash(&mut hasher);
    hasher.finish()
}

fn blank(row: &[char]) -> bool {
    row.iter()
        .all(|character| *character == ' ' || *character == SPILL)
}

fn screen_rows<T: EventListener>(terminal: &Term<T>) -> Vec<Vec<char>> {
    let grid = terminal.grid();
    (0..grid.screen_lines())
        .map(|row| {
            let line = &grid[Line(row as i32)];
            (0..grid.columns())
                .map(|column| {
                    let cell = &line[Column(column)];
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        SPILL
                    } else {
                        cell.c
                    }
                })
                .collect()
        })
        .collect()
}

/// The primary screen: the terminal's own selection, which the scrollback
/// carries.
fn apply_to_terminal<T: EventListener>(
    terminal: &mut Term<T>,
    reversed: &mut bool,
    gesture: SelectionGesture,
) -> bool {
    let before = terminal
        .selection
        .as_ref()
        .and_then(|selection| selection.to_range(terminal));
    match gesture {
        SelectionGesture::Begin { anchor, head } => {
            let (selection, backwards) =
                spanning(grid_point(terminal, anchor), grid_point(terminal, head));
            terminal.selection = Some(selection);
            *reversed = backwards;
        }
        SelectionGesture::Extend { head } => {
            if let Some(range) = before {
                let anchor = if *reversed { range.end } else { range.start };
                let (selection, backwards) = spanning(anchor, grid_point(terminal, head));
                terminal.selection = Some(selection);
                *reversed = backwards;
            }
        }
        SelectionGesture::Release => {}
        SelectionGesture::Clear => terminal.selection = None,
    }
    terminal
        .selection
        .as_ref()
        .and_then(|selection| selection.to_range(terminal))
        != before
}

/// A cell of the view as a point of the grid, whose lines stay the same
/// lines while the view scrolls over them.
fn grid_point<T>(terminal: &Term<T>, (column, row): (u16, u16)) -> Point {
    let row = usize::from(row).min(terminal.screen_lines().saturating_sub(1));
    let column = usize::from(column).min(terminal.columns().saturating_sub(1));
    viewport_to_point(
        terminal.grid().display_offset(),
        Point::new(row, Column(column)),
    )
}

/// A selection covering both `anchor` and `head` whichever reads first,
/// and whether it was drawn backwards.
fn spanning(anchor: Point, head: Point) -> (Selection, bool) {
    let backwards = head < anchor;
    let (anchor_side, head_side) = if backwards {
        (Side::Right, Side::Left)
    } else {
        (Side::Left, Side::Right)
    };
    let mut selection = Selection::new(SelectionType::Simple, anchor, anchor_side);
    selection.update(head, head_side);
    (selection, backwards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{
        event::VoidListener, term::Config, term::test::TermSize, vte::ansi::Processor,
    };

    fn terminal(columns: usize, rows: usize) -> Term<VoidListener> {
        Term::new(
            Config::default(),
            &TermSize::new(columns, rows),
            VoidListener,
        )
    }

    fn feed(terminal: &mut Term<VoidListener>, bytes: &[u8]) {
        let mut parser: Processor = Processor::new();
        parser.advance(terminal, bytes);
    }

    fn selected_rows(terminal: &Term<VoidListener>, selection: &PaneSelection) -> Vec<String> {
        let highlight = selection.highlight(terminal);
        let offset = terminal.grid().display_offset();
        (0..terminal.screen_lines())
            .map(|row| {
                (0..terminal.columns())
                    .map(|column| viewport_to_point(offset, Point::new(row, Column(column))))
                    .filter(|point| highlight.contains(*point))
                    .map(|point| terminal.grid()[point].c)
                    .collect()
            })
            .collect()
    }

    fn begin(anchor: (u16, u16), head: (u16, u16)) -> SelectionGesture {
        SelectionGesture::Begin { anchor, head }
    }

    fn extend(head: (u16, u16)) -> SelectionGesture {
        SelectionGesture::Extend { head }
    }

    /// Five numbered lines into a three-row pane: two in the scrollback,
    /// three on screen.
    fn numbered_lines() -> Term<VoidListener> {
        let mut terminal = terminal(12, 3);
        feed(
            &mut terminal,
            b"line 1\r\nline 2\r\nline 3\r\nline 4\r\nline 5",
        );
        terminal
    }

    #[test]
    fn a_selection_stays_on_its_lines_while_the_view_scrolls() {
        let mut terminal = numbered_lines();
        let mut selection = PaneSelection::default();
        assert!(selection.apply(&mut terminal, begin((0, 0), (5, 0))));
        assert_eq!(selected_rows(&terminal, &selection), ["line 3", "", ""]);

        terminal.scroll_display(alacritty_terminal::grid::Scroll::Delta(1));
        assert_eq!(selected_rows(&terminal, &selection), ["", "line 3", ""]);
    }

    #[test]
    fn a_selection_extended_after_scrolling_copies_past_the_screen() {
        let mut terminal = numbered_lines();
        let mut selection = PaneSelection::default();
        selection.apply(&mut terminal, begin((5, 2), (5, 2)));
        terminal.scroll_display(alacritty_terminal::grid::Scroll::Delta(2));
        selection.apply(&mut terminal, extend((0, 0)));
        assert_eq!(
            selection.text(&terminal),
            "line 1\nline 2\nline 3\nline 4\nline 5"
        );
    }

    #[test]
    fn a_selection_drawn_backwards_keeps_the_cell_it_was_pressed_on() {
        let mut terminal = numbered_lines();
        let mut selection = PaneSelection::default();
        selection.apply(&mut terminal, begin((5, 1), (2, 0)));
        assert_eq!(selection.text(&terminal), "ne 3\nline 4");
        selection.apply(&mut terminal, extend((3, 2)));
        assert_eq!(selection.text(&terminal), "4\nline");
    }

    #[test]
    fn a_selection_moves_with_output_that_scrolls_its_lines_up() {
        let mut terminal = numbered_lines();
        let mut selection = PaneSelection::default();
        selection.apply(&mut terminal, begin((0, 2), (5, 2)));
        feed(&mut terminal, b"\r\nline 6");
        assert_eq!(selected_rows(&terminal, &selection), ["", "line 5", ""]);
        assert_eq!(selection.text(&terminal), "line 5");
    }

    #[test]
    fn clearing_the_selection_leaves_nothing_selected_or_to_copy() {
        let mut terminal = numbered_lines();
        let mut selection = PaneSelection::default();
        selection.apply(&mut terminal, begin((0, 0), (5, 2)));
        assert!(selection.apply(&mut terminal, SelectionGesture::Clear));
        assert_eq!(selected_rows(&terminal, &selection), ["", "", ""]);
        assert_eq!(selection.text(&terminal), "");
    }

    /// A full-screen program on the alternate screen: a transcript that
    /// shows `first..` in every row but the last, and a prompt it keeps in
    /// the last row, redrawn whole the way such a program scrolls.
    struct FullScreen {
        terminal: Term<VoidListener>,
        selection: PaneSelection,
        rows: usize,
    }

    impl FullScreen {
        fn showing(first: usize) -> Self {
            Self::showing_rows(first, 5)
        }

        fn showing_rows(first: usize, rows: usize) -> Self {
            let mut screen = Self {
                terminal: terminal(20, rows),
                selection: PaneSelection::default(),
                rows,
            };
            feed(&mut screen.terminal, b"\x1b[?1049h");
            screen.transcript_from(first);
            screen
        }

        fn frame(first: usize, rows: usize) -> Vec<String> {
            let mut lines: Vec<String> = (first..first + rows - 1)
                .map(|line| format!("message {line}"))
                .collect();
            lines.push("> prompt".to_owned());
            lines
        }

        fn transcript_from(&mut self, first: usize) {
            let lines = Self::frame(first, self.rows);
            self.draw_rows(&lines, 0..lines.len());
        }

        /// Redraws only `rows` of the frame: what one read of a program's
        /// output that was cut short leaves on screen.
        fn draw_rows(&mut self, lines: &[String], rows: std::ops::Range<usize>) {
            let mut bytes = Vec::new();
            for row in rows {
                bytes.extend(format!("\x1b[{};1H\x1b[2K{}", row + 1, lines[row]).bytes());
            }
            feed(&mut self.terminal, &bytes);
            self.selection.observe(&self.terminal);
        }

        fn apply(&mut self, gesture: SelectionGesture) {
            self.selection.apply(&mut self.terminal, gesture);
        }

        fn selected(&self) -> Vec<String> {
            selected_rows(&self.terminal, &self.selection)
        }

        fn text(&self) -> String {
            self.selection.text(&self.terminal)
        }
    }

    #[test]
    fn a_selection_follows_the_lines_a_full_screen_program_moves() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 1), (9, 1)));
        screen.apply(SelectionGesture::Release);
        assert_eq!(screen.selected(), ["", "message 11", "", "", ""]);

        screen.transcript_from(8);
        assert_eq!(screen.selected(), ["", "", "", "message 11", ""]);
        screen.transcript_from(10);
        assert_eq!(screen.selected(), ["", "message 11", "", "", ""]);
    }

    #[test]
    fn what_scrolled_off_a_full_screen_program_is_still_copied() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 0), (0, 0)));
        for first in 11..=14 {
            screen.transcript_from(first);
        }
        screen.apply(extend((9, 3)));
        assert_eq!(
            screen.text(),
            "message 10\nmessage 11\nmessage 12\nmessage 13\nmessage 14\nmessage 15\nmessage 16\nmessage 17"
        );
    }

    #[test]
    fn the_row_a_program_keeps_in_place_is_not_part_of_what_moved() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 0), (9, 3)));
        screen.apply(SelectionGesture::Release);
        screen.transcript_from(12);
        assert_eq!(
            screen.selected(),
            ["message 12          ", "message 13", "", "", ""],
            "the prompt row is not the transcript's next line"
        );
    }

    #[test]
    fn a_held_pointer_takes_the_selection_to_what_moves_under_it() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((9, 2), (0, 3)));
        screen.transcript_from(8);
        assert_eq!(
            screen.text(),
            "message 11\nmessage 12",
            "the pointer rests on row 3, which shows message 11 now"
        );
    }

    #[test]
    fn a_redraw_cut_across_two_reads_moves_the_selection_once() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 3), (9, 3)));
        screen.apply(SelectionGesture::Release);
        let lines = FullScreen::frame(11, 5);
        screen.draw_rows(&lines, 0..2);
        screen.draw_rows(&lines, 2..5);
        assert_eq!(screen.selected(), ["", "", "message 13", "", ""]);
    }

    #[test]
    fn a_line_edited_where_it_stands_stays_selected() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 1), (19, 1)));
        screen.apply(SelectionGesture::Release);
        let mut lines = FullScreen::frame(10, 5);
        lines[1] = "message 11 (edited)".to_owned();
        screen.draw_rows(&lines, 1..2);
        assert_eq!(screen.text(), "message 11 (edited)");
    }

    #[test]
    fn another_screen_altogether_ends_the_selection() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 1), (9, 1)));
        let other: Vec<String> = (0..5).map(|row| format!("settings {row}")).collect();
        screen.draw_rows(&other, 0..5);
        assert_eq!(screen.selected(), ["", "", "", "", ""]);
        assert_eq!(screen.text(), "");
    }

    #[test]
    fn a_jump_of_more_than_half_the_screen_is_still_a_move() {
        let mut screen = FullScreen::showing_rows(10, 10);
        screen.apply(begin((0, 7), (9, 8)));
        screen.apply(SelectionGesture::Release);
        screen.transcript_from(16);
        assert_eq!(
            screen.selected(),
            [
                "",
                "message 17          ",
                "message 18",
                "",
                "",
                "",
                "",
                "",
                "",
                ""
            ],
            "three rows carried and six brought in explain the whole redraw"
        );
    }

    #[test]
    fn rows_repeated_on_screen_do_not_vote_for_a_move() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 1), (9, 1)));
        screen.apply(SelectionGesture::Release);
        let rule = "-".repeat(10);
        let mut lines = FullScreen::frame(10, 5);
        lines[2] = rule.clone();
        lines[3] = rule.clone();
        screen.draw_rows(&lines, 2..4);
        let mut edited = lines.clone();
        edited[0] = rule.clone();
        edited[1] = rule;
        screen.draw_rows(&edited, 0..2);
        assert_eq!(
            screen.selected(),
            ["", "----------", "", "", ""],
            "two rows turning into a rule that already stood twice is an edit where \
             they stand, not the rules moving up two rows"
        );
    }

    #[test]
    fn a_selection_starting_on_a_wide_characters_spill_copies_the_character() {
        let mut screen = FullScreen::showing(10);
        let mut lines = FullScreen::frame(10, 5);
        lines[0] = "日本 x".to_owned();
        screen.draw_rows(&lines, 0..1);
        screen.apply(begin((1, 0), (5, 0)));
        assert_eq!(screen.text(), "日本 x");
    }

    /// A transcript whose every third line is blank, the way paragraphs
    /// leave it, above the prompt.
    fn paragraphs(first: usize, rows: usize) -> Vec<String> {
        let mut lines: Vec<String> = (first..first + rows - 1)
            .map(|line| {
                if line % 3 == 2 {
                    String::new()
                } else {
                    format!("message {line}")
                }
            })
            .collect();
        lines.push("> prompt".to_owned());
        lines
    }

    #[test]
    fn blank_lines_inside_the_content_move_with_it() {
        let mut screen = FullScreen::showing_rows(10, 10);
        screen.draw_rows(&paragraphs(10, 10), 0..10);
        screen.apply(begin((0, 3), (9, 6)));
        screen.apply(SelectionGesture::Release);
        screen.draw_rows(&paragraphs(13, 10), 0..10);
        let blank = " ".repeat(20);
        assert_eq!(
            screen.selected()[..5],
            [
                "message 13          ".to_owned(),
                blank,
                "message 15          ".to_owned(),
                "message 16".to_owned(),
                String::new(),
            ],
            "the blank row between 13 and 15 is the blank line between them, \
             not the one that stood there before the move"
        );
    }

    #[test]
    fn lines_that_scrolled_past_a_held_press_are_still_copied() {
        let mut screen = FullScreen::showing_rows(10, 10);
        screen.draw_rows(&paragraphs(10, 10), 0..10);
        screen.apply(begin((0, 0), (5, 0)));
        screen.draw_rows(&paragraphs(13, 10), 0..10);
        screen.apply(extend((0, 4)));
        assert_eq!(
            screen.text(),
            "message 10\n\nmessage 12\nmessage 13\n\nmessage 15\nmessage 16",
            "the press was on message 10, and the pointer came to rest on the blank row \
             after message 16"
        );
    }

    #[test]
    fn a_pointer_over_the_prompt_selects_to_the_end_of_the_content() {
        let mut screen = FullScreen::showing(10);
        screen.apply(begin((0, 1), (3, 1)));
        screen.transcript_from(11);
        screen.apply(extend((5, 4)));
        assert_eq!(
            screen.text(),
            "message 11\nmessage 12\nmessage 13\nmessage 14"
        );
    }

    /// A transcript with three blank lines after every two messages.
    fn spaced(first: usize) -> Vec<String> {
        let mut lines: Vec<String> = (first..first + 9)
            .map(|line| {
                if (2..=4).contains(&(line % 5)) {
                    String::new()
                } else {
                    format!("message {line}")
                }
            })
            .collect();
        lines.push("> prompt".to_owned());
        lines
    }

    #[test]
    fn a_run_of_blank_lines_longer_than_the_move_moves_with_it() {
        let mut screen = FullScreen::showing_rows(10, 10);
        screen.draw_rows(&spaced(10), 0..10);
        screen.apply(begin((0, 1), (9, 6)));
        screen.apply(SelectionGesture::Release);
        screen.draw_rows(&spaced(11), 0..10);
        let blank = " ".repeat(20);
        assert_eq!(
            screen.selected()[..6],
            [
                "message 11          ".to_owned(),
                blank.clone(),
                blank.clone(),
                blank,
                "message 15          ".to_owned(),
                "message 16".to_owned(),
            ],
            "three blank lines moved by one are still the three between 11 and 15"
        );
    }

    #[test]
    fn a_blank_line_entering_over_a_blank_row_is_still_content() {
        let mut screen = FullScreen::showing_rows(10, 10);
        screen.draw_rows(&spaced(9), 0..10);
        screen.apply(begin((0, 7), (9, 7)));
        screen.apply(SelectionGesture::Release);
        screen.draw_rows(&spaced(10), 0..10);
        screen.apply(extend((9, 8)));
        assert_eq!(
            screen.selected()[8],
            " ".repeat(10),
            "the last row shows the blank line 18 the move brought in, though it \
             was blank before too"
        );
    }
}
