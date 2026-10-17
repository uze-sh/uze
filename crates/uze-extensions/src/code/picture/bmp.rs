//! BMP: a file header, a DIB header, maybe a palette, and rows.
//!
//! Read here rather than through a crate because the bitmaps people still
//! commit are the uncompressed kinds, which are a few dozen lines. What is
//! read: 1, 4 and 8 bits through a palette, 16 and 32 bits through bit
//! masks, and 24 bits, bottom-up or top-down. Run-length compression and
//! embedded JPEG or PNG are not, and come back as `None`.

use super::{Decoded, affordable};

const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const BI_ALPHABITFIELDS: u32 = 6;

/// The DIB header every reader since Windows 3 understands; the later
/// ones extend it without moving anything in it.
const INFO_HEADER: usize = 40;

/// A `.bmp` file.
pub(super) fn decode(bytes: &[u8]) -> Option<Decoded> {
    if bytes.get(..2)? != b"BM" {
        return None;
    }
    let pixels_at = usize::try_from(u32_at(bytes, 10)?).ok()?.checked_sub(14)?;
    dib(bytes.get(14..)?, Some(pixels_at), false)
}

/// A bitmap stored inside an icon: no file header, its height counting
/// the colour rows and the transparency mask after them.
pub(super) fn icon_entry(bytes: &[u8]) -> Option<Decoded> {
    dib(bytes, None, true)
}

/// The image in a DIB — a header, then its palette or masks, then rows.
/// `pixels_at` is where the rows start when a file header said so;
/// otherwise they follow the palette.
fn dib(dib: &[u8], pixels_at: Option<usize>, icon: bool) -> Option<Decoded> {
    let header = usize::try_from(u32_at(dib, 0)?).ok()?;
    if header < INFO_HEADER {
        return None;
    }
    let width = i32_at(dib, 4)?;
    let stored_height = i32_at(dib, 8)?;
    let bits = u16_at(dib, 14)?;
    let compression = u32_at(dib, 16)?;
    let colours_used = usize::try_from(u32_at(dib, 32)?).ok()?;
    let width = usize::try_from(width).ok().filter(|width| *width > 0)?;
    let top_down = stored_height < 0;
    let mut height = usize::try_from(stored_height.unsigned_abs()).ok()?;
    if icon {
        height /= 2;
    }
    if height == 0 || !affordable(width, height) {
        return None;
    }

    // Masks written after a 40-byte header occupy the place a later header
    // keeps them in, so they are read from the same offset either way.
    let (masks, masks_after_header) = match (compression, bits) {
        (BI_RGB, 16) => (Some([0x7C00, 0x03E0, 0x001F, 0]), 0),
        (BI_RGB, 1 | 4 | 8 | 24 | 32) => (None, 0),
        (BI_BITFIELDS | BI_ALPHABITFIELDS, 16 | 32) => {
            let alpha = compression == BI_ALPHABITFIELDS || header >= 56;
            let masks = [
                u32_at(dib, 40)?,
                u32_at(dib, 44)?,
                u32_at(dib, 48)?,
                if alpha { u32_at(dib, 52)? } else { 0 },
            ];
            let after = match header {
                INFO_HEADER => 4 * (3 + usize::from(compression == BI_ALPHABITFIELDS)),
                _ => 0,
            };
            (Some(masks), after)
        }
        _ => return None,
    };

    let palette_at = header + masks_after_header;
    let palette: Vec<[u8; 4]> = match bits {
        1 | 4 | 8 => {
            let count = match colours_used {
                0 => 1 << bits,
                used => used.min(1 << bits),
            };
            (0..count)
                .map(|entry| {
                    let at = palette_at + entry * 4;
                    dib.get(at..at + 3)
                        .map(|bgr| [bgr[2], bgr[1], bgr[0], u8::MAX])
                })
                .collect::<Option<_>>()?
        }
        _ => Vec::new(),
    };
    let rows_at = pixels_at.unwrap_or(palette_at + palette.len() * 4);
    let stride = (usize::from(bits) * width).div_ceil(32) * 4;
    let rows = dib.get(rows_at..rows_at.checked_add(stride.checked_mul(height)?)?)?;

    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        let stored = if top_down { row } else { height - 1 - row };
        let line = &rows[stored * stride..(stored + 1) * stride];
        for x in 0..width {
            pixels.push(match (bits, masks) {
                (1 | 4 | 8, _) => {
                    let per_byte = 8 / usize::from(bits);
                    let byte = line[x / per_byte];
                    let shift = 8 - usize::from(bits) * (x % per_byte + 1);
                    let index = usize::from(byte >> shift) & ((1 << bits) - 1);
                    *palette.get(index)?
                }
                (24, _) => {
                    let bgr = &line[x * 3..x * 3 + 3];
                    [bgr[2], bgr[1], bgr[0], u8::MAX]
                }
                (32, None) => {
                    let bgra = &line[x * 4..x * 4 + 4];
                    [bgra[2], bgra[1], bgra[0], bgra[3]]
                }
                (16, Some(masks)) => {
                    let value = u32::from(u16::from_le_bytes([line[x * 2], line[x * 2 + 1]]));
                    masked(value, masks)
                }
                (32, Some(masks)) => {
                    let value = u32::from_le_bytes(line[x * 4..x * 4 + 4].try_into().ok()?);
                    masked(value, masks)
                }
                _ => return None,
            });
        }
    }

    // A 32-bit bitmap whose fourth byte is zero everywhere did not mean
    // "invisible"; it never used that byte. Most were written that way.
    let alpha_was_used = bits == 32 && pixels.iter().any(|pixel| pixel[3] != 0);
    if bits == 32 && !alpha_was_used {
        pixels.iter_mut().for_each(|pixel| pixel[3] = u8::MAX);
    }
    if icon && !alpha_was_used {
        apply_transparency_mask(&mut pixels, width, height, dib.get(rows_at + rows.len()..)?)?;
    }
    Decoded::new(width, height, pixels)
}

/// An icon's AND mask: one bit a pixel, rows bottom-up, a set bit
/// transparent.
fn apply_transparency_mask(
    pixels: &mut [[u8; 4]],
    width: usize,
    height: usize,
    mask: &[u8],
) -> Option<()> {
    let stride = width.div_ceil(32) * 4;
    for row in 0..height {
        let line = mask.get((height - 1 - row) * stride..(height - row) * stride)?;
        for x in 0..width {
            if line[x / 8] & (0x80 >> (x % 8)) != 0 {
                pixels[row * width + x][3] = 0;
            }
        }
    }
    Some(())
}

/// One pixel read through red, green, blue and alpha masks, each channel
/// scaled to eight bits; a zero alpha mask is opaque.
fn masked(value: u32, [red, green, blue, alpha]: [u32; 4]) -> [u8; 4] {
    let channel = |mask: u32| {
        if mask == 0 {
            return u8::MAX;
        }
        let bits = mask.count_ones();
        let raw = (value & mask) >> mask.trailing_zeros();
        let full = (1u64 << bits) - 1;
        ((u64::from(raw) * 255 + full / 2) / full) as u8
    };
    [channel(red), channel(green), channel(blue), channel(alpha)]
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn i32_at(bytes: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A `.bmp` of `width` × `height` at `bits`, rows as given (already
    /// padded), with a 40-byte header and `palette` (BGRA entries).
    pub(in crate::code::picture) fn file(
        width: i32,
        height: i32,
        bits: u16,
        compression: u32,
        extra: &[u8],
        rows: &[u8],
    ) -> Vec<u8> {
        let mut dib = info_header(width, height, bits, compression);
        dib.extend_from_slice(extra);
        let pixels_at = 14 + dib.len();
        let mut file = b"BM".to_vec();
        file.extend((pixels_at + rows.len()).to_le_bytes()[..4].iter());
        file.extend([0; 4]);
        file.extend((pixels_at as u32).to_le_bytes());
        file.extend(dib);
        file.extend_from_slice(rows);
        file
    }

    pub(in crate::code::picture) fn info_header(
        width: i32,
        height: i32,
        bits: u16,
        compression: u32,
    ) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend(40u32.to_le_bytes());
        header.extend(width.to_le_bytes());
        header.extend(height.to_le_bytes());
        header.extend(1u16.to_le_bytes());
        header.extend(bits.to_le_bytes());
        header.extend(compression.to_le_bytes());
        header.extend([0; 20]);
        header
    }

    #[test]
    fn a_24_bit_bitmap_is_read_bottom_up() {
        // Two rows of one pixel, each padded to four bytes; the first
        // stored row is the bottom one.
        let rows = [0, 0, 255, 0, 255, 0, 0, 0];
        let image = decode(&file(1, 2, 24, BI_RGB, &[], &rows)).unwrap();
        assert_eq!(image.pixels, [[0, 0, 255, 255], [255, 0, 0, 255]]);
    }

    #[test]
    fn a_negative_height_is_read_top_down() {
        let rows = [0, 0, 255, 0, 255, 0, 0, 0];
        let image = decode(&file(1, -2, 24, BI_RGB, &[], &rows)).unwrap();
        assert_eq!(image.pixels, [[255, 0, 0, 255], [0, 0, 255, 255]]);
    }

    #[test]
    fn a_32_bit_bitmap_that_never_used_alpha_is_opaque() {
        let image = decode(&file(1, 1, 32, BI_RGB, &[], &[10, 20, 30, 0])).unwrap();
        assert_eq!(image.pixels, [[30, 20, 10, 255]]);
        let image = decode(&file(1, 1, 32, BI_RGB, &[], &[10, 20, 30, 128])).unwrap();
        assert_eq!(image.pixels, [[30, 20, 10, 128]]);
    }

    #[test]
    fn bit_masks_are_read_from_after_the_header() {
        let masks: Vec<u8> = [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF]
            .iter()
            .flat_map(|mask| mask.to_le_bytes())
            .collect();
        let image = decode(&file(1, 1, 32, BI_BITFIELDS, &masks, &[1, 2, 3, 0])).unwrap();
        assert_eq!(image.pixels, [[3, 2, 1, 255]]);
    }

    #[test]
    fn a_paletted_bitmap_reads_its_colours_from_the_palette() {
        let palette = [0, 0, 0, 0, 255, 255, 255, 0];
        // Eight pixels at one bit: 1010_1010, padded to four bytes.
        let image = decode(&file(8, 1, 1, BI_RGB, &palette, &[0b1010_1010, 0, 0, 0])).unwrap();
        assert_eq!(image.pixels[0], [255, 255, 255, 255]);
        assert_eq!(image.pixels[1], [0, 0, 0, 255]);
    }

    #[test]
    fn a_truncated_or_compressed_bitmap_is_none() {
        let rows = [0, 0, 255, 0, 255, 0, 0, 0];
        let whole = file(1, 2, 24, BI_RGB, &[], &rows);
        assert!(decode(&whole[..whole.len() - 1]).is_none());
        assert!(
            decode(&file(1, 1, 8, 1, &[], &[0, 0, 0, 0])).is_none(),
            "RLE8"
        );
        assert!(decode(b"BM").is_none());
    }
}
