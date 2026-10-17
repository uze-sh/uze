//! Images at the terminal's own resolution, where it can draw them.
//!
//! A cell holds two pixels in half blocks, which is enough to recognise an
//! image and not enough to read one. A terminal that speaks Sixel draws
//! real pixels, so where one says it does, the frame's image is sent again
//! as Sixel over the half blocks that stand in for it. The half blocks stay
//! drawn underneath: they are what shows on any other terminal, and the
//! mark that tells whether anything was drawn over the image this frame.
//!
//! Encoded here rather than by a crate: Sixel is a palette and runs of
//! six-pixel columns, and the one thing worth care — choosing the palette —
//! is a median cut of a few dozen lines.

use std::sync::{
    Arc, OnceLock,
    mpsc::{Receiver, Sender},
};
use std::time::Duration;

use ratatui::{buffer::Buffer, layout::Rect};
use uze_extensions::view::Picture;

use super::theme;

/// What the terminal answered about drawing images: the pixels in one
/// cell, when it speaks Sixel and said how large its cells are.
static CELL_PIXELS: OnceLock<Option<(u16, u16)>> = OnceLock::new();

/// Long enough for a reply over SSH; a terminal that answers DA1 at all
/// ends the wait as soon as it does.
const PATIENCE: Duration = Duration::from_millis(500);

/// The upper half block: its glyph paints the top pixel and the cell's
/// ground the bottom one, so every terminal with true colour shows two
/// pixels per cell without a graphics protocol. Also the mark
/// [`Placement::uncovered_in`] looks for, which is why there is one.
pub(crate) const HALF_BLOCK: &str = "\u{2580}";

/// Asks the terminal whether it draws Sixel and how large a cell is.
/// Before anything reads the terminal's input, which would take the reply
/// for keystrokes.
pub(crate) fn probe() {
    use std::io::IsTerminal as _;
    let answer = (std::io::stdin().is_terminal() && std::io::stdout().is_terminal())
        .then(|| {
            uze_platform::stdio::ask_terminal(b"\x1b[16t\x1b[c", &device_attributes_in, PATIENCE)
        })
        .flatten()
        .and_then(|reply| {
            let sixel = speaks_sixel(&reply);
            let cell = cell_size_in(&reply).or_else(cell_size_from_window);
            tracing::info!(sixel, ?cell, "terminal graphics");
            cell.filter(|_| sixel)
        });
    let _ = CELL_PIXELS.set(answer);
}

/// The pixels in one cell, `(width, height)`, when images can be drawn.
pub(crate) fn cell_pixels() -> Option<(u16, u16)> {
    CELL_PIXELS.get().copied().flatten()
}

fn device_attributes(reply: &[u8]) -> Option<&[u8]> {
    const LEAD: &[u8] = b"\x1b[?";
    let start = reply.windows(LEAD.len()).position(|w| w == LEAD)? + LEAD.len();
    let tail = &reply[start..];
    let length = tail
        .iter()
        .take_while(|byte| byte.is_ascii_digit() || **byte == b';')
        .count();
    (tail.get(length) == Some(&b'c')).then(|| &tail[..length])
}

fn device_attributes_in(reply: &[u8]) -> bool {
    device_attributes(reply).is_some()
}

/// DA1 lists `4` among its attributes for a terminal that draws Sixel.
fn speaks_sixel(reply: &[u8]) -> bool {
    device_attributes(reply).is_some_and(|attributes| {
        attributes
            .split(|byte| *byte == b';')
            .skip(1)
            .any(|attribute| attribute == b"4")
    })
}

/// `ESC [ 6 ; height ; width t`, the answer to `CSI 16 t`.
fn cell_size_in(reply: &[u8]) -> Option<(u16, u16)> {
    const LEAD: &[u8] = b"\x1b[6;";
    let start = reply.windows(LEAD.len()).position(|w| w == LEAD)? + LEAD.len();
    let tail = &reply[start..];
    let end = tail.iter().position(|byte| *byte == b't')?;
    let text = std::str::from_utf8(&tail[..end]).ok()?;
    let (height, width) = text.split_once(';')?;
    let (width, height) = (width.parse().ok()?, height.parse().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

/// The window's size in pixels over its size in cells, for a terminal
/// that reports the one through the kernel but not the other in reply.
fn cell_size_from_window() -> Option<(u16, u16)> {
    let size = crossterm::terminal::window_size().ok()?;
    (size.width > 0 && size.height > 0 && size.columns > 0 && size.rows > 0)
        .then(|| (size.width / size.columns, size.height / size.rows))
        .filter(|(width, height)| *width > 0 && *height > 0)
}

/// An image the frame drew in half blocks, to be drawn again in pixels.
#[derive(Clone, Debug)]
pub(crate) struct Placement {
    /// The cells the image covers, all of them half blocks when nothing
    /// was drawn over it.
    pub(crate) cells: Rect,
    /// Its size in pixels, within `cells`.
    pixels: (u16, u16),
    picture: Picture,
    /// The whole frame it was drawn in. A frame of another size is drawn
    /// on a cleared screen, which took the pixels with it even where the
    /// image lands on the same cells.
    screen: Rect,
}

impl Placement {
    /// `picture` fitted to `room` at `cell` pixels a cell, keeping its
    /// proportions, centred by whole cells.
    pub(crate) fn fit(picture: &Picture, room: Rect, cell: (u16, u16)) -> Option<Self> {
        if picture.width == 0 || picture.height == 0 || room.width == 0 || room.height == 0 {
            return None;
        }
        let (cell_width, cell_height) = (usize::from(cell.0), usize::from(cell.1));
        let space = (
            usize::from(room.width) * cell_width,
            usize::from(room.height) * cell_height,
        );
        // Never past its own size: one source pixel spread over several
        // is a blur, and a picture shown at its own size is the sharpest
        // it can be.
        let space = (space.0.min(picture.width), space.1.min(picture.height));
        let (width, height) = match space.0 * picture.height <= space.1 * picture.width {
            true => (space.0, (picture.height * space.0 / picture.width).max(1)),
            false => ((picture.width * space.1 / picture.height).max(1), space.1),
        };
        let columns = u16::try_from(width.div_ceil(cell_width)).ok()?;
        let rows = u16::try_from(height.div_ceil(cell_height)).ok()?;
        Some(Self {
            cells: Rect::new(
                room.x + (room.width - columns.min(room.width)) / 2,
                room.y + (room.height - rows.min(room.height)) / 2,
                columns.min(room.width),
                rows.min(room.height),
            ),
            pixels: (u16::try_from(width).ok()?, u16::try_from(height).ok()?),
            picture: picture.clone(),
            screen: Rect::default(),
        })
    }

    /// This placement as drawn in a frame of `screen`.
    pub(crate) fn on(self, screen: Rect) -> Self {
        Self { screen, ..self }
    }

    /// Whether every cell it covers still holds a half block — nothing,
    /// a menu or a toast, was drawn over it after it.
    pub(crate) fn uncovered_in(&self, buffer: &Buffer) -> bool {
        (self.cells.y..self.cells.bottom()).all(|y| {
            (self.cells.x..self.cells.right()).all(|x| {
                buffer
                    .cell((x, y))
                    .is_some_and(|cell| cell.symbol() == HALF_BLOCK)
            })
        })
    }

    fn identity(&self) -> Identity {
        Identity {
            pixels: self.picture.pixels.clone(),
            cells: self.cells,
            size: self.pixels,
            screen: self.screen,
            theme: uze_theme::generation(),
        }
    }
}

/// What makes two frames' images the same image on screen.
///
/// The pixels by their allocation, held rather than remembered as an
/// address: an address alone is free to be reused by the next image of
/// the same size, which would then pass for the one already shown. The
/// theme is part of it because a transparent pixel is drawn over its
/// ground, so a new theme redraws the half blocks under the pixels.
#[derive(Clone)]
struct Identity {
    pixels: Arc<[[u8; 4]]>,
    cells: Rect,
    size: (u16, u16),
    screen: Rect,
    theme: u64,
}

impl PartialEq for Identity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pixels, &other.pixels)
            && (self.cells, self.size, self.screen, self.theme)
                == (other.cells, other.size, other.screen, other.theme)
    }
}

/// What was last sent as pixels, so a frame that changed nothing about
/// the image sends nothing — and the encoding of the one wanted next,
/// made on a thread of its own.
///
/// Resampling and encoding a pane-sized image is tens of milliseconds,
/// which the frame may not wait for. So a frame only says which image it
/// left uncovered; the bytes are written when they arrive, if that image
/// is still the one on screen and uncovered, and dropped otherwise.
pub(crate) struct Painter {
    /// What the terminal shows over the half blocks, as far as anything
    /// written since says.
    shown: Option<Identity>,
    /// What the last frame left uncovered and the terminal does not show
    /// yet.
    wanted: Option<Identity>,
    /// The encode out, so a frame asking for the same image again does not
    /// start a second one.
    encoding: Option<Identity>,
    /// The last encode that came back, kept so an image covered and then
    /// uncovered is written again without being made again.
    encoded: Option<(Identity, Arc<Vec<u8>>)>,
    /// The one thread that encodes, started with the first image.
    encoder: Option<Sender<(Identity, Placement)>>,
    sender: Sender<(Identity, Arc<Vec<u8>>)>,
    answers: Receiver<(Identity, Arc<Vec<u8>>)>,
}

impl Default for Painter {
    fn default() -> Self {
        let (sender, answers) = std::sync::mpsc::channel();
        Self {
            shown: None,
            wanted: None,
            encoding: None,
            encoded: None,
            encoder: None,
            sender,
            answers,
        }
    }
}

impl Painter {
    /// Records the image a frame left uncovered, `placement`, and returns
    /// its bytes at once when they are already made.
    ///
    /// A frame that covered the image or drew none forgets what was
    /// shown, because whatever covered it wrote over those pixels, and
    /// the half blocks drawn back when it went are all that is left.
    pub(crate) fn after_frame(&mut self, placement: Option<&Placement>) -> Option<Arc<Vec<u8>>> {
        let Some(placement) = placement else {
            self.shown = None;
            self.wanted = None;
            return None;
        };
        let identity = placement.identity();
        if self.shown.as_ref() == Some(&identity) {
            return None;
        }
        self.wanted = Some(identity.clone());
        if let Some(bytes) = self.ready() {
            return Some(bytes);
        }
        if self.encoding.as_ref() != Some(&identity) {
            self.encoding = Some(identity.clone());
            let answers = self.sender.clone();
            let encoder = self.encoder.get_or_insert_with(|| encoder(answers));
            let _ = encoder.send((identity, placement.clone()));
        }
        None
    }

    /// The bytes to write now, when an encode has come back for the image
    /// the last frame left uncovered. Asked between frames.
    pub(crate) fn take_ready(&mut self) -> Option<Arc<Vec<u8>>> {
        while let Ok(answer) = self.answers.try_recv() {
            self.absorb(answer);
        }
        self.ready()
    }

    /// Keeps an answer only when it is for the encode last asked for: one
    /// that started earlier is for an image already replaced, whichever
    /// order the two threads finish in.
    fn absorb(&mut self, (identity, bytes): (Identity, Arc<Vec<u8>>)) {
        if self.encoding.as_ref() != Some(&identity) {
            return;
        }
        self.encoding = None;
        self.encoded = Some((identity, bytes));
    }

    fn ready(&mut self) -> Option<Arc<Vec<u8>>> {
        let (made, bytes) = self.encoded.as_ref()?;
        if self.wanted.as_ref() != Some(made) {
            return None;
        }
        let bytes = bytes.clone();
        self.shown = self.wanted.take();
        Some(bytes)
    }
}

/// Starts the thread that encodes, which answers on `answers`.
///
/// One thread, taking only the latest request it has been sent: moving
/// through a directory of images asks for one encode per selection, and a
/// thread each would run them all at once, every one for an image already
/// gone. The painter drops any answer but the last one asked for.
fn encoder(answers: Sender<(Identity, Arc<Vec<u8>>)>) -> Sender<(Identity, Placement)> {
    let (requests, received) = std::sync::mpsc::channel::<(Identity, Placement)>();
    std::thread::spawn(move || {
        while let Ok(first) = received.recv() {
            let (identity, placement) = received.try_iter().last().unwrap_or(first);
            if answers
                .send((identity, Arc::new(positioned(&placement))))
                .is_err()
            {
                return;
            }
        }
    });
    requests
}

/// The image as Sixel, written at its top-left cell with the cursor put
/// back where it was.
fn positioned(placement: &Placement) -> Vec<u8> {
    let (width, height) = placement.pixels;
    let pixels = resampled(&placement.picture, usize::from(width), usize::from(height));
    let mut out = format!(
        "\x1b7\x1b[{};{}H",
        placement.cells.y + 1,
        placement.cells.x + 1
    )
    .into_bytes();
    out.extend(sixel(&pixels, usize::from(width), usize::from(height)));
    out.extend_from_slice(b"\x1b8");
    out
}

/// `picture` laid over the theme's ground and resampled to `width` ×
/// `height` with a Lanczos-3 filter, one axis at a time.
///
/// Lanczos rather than an average of the pixels each one covers: an
/// average over a span that is one source pixel here and two there is
/// what makes the strokes of text uneven, and the windowed sinc keeps
/// them as sharp as the new size allows. Laid over the ground first, so
/// a transparent edge blends into what it is drawn on rather than into
/// whatever colour its invisible pixels happen to hold.
fn resampled(picture: &Picture, width: usize, height: usize) -> Vec<[u8; 3]> {
    let lay = theme::laid_over_ground();
    let columns = lanczos_weights(picture.width, width);
    let rows = lanczos_weights(picture.height, height);
    // One source row laid over the ground at a time, not the whole image:
    // as floats, a 4096-pixel square would be 200 MB held for one pass.
    let mut source = vec![[0f32; 3]; picture.width];
    let mut across = vec![[0f32; 3]; width * picture.height];
    for y in 0..picture.height {
        let row = &picture.pixels[y * picture.width..(y + 1) * picture.width];
        for (laid, pixel) in source.iter_mut().zip(row) {
            *laid = lay(*pixel).map(f32::from);
        }
        for (x, (first, weights)) in columns.iter().enumerate() {
            across[y * width + x] = blend(weights.iter().zip(&source[*first..]));
        }
    }
    let mut out = Vec::with_capacity(width * height);
    for (first, weights) in &rows {
        for x in 0..width {
            let column = (*first..).map(|y| &across[y * width + x]);
            let [red, green, blue] = blend(weights.iter().zip(column));
            out.push([red, green, blue].map(|channel| channel.round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

fn blend<'a>(taps: impl Iterator<Item = (&'a f32, &'a [f32; 3])>) -> [f32; 3] {
    taps.fold([0.0; 3], |sum, (weight, pixel)| {
        [
            sum[0] + weight * pixel[0],
            sum[1] + weight * pixel[1],
            sum[2] + weight * pixel[2],
        ]
    })
}

/// For each of `target` pixels along an axis of `source`, the first
/// source pixel it reads and the weight of each from there, summing to
/// one. Shrinking widens the filter by the same factor, so every source
/// pixel is heard rather than skipped.
fn lanczos_weights(source: usize, target: usize) -> Vec<(usize, Vec<f32>)> {
    const LOBES: f32 = 3.0;
    let kernel = |x: f32| {
        if x.abs() < f32::EPSILON {
            return 1.0;
        }
        if x.abs() >= LOBES {
            return 0.0;
        }
        let pi_x = std::f32::consts::PI * x;
        LOBES * pi_x.sin() * (pi_x / LOBES).sin() / (pi_x * pi_x)
    };
    let scale = source as f32 / target as f32;
    let stretch = scale.max(1.0);
    let reach = LOBES * stretch;
    (0..target)
        .map(|index| {
            let centre = (index as f32 + 0.5) * scale - 0.5;
            let first = ((centre - reach).ceil().max(0.0)) as usize;
            let last = ((centre + reach).floor() as usize).min(source - 1);
            let mut weights: Vec<f32> = (first..=last)
                .map(|at| kernel((at as f32 - centre) / stretch))
                .collect();
            let total: f32 = weights.iter().sum();
            if total.abs() > f32::EPSILON {
                weights.iter_mut().for_each(|weight| *weight /= total);
            }
            (first, weights)
        })
        .collect()
}

/// The pixel at `column`, `row` of the picture drawn `columns` × `rows`,
/// for half blocks: the average of an even sample of the source pixels it
/// covers, or the nearest one when it covers less than one.
pub(crate) fn sample(picture: &Picture, column: u16, row: u16, columns: u16, rows: u16) -> [u8; 4] {
    // At most this many taps a side: a cell standing for a hundred source
    // pixels is drawn on every frame, and an even sample of them looks the
    // same as all of them at two pixels a cell.
    const TAPS: usize = 8;
    let span = |index: u16, target: u16, source: usize| {
        let (index, target) = (usize::from(index), usize::from(target));
        let start = index * source / target;
        let end = ((index + 1) * source / target).clamp(start + 1, source);
        (start..end).step_by((end - start).div_ceil(TAPS))
    };
    let mut sum = [0u32; 4];
    let mut count = 0u32;
    for y in span(row, rows, picture.height) {
        for x in span(column, columns, picture.width) {
            for (total, channel) in sum.iter_mut().zip(picture.pixels[y * picture.width + x]) {
                *total += u32::from(channel);
            }
            count += 1;
        }
    }
    sum.map(|total| (total / count.max(1)) as u8)
}

/// The most colours one Sixel image carries; terminals agree on 256.
const PALETTE: usize = 256;

/// `pixels`, `width` × `height` row by row, as one Sixel image.
fn sixel(pixels: &[[u8; 3]], width: usize, height: usize) -> Vec<u8> {
    let (palette, index) = quantized(pixels);
    let mut out = format!("\x1bP0;1;0q\"1;1;{width};{height}").into_bytes();
    for (number, [red, green, blue]) in palette.iter().enumerate() {
        let percent = |channel: u8| (u32::from(channel) * 100 + 127) / 255;
        out.extend(
            format!(
                "#{number};2;{};{};{}",
                percent(*red),
                percent(*green),
                percent(*blue)
            )
            .bytes(),
        );
    }
    let mut columns = vec![vec![0u8; width]; palette.len()];
    let mut used = Vec::with_capacity(palette.len());
    let mut in_band = vec![false; palette.len()];
    for band in (0..height).step_by(6) {
        for row in band..(band + 6).min(height) {
            let bit = 1 << (row - band);
            for (x, pixel) in index[row * width..(row + 1) * width].iter().enumerate() {
                let colour = usize::from(*pixel);
                if !in_band[colour] {
                    in_band[colour] = true;
                    used.push(colour);
                }
                columns[colour][x] |= bit;
            }
        }
        for (position, colour) in used.iter().enumerate() {
            if position > 0 {
                out.push(b'$');
            }
            out.extend(format!("#{colour}").bytes());
            run_length(&mut out, &columns[*colour]);
            columns[*colour].fill(0);
            in_band[*colour] = false;
        }
        used.clear();
        out.push(b'-');
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

/// One colour's row of six-pixel columns, repeats folded into `!count`,
/// with the empty columns after the last one left out.
fn run_length(out: &mut Vec<u8>, columns: &[u8]) {
    let end = columns
        .iter()
        .rposition(|bits| *bits != 0)
        .map_or(0, |at| at + 1);
    let mut at = 0;
    while at < end {
        let bits = columns[at];
        let run = columns[at..end]
            .iter()
            .take_while(|next| **next == bits)
            .count();
        let glyph = b'?' + bits;
        match run {
            1..=3 => out.extend(std::iter::repeat_n(glyph, run)),
            _ => {
                out.extend(format!("!{run}").bytes());
                out.push(glyph);
            }
        }
        at += run;
    }
}

/// A palette of at most [`PALETTE`] colours for `pixels`, and each pixel's
/// place in it.
///
/// A median cut over colours bucketed at five bits a channel: the box
/// holding the widest spread is split at its median until there are
/// enough, and each box is drawn as the average of what fell in it. Flat
/// interfaces, which is what a screenshot of one mostly is, come out
/// exact; a photograph loses only what 256 colours cannot hold.
fn quantized(pixels: &[[u8; 3]]) -> (Vec<[u8; 3]>, Vec<u8>) {
    let key = |[red, green, blue]: [u8; 3]| {
        (usize::from(red >> 3) << 10) | (usize::from(green >> 3) << 5) | usize::from(blue >> 3)
    };
    let mut buckets = vec![(0u64, [0u64; 3]); 1 << 15];
    for pixel in pixels {
        let bucket = &mut buckets[key(*pixel)];
        bucket.0 += 1;
        for (sum, channel) in bucket.1.iter_mut().zip(pixel) {
            *sum += u64::from(*channel);
        }
    }
    let present: Vec<usize> = (0..buckets.len()).filter(|at| buckets[*at].0 > 0).collect();
    let mean = |members: &[usize]| {
        let (count, sums) = members.iter().fold((0u64, [0u64; 3]), |(count, sums), at| {
            let (n, s) = buckets[*at];
            (count + n, [sums[0] + s[0], sums[1] + s[1], sums[2] + s[2]])
        });
        sums.map(|sum| (sum / count.max(1)) as u8)
    };
    let channel = |at: usize, axis: usize| (at >> (10 - axis * 5)) & 31;
    let mut boxes: Vec<Vec<usize>> = vec![present];
    while boxes.len() < PALETTE {
        let widest = boxes
            .iter()
            .enumerate()
            .filter(|(_, members)| members.len() > 1)
            .map(|(at, members)| {
                let spread = (0..3)
                    .map(|axis| {
                        let values = members.iter().map(|member| channel(*member, axis));
                        let (low, high) = values.fold((31, 0), |(low, high), value| {
                            (low.min(value), high.max(value))
                        });
                        (high - low, axis)
                    })
                    .max()
                    .unwrap_or((0, 0));
                (spread, at)
            })
            .max();
        let Some(((spread, axis), at)) = widest else {
            break;
        };
        if spread == 0 {
            break;
        }
        let mut members = boxes.swap_remove(at);
        members.sort_by_key(|member| channel(*member, axis));
        let total: u64 = members.iter().map(|member| buckets[*member].0).sum();
        let mut seen = 0;
        let cut = members
            .iter()
            .position(|member| {
                seen += buckets[*member].0;
                seen * 2 >= total
            })
            .map_or(1, |at| (at + 1).clamp(1, members.len() - 1));
        let upper = members.split_off(cut);
        boxes.push(members);
        boxes.push(upper);
    }
    let palette: Vec<[u8; 3]> = boxes.iter().map(|members| mean(members)).collect();
    let mut place = vec![0u8; 1 << 15];
    for (number, members) in boxes.iter().enumerate() {
        for member in members {
            place[*member] = number as u8;
        }
    }
    let index = pixels.iter().map(|pixel| place[key(*pixel)]).collect();
    (palette, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_image_resamples_to_the_same_colour_at_any_size() {
        let picture = Picture {
            width: 7,
            height: 5,
            pixels: vec![[120, 60, 30, 255]; 35].into(),
        };
        for (width, height) in [(3, 2), (7, 5), (4, 9)] {
            let pixels = resampled(&picture, width, height);
            assert_eq!(pixels.len(), width * height);
            assert!(
                pixels.iter().all(|pixel| *pixel == [120, 60, 30]),
                "{pixels:?}"
            );
        }
    }

    #[test]
    fn a_picture_smaller_than_the_room_is_shown_at_its_own_size() {
        let picture = Picture {
            width: 50,
            height: 20,
            pixels: vec![[0, 0, 0, 255]; 1000].into(),
        };
        let placement = Placement::fit(&picture, Rect::new(0, 0, 40, 40), (10, 20)).unwrap();
        assert_eq!(placement.pixels, (50, 20));
        assert_eq!((placement.cells.width, placement.cells.height), (5, 1));
    }

    #[test]
    fn a_terminal_that_lists_attribute_four_draws_sixel() {
        assert!(speaks_sixel(b"\x1b[?61;4;6;7;22c"));
        assert!(!speaks_sixel(b"\x1b[?62;22c"));
        assert!(
            !speaks_sixel(b"\x1b[?4c"),
            "the first value is the class, not an attribute"
        );
    }

    #[test]
    fn the_cell_size_reply_is_height_then_width() {
        assert_eq!(cell_size_in(b"\x1b[6;20;10t\x1b[?62;4c"), Some((10, 20)));
        assert_eq!(cell_size_in(b"\x1b[?62;4c"), None);
    }

    #[test]
    fn an_image_is_fitted_by_its_pixels_and_centred_by_whole_cells() {
        let picture = Picture {
            width: 800,
            height: 400,
            pixels: vec![[0, 0, 0, 255]; 800 * 400].into(),
        };
        let placement = Placement::fit(&picture, Rect::new(0, 0, 40, 40), (10, 20)).unwrap();
        assert_eq!(placement.pixels, (400, 200));
        assert_eq!(placement.cells, Rect::new(0, 15, 40, 10));
    }

    #[test]
    fn a_few_colours_are_kept_exactly() {
        let pixels = [[10, 20, 30], [200, 100, 50], [10, 20, 30]];
        let (palette, index) = quantized(&pixels);
        assert_eq!(palette.len(), 2);
        assert_eq!(index[0], index[2]);
        assert_ne!(index[0], index[1]);
        assert_eq!(palette[usize::from(index[1])], [200, 100, 50]);
    }

    #[test]
    fn many_colours_are_cut_to_one_palette() {
        let pixels: Vec<[u8; 3]> = (0..=255u8)
            .flat_map(|red| (0..=255u8).step_by(5).map(move |green| [red, green, 128]))
            .collect();
        let (palette, index) = quantized(&pixels);
        assert!(palette.len() <= PALETTE);
        assert!(index.iter().all(|at| usize::from(*at) < palette.len()));
    }

    #[test]
    fn repeats_fold_into_a_count_and_trailing_blanks_are_left_out() {
        let mut out = Vec::new();
        run_length(&mut out, &[1, 1, 1, 1, 1, 2, 0, 0]);
        assert_eq!(out, b"!5@A");
    }

    /// Waits for the encode last asked for, by receiving answers until it
    /// lands, never by polling.
    fn settle(painter: &mut Painter) {
        while painter.encoding.is_some() {
            let answer = painter.answers.recv().unwrap();
            painter.absorb(answer);
        }
    }

    fn placement_of(side: usize, room: Rect) -> Placement {
        let picture = Picture {
            width: side,
            height: side,
            pixels: vec![[255, 255, 255, 255]; side * side].into(),
        };
        Placement::fit(&picture, room, (10, 20)).unwrap()
    }

    #[test]
    fn a_frame_that_changed_nothing_about_the_image_sends_nothing() {
        let placement = placement_of(2, Rect::new(0, 0, 4, 4));
        let mut painter = Painter::default();
        assert!(painter.after_frame(Some(&placement)).is_none(), "encoding");
        settle(&mut painter);
        assert!(painter.take_ready().is_some(), "written once made");
        assert!(painter.after_frame(Some(&placement)).is_none());
        assert!(painter.take_ready().is_none());
        assert!(painter.after_frame(None).is_none(), "covered");
        assert!(
            painter.after_frame(Some(&placement)).is_some(),
            "written again once uncovered, without being made again"
        );
    }

    #[test]
    fn an_encode_for_an_image_no_longer_shown_is_never_written() {
        let old = placement_of(2, Rect::new(0, 0, 4, 4));
        let new = placement_of(3, Rect::new(0, 0, 4, 4));
        let mut painter = Painter::default();
        painter.after_frame(Some(&old));
        painter.after_frame(Some(&new));
        settle(&mut painter);
        let written = painter
            .take_ready()
            .expect("the image on screen is written");
        assert_eq!(*written, positioned(&new));
        assert!(painter.take_ready().is_none());
    }

    /// A picture read again — a screenshot an agent regenerated at the
    /// same size — is a new image even with every pixel equal, and is sent.
    #[test]
    fn the_same_size_image_read_again_is_sent_again() {
        let room = Rect::new(0, 0, 4, 4);
        let mut painter = Painter::default();
        let first = placement_of(2, room);
        painter.after_frame(Some(&first));
        settle(&mut painter);
        assert!(painter.take_ready().is_some());
        drop(first);
        let second = placement_of(2, room);
        assert!(painter.after_frame(Some(&second)).is_none(), "encoding");
        settle(&mut painter);
        assert!(
            painter.take_ready().is_some(),
            "sent, not taken for the first"
        );
    }

    /// A frame of a new size is drawn on a cleared screen, so the image is
    /// sent again even when it lands on the very same cells.
    #[test]
    fn a_resized_screen_sends_the_image_again_on_the_same_cells() {
        let placement = placement_of(2, Rect::new(0, 0, 4, 4));
        let mut painter = Painter::default();
        let before = placement.clone().on(Rect::new(0, 0, 80, 24));
        painter.after_frame(Some(&before));
        settle(&mut painter);
        assert!(painter.take_ready().is_some());
        let after = placement.on(Rect::new(0, 0, 80, 25));
        assert_eq!(before.cells, after.cells);
        painter.after_frame(Some(&after));
        settle(&mut painter);
        assert!(painter.take_ready().is_some(), "sent again after the clear");
    }

    #[test]
    fn an_encode_that_lands_while_the_image_is_covered_waits_for_it_to_be_uncovered() {
        let placement = placement_of(2, Rect::new(0, 0, 4, 4));
        let mut painter = Painter::default();
        painter.after_frame(Some(&placement));
        painter.after_frame(None);
        settle(&mut painter);
        assert!(painter.take_ready().is_none(), "never over what covers it");
        assert!(painter.after_frame(Some(&placement)).is_some());
    }
}
