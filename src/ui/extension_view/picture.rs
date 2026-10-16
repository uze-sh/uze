//! A surface's image: fitted to the room there is, two pixels to a cell.

use uze_extensions::view::Picture;

use super::*;
use crate::ui::graphics::{self, Placement};

/// The upper half block: its glyph paints the top pixel and the cell's
/// ground the bottom one, so every terminal with true colour shows two
/// pixels per cell without a graphics protocol — which a pane inside the
/// workspace could not pass through anyway.
const UPPER_HALF: &str = "\u{2580}";

/// `heading` over `picture`, scaled to fit what is left of `area` and
/// centred in it — in half blocks always, and where the terminal draws
/// images, as the [`Placement`] the frame lays over them once it is out.
pub(super) fn render_picture(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    heading: &str,
    picture: &Picture,
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
    let placement = graphics::cell_pixels().and_then(|cell| Placement::fit(picture, body, cell));
    let room = placement.as_ref().map_or(body, |placement| placement.cells);
    render_half_blocks(frame, room, picture);
    placement
}

/// `picture` in half blocks, centred in `room`.
fn render_half_blocks(frame: &mut ratatui::Frame<'_>, room: Rect, picture: &Picture) {
    let Some((columns, rows)) = fitted(picture, room.width, room.height) else {
        return;
    };
    let left = room.x + (room.width - columns) / 2;
    let top = room.y + (room.height - rows.div_ceil(2)) / 2;
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
                cell.set_symbol(UPPER_HALF)
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

    #[test]
    fn a_picture_with_no_room_draws_nothing() {
        assert_eq!(fitted(&picture(10, 10), 0, 5), None);
    }
}
