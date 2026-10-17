//! A surface's image: fitted to the room there is, two pixels to a cell.

use uze_extensions::view::Picture;

use super::*;
use crate::ui::graphics::{self, Placement};

/// `heading` over `picture`, scaled to fit what is left of `area` and
/// centred in it — in half blocks always, and where the terminal draws
/// images (`cell`, its pixels a cell), as the [`Placement`] the frame
/// lays over them once it is out.
pub(super) fn render_picture(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    heading: &str,
    picture: &Picture,
    cell: Option<(u16, u16)>,
    modes: &[Mode],
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Option<Placement> {
    frame.render_widget(
        Paragraph::new(TextSpan::styled(
            heading.to_owned(),
            theme::fg(Token::TextSecondary),
        )),
        Rect::new(
            area.x,
            area.y,
            area.width.saturating_sub(modes_width(modes)),
            1,
        ),
    );
    render_modes(frame, area, modes, hits);
    let body = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    );
    let screen = frame.area();
    let placement = cell
        .and_then(|cell| Placement::fit(picture, body, cell))
        .map(|placement| placement.on(screen));
    // Under pixels, the half blocks fill exactly the cells the pixels are
    // sent over: they are the mark that says nothing covered them, and a
    // fit at two pixels a cell can come out a column short of a fit at the
    // terminal's own cell size.
    let drawn = match &placement {
        Some(placement) => Some((placement.cells, placement.cells.height * 2)),
        None => fitted(picture, body.width, body.height).map(|(columns, rows)| {
            let left = body.x + (body.width - columns) / 2;
            let top = body.y + (body.height - rows.div_ceil(2)) / 2;
            (Rect::new(left, top, columns, rows.div_ceil(2)), rows)
        }),
    };
    if let Some((cells, rows)) = drawn {
        render_half_blocks(frame, cells, rows, picture);
    }
    placement
}

/// `picture` in half blocks over `cells`, `rows` pixels tall: twice the
/// cells, or one fewer when the picture ends on an upper half.
fn render_half_blocks(frame: &mut ratatui::Frame<'_>, cells: Rect, rows: u16, picture: &Picture) {
    let (columns, left, top) = (cells.width, cells.x, cells.y);
    let buffer = frame.buffer_mut();
    for row in 0..rows.div_ceil(2) {
        for column in 0..columns {
            let upper = graphics::sample(picture, column, row * 2, columns, rows);
            let lower = match row * 2 + 1 < rows {
                true => theme::pixel(graphics::sample(
                    picture,
                    column,
                    row * 2 + 1,
                    columns,
                    rows,
                )),
                false => theme::color(Token::SurfaceBackground),
            };
            if let Some(cell) = buffer.cell_mut((left + column, top + row)) {
                cell.set_symbol(graphics::HALF_BLOCK)
                    .set_fg(theme::pixel(upper))
                    .set_bg(lower);
            }
        }
    }
}

/// How many columns and pixel rows `picture` takes in `width` × `height`
/// cells, keeping its proportions: a cell is about twice as tall as it is
/// wide, so each half of one is close to square.
fn fitted(picture: &Picture, width: u16, height: u16) -> Option<(u16, u16)> {
    if picture.width == 0 || picture.height == 0 || width == 0 || height == 0 {
        return None;
    }
    let room = (usize::from(width), usize::from(height) * 2);
    let (columns, rows) = match room.0 * picture.height <= room.1 * picture.width {
        true => (room.0, (picture.height * room.0 / picture.width).max(1)),
        false => ((picture.width * room.1 / picture.height).max(1), room.1),
    };
    Some((u16::try_from(columns).ok()?, u16::try_from(rows).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: usize, height: usize) -> Picture {
        Picture {
            width,
            height,
            pixels: vec![[255, 0, 0, 255]; width * height].into(),
        }
    }

    #[test]
    fn a_wide_image_takes_the_width_and_keeps_its_proportions() {
        assert_eq!(fitted(&picture(200, 100), 80, 40), Some((80, 40)));
    }

    #[test]
    fn a_tall_image_takes_the_height_two_pixels_to_a_row() {
        assert_eq!(fitted(&picture(100, 200), 80, 20), Some((20, 40)));
    }

    /// The pixels are sent only over cells that all still hold a half
    /// block, so the half blocks must cover every cell of the placement —
    /// on a terminal whose cells are not exactly twice as tall as wide as
    /// much as on one whose are.
    #[test]
    fn the_half_blocks_cover_every_cell_the_pixels_are_sent_over() {
        for cell in [(10, 20), (9, 19), (8, 17), (7, 16)] {
            let backend = ratatui::backend::TestBackend::new(100, 41);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            let mut placement = None;
            terminal
                .draw(|frame| {
                    placement = render_picture(
                        frame,
                        frame.area(),
                        "poster.png",
                        &picture(1000, 500),
                        Some(cell),
                        &[],
                        &mut Vec::new(),
                    );
                })
                .unwrap();
            let placement = placement.expect("a terminal that draws images gets a placement");
            assert!(
                placement.uncovered_in(terminal.backend().buffer()),
                "{cell:?}: every cell of {:?} is a half block",
                placement.cells
            );
        }
    }

    #[test]
    fn a_picture_with_no_room_draws_nothing() {
        assert_eq!(fitted(&picture(10, 10), 0, 5), None);
    }
}
