//! ICO: a directory of the same icon at several sizes. The largest is the
//! one drawn, since the host only ever shrinks; each entry is a PNG or a
//! bitmap without its file header.

use super::{Decoded, bmp, png};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub(super) fn decode(bytes: &[u8]) -> Option<Decoded> {
    if bytes.get(..4)? != [0, 0, 1, 0] {
        return None;
    }
    let count = usize::from(u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?));
    let largest = (0..count)
        .filter_map(|index| {
            let entry = bytes.get(6 + index * 16..6 + (index + 1) * 16)?;
            // Zero means 256, the one size a byte cannot hold.
            let side = |byte: u8| if byte == 0 { 256 } else { u32::from(byte) };
            let size = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
            Some((side(entry[0]) * side(entry[1]), offset, size))
        })
        .max_by_key(|(area, ..)| *area)?;
    let (_, offset, size) = largest;
    let image = bytes.get(offset..offset.checked_add(size)?)?;
    match image.starts_with(PNG_SIGNATURE) {
        true => png(image),
        false => bmp::icon_entry(image),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(entries: &[(u8, &[u8])]) -> Vec<u8> {
        let mut file = vec![0, 0, 1, 0];
        file.extend((entries.len() as u16).to_le_bytes());
        let mut offset = 6 + entries.len() * 16;
        for (side, image) in entries {
            file.extend([*side, *side, 0, 0, 1, 0, 32, 0]);
            file.extend((image.len() as u32).to_le_bytes());
            file.extend((offset as u32).to_le_bytes());
            offset += image.len();
        }
        for (_, image) in entries {
            file.extend_from_slice(image);
        }
        file
    }

    fn png_of(side: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = ::png::Encoder::new(&mut bytes, side, side);
        encoder.set_color(::png::ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&rgba.repeat((side * side) as usize))
            .unwrap();
        writer.finish().unwrap();
        bytes
    }

    #[test]
    fn the_largest_entry_is_drawn() {
        let small = png_of(1, [255, 0, 0, 255]);
        let large = png_of(2, [0, 0, 255, 255]);
        let image = decode(&icon(&[(1, &small), (2, &large)])).unwrap();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(image.pixels[0], [0, 0, 255, 255]);
    }

    #[test]
    fn a_bitmap_entry_takes_its_transparency_from_the_mask() {
        // 2×1 at 24 bits: a doubled height, one colour row padded to eight
        // bytes, then one mask row padded to four, its first pixel masked.
        let mut entry = bmp::tests::info_header(2, 2, 24, 0);
        entry.extend([0, 0, 255, 0, 255, 0, 0, 0]);
        entry.extend([0b1000_0000, 0, 0, 0]);
        let image = decode(&icon(&[(2, &entry)])).unwrap();
        assert_eq!(image.pixels, [[255, 0, 0, 0], [0, 255, 0, 255]]);
    }

    #[test]
    fn a_file_that_is_not_an_icon_is_none() {
        assert!(decode(b"not an icon").is_none());
        assert!(decode(&icon(&[])).is_none());
    }
}
