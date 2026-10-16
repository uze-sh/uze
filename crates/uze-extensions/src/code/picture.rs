//! An image file, decoded into the pixels the host fits to a pane.
//!
//! Decoded here rather than by the host because which formats a surface
//! can show is the surface's to say; the host only ever sees RGBA.

use std::path::Path;

use crate::view::Picture;

/// The most pixels a decoded image keeps on either side.
///
/// Well past any pane, because the host resamples once more to fit it and
/// every resampling before that one only blurs: this bounds the memory a
/// very large image holds for the life of the open file, nothing else.
const LARGEST_SIDE: usize = 4096;

/// Whether `path` names a format [`decode`] reads.
pub(super) fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

/// The image in `bytes`, shrunk to [`LARGEST_SIDE`], or `None` for bytes
/// that are not one — a file named `.png` is only a claim.
pub(super) fn decode(bytes: &[u8]) -> Option<Picture> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let samples = &buffer[..frame.buffer_size()];
    let pixels: Vec<[u8; 4]> = match frame.color_type {
        png::ColorType::Rgba => samples.as_chunks::<4>().0.to_vec(),
        png::ColorType::Rgb => samples
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[r, g, b]| [r, g, b, u8::MAX])
            .collect(),
        png::ColorType::GrayscaleAlpha => samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[v, a]| [v, v, v, a])
            .collect(),
        png::ColorType::Grayscale => samples.iter().map(|&v| [v, v, v, u8::MAX]).collect(),
        png::ColorType::Indexed => return None,
    };
    let (width, height) = (frame.width as usize, frame.height as usize);
    (pixels.len() == width * height).then(|| shrunk(width, height, pixels, LARGEST_SIDE))
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

    #[test]
    fn an_image_arrives_as_rgba_whatever_it_was_stored_as() {
        let gray = decode(&encoded(2, 1, png::ColorType::Grayscale, &[0, 255])).unwrap();
        assert_eq!(*gray.pixels, [[0, 0, 0, 255], [255, 255, 255, 255]]);

        let rgb = decode(&encoded(1, 1, png::ColorType::Rgb, &[10, 20, 30])).unwrap();
        assert_eq!((rgb.width, rgb.height), (1, 1));
        assert_eq!(*rgb.pixels, [[10, 20, 30, 255]]);
    }

    #[test]
    fn a_file_that_only_claims_to_be_an_image_is_none() {
        assert_eq!(decode(b"not a png at all"), None);
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
    fn only_a_png_is_taken_for_a_picture() {
        assert!(is_picture(Path::new("docs/logo.PNG")));
        assert!(!is_picture(Path::new("docs/logo.svg")));
    }
}
