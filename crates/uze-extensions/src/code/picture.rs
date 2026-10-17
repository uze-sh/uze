//! An image file, decoded into the pixels the host fits to a pane.
//!
//! Decoded here rather than by the host because which formats a surface
//! can show is the surface's to say; the host only ever sees RGBA. Every
//! decoder is pure Rust: PNG, GIF and WebP from image-rs, JPEG from
//! zune-jpeg, and BMP and ICO here, being a header and rows.

use std::path::Path;

use crate::view::Picture;

mod bmp;
mod ico;

/// The most pixels a decoded image keeps on either side.
///
/// Well past any pane, because the host resamples once more to fit it and
/// every resampling before that one only blurs: this bounds the memory a
/// very large image holds for the life of the open file, nothing else.
const LARGEST_SIDE: usize = 4096;

/// The most pixels an image may have to be decoded at all. Checked from
/// its header, before the full-size buffer is allocated: a file of a few
/// kilobytes can claim to be a hundred thousand pixels a side.
const MOST_PIXELS: usize = 64 * 1024 * 1024;

/// An image as a decoder hands it over, before it is shrunk.
struct Decoded {
    width: usize,
    height: usize,
    pixels: Vec<[u8; 4]>,
}

impl Decoded {
    fn new(width: usize, height: usize, pixels: Vec<[u8; 4]>) -> Option<Self> {
        (width > 0 && height > 0 && pixels.len() == width * height).then_some(Self {
            width,
            height,
            pixels,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
    Bmp,
    Ico,
}

/// The format `path` claims by its extension, if it is one [`decode`]
/// reads.
fn format_of(path: &Path) -> Option<Format> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => Format::Png,
        "jpg" | "jpeg" => Format::Jpeg,
        "gif" => Format::Gif,
        "webp" => Format::WebP,
        "bmp" => Format::Bmp,
        "ico" => Format::Ico,
        _ => return None,
    })
}

/// Whether `path` names a format [`decode`] reads.
pub(super) fn is_picture(path: &Path) -> bool {
    format_of(path).is_some()
}

/// The image in `bytes`, read as the format `path` names and shrunk to
/// [`LARGEST_SIDE`], or `None` for bytes that are not one — an extension
/// is only a claim.
pub(super) fn decode(path: &Path, bytes: &[u8]) -> Option<Picture> {
    let decoded = match format_of(path)? {
        Format::Png => png(bytes),
        Format::Jpeg => jpeg(bytes),
        Format::Gif => gif(bytes),
        Format::WebP => webp(bytes),
        Format::Bmp => bmp::decode(bytes),
        Format::Ico => ico::decode(bytes),
    }?;
    Some(shrunk(
        decoded.width,
        decoded.height,
        decoded.pixels,
        LARGEST_SIDE,
    ))
}

fn affordable(width: usize, height: usize) -> bool {
    width
        .checked_mul(height)
        .is_some_and(|pixels| pixels <= MOST_PIXELS)
}

fn png(bytes: &[u8]) -> Option<Decoded> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if !affordable(info.width as usize, info.height as usize) {
        return None;
    }
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let samples = &buffer[..frame.buffer_size()];
    let pixels = match frame.color_type {
        png::ColorType::Rgba => from_channels(samples, 4),
        png::ColorType::Rgb => from_channels(samples, 3),
        png::ColorType::GrayscaleAlpha => from_channels(samples, 2),
        png::ColorType::Grayscale => from_channels(samples, 1),
        png::ColorType::Indexed => return None,
    };
    Decoded::new(frame.width as usize, frame.height as usize, pixels)
}

fn jpeg(bytes: &[u8]) -> Option<Decoded> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().ok()?;
    let (width, height) = decoder.dimensions()?;
    if !affordable(width, height) {
        return None;
    }
    let samples = decoder.decode().ok()?;
    // The decoder may keep a colour space other than the one asked for —
    // grayscale stays grayscale — so its own answer says how to read it.
    let components = decoder.output_colorspace()?.num_components();
    Decoded::new(width, height, from_channels(&samples, components))
}

/// The first frame, laid on the image's own canvas: a frame may cover only
/// part of it, and what it leaves uncovered is transparent.
fn gif(bytes: &[u8]) -> Option<Decoded> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = (usize::from(decoder.width()), usize::from(decoder.height()));
    if !affordable(width, height) {
        return None;
    }
    let frame = decoder.read_next_frame().ok()??;
    let mut pixels = vec![[0u8; 4]; width * height];
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let frame_width = usize::from(frame.width);
    for (row, line) in from_channels(&frame.buffer, 4)
        .chunks(frame_width.max(1))
        .enumerate()
    {
        let y = top + row;
        if y >= height {
            break;
        }
        for (column, pixel) in line.iter().enumerate() {
            let x = left + column;
            if x < width {
                pixels[y * width + x] = *pixel;
            }
        }
    }
    Decoded::new(width, height, pixels)
}

/// Lossy and lossless alike; an animation's first frame.
fn webp(bytes: &[u8]) -> Option<Decoded> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    let (width, height) = (width as usize, height as usize);
    if !affordable(width, height) {
        return None;
    }
    let mut samples = vec![0; decoder.output_buffer_size()?];
    decoder.read_image(&mut samples).ok()?;
    let channels = if decoder.has_alpha() { 4 } else { 3 };
    Decoded::new(width, height, from_channels(&samples, channels))
}

/// Interleaved 8-bit samples of 1 (gray), 2 (gray, alpha), 3 (RGB) or 4
/// (RGBA) channels as RGBA. Any other count is no image this reads.
fn from_channels(samples: &[u8], channels: usize) -> Vec<[u8; 4]> {
    match channels {
        4 => samples.as_chunks::<4>().0.to_vec(),
        3 => samples
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[r, g, b]| [r, g, b, u8::MAX])
            .collect(),
        2 => samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[v, a]| [v, v, v, a])
            .collect(),
        1 => samples.iter().map(|&v| [v, v, v, u8::MAX]).collect(),
        _ => Vec::new(),
    }
}

/// The image no larger than `largest` on either side, each kept pixel the
/// average of the ones it stands for.
fn shrunk(
    source_width: usize,
    source_height: usize,
    source: Vec<[u8; 4]>,
    largest: usize,
) -> Picture {
    let longest = source_width.max(source_height);
    if longest <= largest {
        return Picture {
            width: source_width,
            height: source_height,
            pixels: source.into(),
        };
    }
    let width = (source_width * largest / longest).max(1);
    let height = (source_height * largest / longest).max(1);
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        let rows = span(y, height, source_height);
        for x in 0..width {
            let columns = span(x, width, source_width);
            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for source_y in rows.clone() {
                for source_x in columns.clone() {
                    let pixel = source[source_y * source_width + source_x];
                    for (total, channel) in sum.iter_mut().zip(pixel) {
                        *total += u64::from(channel);
                    }
                    count += 1;
                }
            }
            pixels.push(sum.map(|total| (total / count) as u8));
        }
    }
    Picture {
        width,
        height,
        pixels: pixels.into(),
    }
}

/// The source indices the `index`th of `target` cells covers, out of
/// `source` — never empty.
fn span(index: usize, target: usize, source: usize) -> std::ops::Range<usize> {
    let start = index * source / target;
    let end = ((index + 1) * source / target).max(start + 1);
    start..end.min(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(width: u32, height: u32, color: png::ColorType, samples: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(samples).unwrap();
        writer.finish().unwrap();
        bytes
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../tests/_fixtures/images/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    /// Lossy formats only promise a colour near the one written.
    fn near(pixel: [u8; 4], expected: [u8; 4]) -> bool {
        pixel
            .iter()
            .zip(expected)
            .all(|(got, want)| got.abs_diff(want) <= 6)
    }

    #[test]
    fn a_png_arrives_as_rgba_whatever_it_was_stored_as() {
        let path = Path::new("a.png");
        let gray = decode(path, &encoded(2, 1, png::ColorType::Grayscale, &[0, 255])).unwrap();
        assert_eq!(*gray.pixels, [[0, 0, 0, 255], [255, 255, 255, 255]]);

        let rgb = decode(path, &encoded(1, 1, png::ColorType::Rgb, &[10, 20, 30])).unwrap();
        assert_eq!((rgb.width, rgb.height), (1, 1));
        assert_eq!(*rgb.pixels, [[10, 20, 30, 255]]);
    }

    #[test]
    fn a_jpeg_in_colour_or_in_gray_arrives_as_rgba() {
        let colour = decode(Path::new("rust.jpg"), &fixture("rust.jpg")).unwrap();
        assert_eq!((colour.width, colour.height), (16, 8));
        assert!(
            colour
                .pixels
                .iter()
                .all(|p| near(*p, [0xC8, 0x32, 0x14, 255])),
            "{:?}",
            colour.pixels[0]
        );

        let gray = decode(Path::new("gray.JPEG"), &fixture("gray.jpg")).unwrap();
        assert!(
            gray.pixels
                .iter()
                .all(|p| near(*p, [0x80, 0x80, 0x80, 255])),
            "{:?}",
            gray.pixels[0]
        );
    }

    #[test]
    fn a_gif_is_its_first_frame_on_its_own_canvas() {
        let mut bytes = Vec::new();
        {
            let palette = [255, 0, 0, 0, 0, 255];
            let mut encoder = gif::Encoder::new(&mut bytes, 2, 2, &palette).unwrap();
            // The first frame covers only the top-left pixel: red.
            let mut first = gif::Frame::default();
            (first.width, first.height) = (1, 1);
            first.buffer = std::borrow::Cow::Owned(vec![0]);
            encoder.write_frame(&first).unwrap();
            let mut second = gif::Frame::default();
            (second.width, second.height) = (2, 2);
            second.buffer = std::borrow::Cow::Owned(vec![1; 4]);
            encoder.write_frame(&second).unwrap();
        }
        let picture = decode(Path::new("spinner.gif"), &bytes).unwrap();
        assert_eq!((picture.width, picture.height), (2, 2));
        assert_eq!(picture.pixels[0], [255, 0, 0, 255]);
        assert_eq!(
            picture.pixels[3][3], 0,
            "what the first frame leaves is transparent"
        );
    }

    #[test]
    fn a_webp_arrives_as_rgba_with_its_alpha() {
        let mut bytes = Vec::new();
        image_webp::WebPEncoder::new(&mut bytes)
            .encode(
                &[10, 20, 30, 40, 50, 60, 70, 255],
                2,
                1,
                image_webp::ColorType::Rgba8,
            )
            .unwrap();
        let picture = decode(Path::new("icon.webp"), &bytes).unwrap();
        assert_eq!(*picture.pixels, [[10, 20, 30, 40], [50, 60, 70, 255]]);
    }

    #[test]
    fn a_bitmap_and_an_icon_are_read_by_their_extensions() {
        let rows = [0, 0, 255, 0];
        let bitmap = bmp::tests::file(1, 1, 24, 0, &[], &rows);
        assert_eq!(
            *decode(Path::new("a.bmp"), &bitmap).unwrap().pixels,
            [[255, 0, 0, 255]]
        );
        assert!(
            decode(Path::new("a.ico"), &bitmap).is_none(),
            "a bitmap is not an icon"
        );
    }

    #[test]
    fn a_file_that_only_claims_to_be_an_image_is_none() {
        for name in ["a.png", "a.jpg", "a.gif", "a.webp", "a.bmp", "a.ico"] {
            assert_eq!(
                decode(Path::new(name), b"not an image at all"),
                None,
                "{name}"
            );
        }
    }

    #[test]
    fn a_header_claiming_more_pixels_than_affordable_is_refused_before_decoding() {
        assert!(affordable(4096, 4096));
        assert!(!affordable(100_000, 100_000));
        assert!(!affordable(usize::MAX, 2));
    }

    #[test]
    fn an_image_larger_than_the_cap_is_kept_at_the_cap() {
        let picture = shrunk(16, 8, vec![[200, 200, 200, 255]; 16 * 8], 8);
        assert_eq!((picture.width, picture.height), (8, 4));
        assert!(
            picture
                .pixels
                .iter()
                .all(|&pixel| pixel == [200, 200, 200, 255])
        );
    }

    #[test]
    fn the_common_raster_formats_are_pictures_and_nothing_else() {
        for name in [
            "a.png", "a.PNG", "a.jpg", "a.JPEG", "a.gif", "a.webp", "a.bmp", "a.ico",
        ] {
            assert!(is_picture(Path::new(name)), "{name}");
        }
        for name in ["a.svg", "a.avif", "a.tiff", "png", "a.png.txt"] {
            assert!(!is_picture(Path::new(name)), "{name}");
        }
    }
}
