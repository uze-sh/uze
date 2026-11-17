//! Where the screen is on the board.
//!
//! A drawing larger than the screen needs the one thing a scrollbar
//! cannot give in two dimensions: the whole of it, small, with the part
//! being looked at marked. Braille gives eight dots to a cell, which is
//! enough resolution for boxes to keep their arrangement — and it is
//! text, so it goes wherever the rest of the diagram goes.
//!
//! The map is one size, in one place, for every drawing: the drawing is
//! scaled to fit the map, never the map to fit the drawing. A control
//! that changes shape between two diagrams has to be found again each
//! time, and this one exists so that nothing has to be looked for.
//!
//! The geometry is one value both halves use: what is painted and what a
//! click on it means cannot disagree about where a dot is.

use crate::view::Role;

use crate::shared::canvas::{Canvas, Cell, Corners, Frame, Glyphs};

const CELLS: (i32, i32) = (28, 7);
const DOTS: (i32, i32) = (CELLS.0 * 2, CELLS.1 * 4);
/// Below this the map would cover more of the board than it explains.
const SMALLEST_SCREEN: (i32, i32) = (70, 18);

/// Braille's dot numbering, as the bit each `(column, row)` of a cell sets.
const DOT_BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Minimap {
    /// The framed map, in screen cells.
    pub frame: Frame,
    /// Board cells per dot across. A cell is twice as tall as it is wide
    /// and a dot is half a cell wide, so a dot down is half as many rows.
    scale: f32,
    /// Where the drawing starts inside the map, in dots: it is centred,
    /// since it rarely has the map's proportions.
    origin: (i32, i32),
}

impl Minimap {
    pub fn of(board: (i32, i32), screen: (i32, i32)) -> Option<Self> {
        if screen.0 < SMALLEST_SCREEN.0 || screen.1 < SMALLEST_SCREEN.1 || board.0 < 1 {
            return None;
        }
        let across = board.0 as f32 / DOTS.0 as f32;
        let down = board.1 as f32 * 2.0 / DOTS.1 as f32;
        let scale = across.max(down).max(0.01);
        let drawn = (
            (board.0 as f32 / scale) as i32,
            (board.1 as f32 * 2.0 / scale) as i32,
        );
        Some(Self {
            frame: Frame {
                x: screen.0 - CELLS.0 - 3,
                y: screen.1 - CELLS.1 - 2,
                w: CELLS.0 + 2,
                h: CELLS.1 + 2,
            },
            scale,
            origin: ((DOTS.0 - drawn.0) / 2, (DOTS.1 - drawn.1) / 2),
        })
    }

    fn dot_of(&self, x: i32, y: i32) -> (i32, i32) {
        (
            self.origin.0 + (x as f32 / self.scale) as i32,
            self.origin.1 + (y as f32 * 2.0 / self.scale) as i32,
        )
    }

    /// The board cell a click on this screen cell of the map points at.
    pub fn board_cell_at(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let inside = Frame {
            x: self.frame.x + 1,
            y: self.frame.y + 1,
            w: CELLS.0,
            h: CELLS.1,
        };
        inside.contains(x, y).then(|| {
            let dot = (
                (x - inside.x) * 2 + 1 - self.origin.0,
                (y - inside.y) * 4 + 2 - self.origin.1,
            );
            (
                (dot.0 as f32 * self.scale) as i32,
                (dot.1 as f32 * self.scale / 2.0) as i32,
            )
        })
    }

    pub fn paint(&self, screen: &mut Canvas, boxes: &[Frame], looking_at: Frame) {
        let mut dots = vec![0u32; (CELLS.0 * CELLS.1) as usize];
        let mut edge = vec![false; (CELLS.0 * CELLS.1) as usize];
        let mut mark = |dot: (i32, i32), on_edge: bool| {
            let inside = dot.0 >= 0 && dot.1 >= 0 && dot.0 < DOTS.0 && dot.1 < DOTS.1;
            if inside {
                let index = (dot.1 / 4 * CELLS.0 + dot.0 / 2) as usize;
                dots[index] |= DOT_BITS[(dot.0 % 2) as usize][(dot.1 % 4) as usize];
                edge[index] |= on_edge;
            }
        };
        for frame in boxes {
            let (from, to) = (
                self.dot_of(frame.x, frame.y),
                self.dot_of(frame.x + frame.w - 1, frame.y + frame.h - 1),
            );
            for y in from.1..=to.1 {
                for x in from.0..=to.0 {
                    mark((x, y), false);
                }
            }
        }
        // The part being looked at, as an outline — cut where it leaves
        // the map, which is what says the screen has gone past the edge.
        let (from, to) = (
            self.dot_of(looking_at.x, looking_at.y),
            self.dot_of(
                looking_at.x + looking_at.w - 1,
                looking_at.y + looking_at.h - 1,
            ),
        );
        for x in from.0..=to.0 {
            mark((x, from.1), true);
            mark((x, to.1), true);
        }
        for y in from.1..=to.1 {
            mark((from.0, y), true);
            mark((to.0, y), true);
        }

        for y in self.frame.y..self.frame.y + self.frame.h {
            for x in self.frame.x..self.frame.x + self.frame.w {
                screen.set(
                    x,
                    y,
                    Cell {
                        solid: true,
                        ..Cell::default()
                    },
                );
            }
        }
        screen.frame(self.frame, Corners::Rounded, Role::Dim);
        for cy in 0..CELLS.1 {
            for cx in 0..CELLS.0 {
                let index = (cy * CELLS.0 + cx) as usize;
                if dots[index] == 0 {
                    continue;
                }
                let glyph = match screen.glyphs {
                    Glyphs::Unicode => char::from_u32(0x2800 + dots[index]).unwrap_or(' '),
                    Glyphs::Ascii if edge[index] => '+',
                    Glyphs::Ascii => '#',
                };
                let role = if edge[index] {
                    Role::Accent
                } else {
                    Role::Secondary
                };
                screen.put(
                    self.frame.x + 1 + cx,
                    self.frame.y + 1 + cy,
                    glyph,
                    role,
                    false,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_is_one_size_in_one_place_whatever_the_drawing() {
        let screen = (120, 40);
        let wide = Minimap::of((300, 25), screen).unwrap();
        let tall = Minimap::of((60, 200), screen).unwrap();
        let small = Minimap::of((40, 12), screen).unwrap();
        assert_eq!(wide.frame, tall.frame);
        assert_eq!(wide.frame, small.frame);
    }

    #[test]
    fn a_click_on_the_map_points_back_at_the_part_of_the_board_drawn_there() {
        let map = Minimap::of((190, 80), (120, 40)).unwrap();
        let near = map.board_cell_at(map.frame.x + 1, map.frame.y + 1).unwrap();
        let far = map
            .board_cell_at(map.frame.x + map.frame.w - 2, map.frame.y + 1)
            .unwrap();
        assert!(far.0 > near.0 + 100, "{near:?} → {far:?}");
        assert_eq!(map.board_cell_at(0, 0), None);
    }

    #[test]
    fn a_point_on_the_board_and_the_click_on_its_dot_agree() {
        let map = Minimap::of((190, 80), (120, 40)).unwrap();
        let dot = map.dot_of(100, 40);
        let cell = (map.frame.x + 1 + dot.0 / 2, map.frame.y + 1 + dot.1 / 4);
        let back = map.board_cell_at(cell.0, cell.1).unwrap();
        assert!(
            (back.0 - 100).abs() <= 8 && (back.1 - 40).abs() <= 6,
            "{back:?}"
        );
    }

    #[test]
    fn a_map_is_offered_only_on_a_screen_with_room_for_it_and_a_drawing_to_show() {
        let board = (100, 50);
        assert!(
            Minimap::of(board, SMALLEST_SCREEN).is_some(),
            "the smallest screen"
        );
        assert!(Minimap::of(board, (SMALLEST_SCREEN.0 - 1, SMALLEST_SCREEN.1)).is_none());
        assert!(Minimap::of(board, (SMALLEST_SCREEN.0, SMALLEST_SCREEN.1 - 1)).is_none());
        assert!(Minimap::of((0, 50), (120, 40)).is_none(), "nothing drawn");
        assert!(
            Minimap::of((1, 50), (120, 40)).is_some(),
            "one column drawn"
        );
    }

    /// The braille cells, framed, in the screen's bottom-right corner.
    #[test]
    fn the_map_sits_framed_in_the_bottom_right_corner_of_the_screen() {
        let screen = (120, 40);
        let map = Minimap::of((300, 25), screen).unwrap();
        assert_eq!((map.frame.w, map.frame.h), (CELLS.0 + 2, CELLS.1 + 2));
        assert!(map.frame.x + map.frame.w <= screen.0 && map.frame.y + map.frame.h <= screen.1);
        assert!(map.frame.x > screen.0 / 2 && map.frame.y > screen.1 / 2);
    }

    /// Two dots across a braille cell and four down: the drawing is scaled
    /// to fit the map whole, and centred along its shorter side.
    #[test]
    fn a_drawing_is_scaled_to_fit_the_map_whole_and_centred_in_it() {
        let across = CELLS.0 * 2;
        let down = CELLS.1 * 4;
        for board in [(560, 40), (100, 200), (190, 80), (20, 5)] {
            let map = Minimap::of(board, (120, 40)).unwrap();
            let first = map.dot_of(0, 0);
            let end = map.dot_of(board.0, board.1);
            assert!(first.0 >= 0 && first.1 >= 0, "{board:?}: {first:?}");
            assert!(end.0 <= across && end.1 <= down, "{board:?}: {end:?}");
            let fills_across = first.0 <= 1 && end.0 >= across - 1;
            let fills_down = first.1 <= 1 && end.1 >= down - 1;
            assert!(fills_across || fills_down, "{board:?} is drawn small");
            assert!(
                (first.0 + end.0 - across).abs() <= 2,
                "{board:?} is centred across: {first:?} {end:?}"
            );
            assert!(
                (first.1 + end.1 - down).abs() <= 2,
                "{board:?} is centred down: {first:?} {end:?}"
            );
        }
    }

    #[test]
    fn a_click_on_a_dot_points_back_at_its_row_where_the_drawing_is_centred_down() {
        let map = Minimap::of((560, 40), (120, 40)).unwrap();
        let dot = map.dot_of(300, 20);
        let cell = (map.frame.x + 1 + dot.0 / 2, map.frame.y + 1 + dot.1 / 4);
        let back = map.board_cell_at(cell.0, cell.1).unwrap();
        assert!((back.1 - 20).abs() <= 20, "{back:?}");
    }

    /// The map painted over a screen covered in `X`, as the glyph and role
    /// of every braille cell inside its frame.
    fn painted(
        board: (i32, i32),
        boxes: &[Frame],
        looking_at: Frame,
        glyphs: Glyphs,
    ) -> (Minimap, Canvas) {
        let screen = (120, 40);
        let map = Minimap::of(board, screen).unwrap();
        let mut canvas = Canvas::new(screen.0, screen.1, glyphs);
        for y in 0..screen.1 {
            for x in 0..screen.0 {
                canvas.put(x, y, 'X', Role::Default, false);
            }
        }
        map.paint(&mut canvas, boxes, looking_at);
        (map, canvas)
    }

    fn braille(map: &Minimap, canvas: &Canvas, column: i32, row: i32) -> Cell {
        canvas
            .cell(map.frame.x + 1 + column, map.frame.y + 1 + row)
            .unwrap()
    }

    fn lit_cells(map: &Minimap, canvas: &Canvas) -> Vec<(i32, i32, Role)> {
        let mut lit = Vec::new();
        for row in 0..CELLS.1 {
            for column in 0..CELLS.0 {
                let cell = braille(map, canvas, column, row);
                if cell.glyph != ' ' {
                    lit.push((column, row, cell.role));
                }
            }
        }
        lit
    }

    const NOWHERE: Frame = Frame {
        x: -1000,
        y: -1000,
        w: 1,
        h: 1,
    };

    /// What the map covers is the map's: the board under it does not show
    /// through, and it is framed.
    #[test]
    fn the_map_covers_the_board_under_it() {
        let (map, canvas) = painted((560, 40), &[], NOWHERE, Glyphs::Unicode);
        for y in map.frame.y..map.frame.y + map.frame.h {
            for x in map.frame.x..map.frame.x + map.frame.w {
                assert_ne!(canvas.cell(x, y).unwrap().glyph, 'X', "({x},{y})");
            }
        }
        assert_ne!(
            canvas.cell(map.frame.x, map.frame.y).unwrap().glyph,
            ' ',
            "framed"
        );
        assert!(lit_cells(&map, &canvas).is_empty(), "nothing to show");
    }

    /// A box is dots in the secondary role, where the box is on the board.
    #[test]
    fn a_box_is_drawn_where_it_stands_on_the_board() {
        let board = (560, 40);
        let left = Frame {
            x: 0,
            y: 0,
            w: 56,
            h: 40,
        };
        let (map, canvas) = painted(board, &[left], NOWHERE, Glyphs::Unicode);
        let lit = lit_cells(&map, &canvas);
        assert!(!lit.is_empty(), "the box is drawn");
        assert!(
            lit.iter()
                .all(|&(column, _, role)| column < CELLS.0 / 4 && role == Role::Secondary),
            "{lit:?}"
        );
        let rows: Vec<i32> = lit.iter().map(|&(_, row, _)| row).collect();
        let (top, bottom) = (rows.iter().min().unwrap(), rows.iter().max().unwrap());
        assert!(
            (top - (CELLS.1 - 1 - bottom)).abs() <= 1,
            "centred down: rows {top}..={bottom}"
        );
    }

    /// Every dot of a cell the drawing covers is lit.
    #[test]
    fn a_cell_the_drawing_covers_lights_every_dot() {
        let board = (56, 28);
        let whole = Frame {
            x: 0,
            y: 0,
            w: 56,
            h: 28,
        };
        let (map, canvas) = painted(board, &[whole], NOWHERE, Glyphs::Unicode);
        assert_eq!(
            braille(&map, &canvas, CELLS.0 / 2, CELLS.1 / 2).glyph,
            '\u{28ff}'
        );
    }

    /// Boxes a cell apart stay apart on the map: a box is drawn no larger
    /// than it is, across or down.
    #[test]
    fn boxes_apart_on_the_board_stay_apart_on_the_map() {
        let side_by_side = [
            Frame {
                x: 0,
                y: 0,
                w: 5,
                h: 2,
            },
            Frame {
                x: 6,
                y: 0,
                w: 5,
                h: 2,
            },
        ];
        let (map, canvas) = painted((11, 2), &side_by_side, NOWHERE, Glyphs::Unicode);
        let gap = map.dot_of(5, 0).0 / 2;
        let lit = lit_cells(&map, &canvas);
        assert!(lit.iter().any(|&(column, _, _)| column < gap), "{lit:?}");
        assert!(lit.iter().any(|&(column, _, _)| column > gap), "{lit:?}");
        assert!(
            !lit.iter().any(|&(column, _, _)| column == gap),
            "column {gap} is between the two: {lit:?}"
        );

        let stacked = [
            Frame {
                x: 0,
                y: 0,
                w: 2,
                h: 5,
            },
            Frame {
                x: 0,
                y: 7,
                w: 2,
                h: 8,
            },
        ];
        let (map, canvas) = painted((2, 15), &stacked, NOWHERE, Glyphs::Unicode);
        let gap = map.dot_of(0, 5).1 / 4;
        let lit = lit_cells(&map, &canvas);
        assert!(lit.iter().any(|&(_, row, _)| row < gap), "{lit:?}");
        assert!(lit.iter().any(|&(_, row, _)| row > gap), "{lit:?}");
        assert!(
            !lit.iter().any(|&(_, row, _)| row == gap),
            "row {gap} is between the two: {lit:?}"
        );
    }

    /// The part being looked at is an outline in the accent role, all four
    /// sides of it, and nothing inside.
    #[test]
    fn the_part_looked_at_is_outlined() {
        let board = (560, 40);
        let looking_at = Frame {
            x: 140,
            y: 0,
            w: 280,
            h: 40,
        };
        let (map, canvas) = painted(board, &[], looking_at, Glyphs::Unicode);
        let lit = lit_cells(&map, &canvas);
        assert!(
            lit.iter().all(|&(_, _, role)| role == Role::Accent),
            "{lit:?}"
        );
        let left = map.dot_of(140, 0).0 / 2;
        let right = map.dot_of(419, 0).0 / 2;
        let top = map.dot_of(0, 0).1 / 4;
        let bottom = map.dot_of(0, 39).1 / 4;
        let middle = (left + right) / 2;
        for (column, row) in [
            (left, top + 1),
            (right, top + 1),
            (middle, top),
            (middle, bottom),
        ] {
            assert!(
                lit.iter().any(|&(c, r, _)| (c, r) == (column, row)),
                "({column},{row}) is on the outline: {lit:?}"
            );
        }
        assert!(
            !lit.iter()
                .any(|&(c, r, _)| c > left && c < right && r > top && r < bottom),
            "an outline: {lit:?}"
        );
        let (map, canvas) = painted(board, &[], looking_at, Glyphs::Ascii);
        assert_eq!(braille(&map, &canvas, middle, top).glyph, '+');
    }

    /// A screen gone past the board's edge is cut where it leaves the map,
    /// and what is cut lands nowhere else.
    #[test]
    fn an_outline_past_the_edge_of_the_map_is_cut_there() {
        let board = (560, 40);
        let past = Frame {
            x: 400,
            y: 20,
            w: 400,
            h: 100,
        };
        let (map, canvas) = painted(board, &[], past, Glyphs::Unicode);
        let left = map.dot_of(400, 0).0 / 2;
        let lit = lit_cells(&map, &canvas);
        assert!(!lit.is_empty());
        assert!(lit.iter().all(|&(column, _, _)| column >= left), "{lit:?}");
    }
}
