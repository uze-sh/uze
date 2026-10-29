## D1 — Read 1- and 4-bit paletted bitmaps, not only 8-bit

- status:   pending
- task:     2.5
- commit:   b4103e91
- trigger:  design.md said paletted BMPs below 8 bits stay "not text"; the
            palette read is the same code for 1, 4 and 8 bits, only the
            index extraction differs.
- decided:  `bmp.rs` reads 1, 4 and 8 bits through the palette, and 16-bit
            `BI_RGB` (5-5-5) through default masks. RLE stays `None`.
- rejected: refusing below 8 bits, which would leave monochrome bitmaps
            (old icons, scanned line art) unshown for no saving.
- blast:    none; dropping it is deleting two match arms.

## D2 — Commit two JPEG fixtures; generate every other format in the test

- status:   pending
- task:     2.2, 2.4
- commit:   b4103e91
- trigger:  no JPEG encoder is in the tree, and `image-webp` turned out to
            ship a lossless encoder, so task 2.4's committed WebP fixture
            is unnecessary.
- decided:  `tests/_fixtures/images/rust.jpg` (16×8, colour) and `gray.jpg`
            (8×8, one channel), made with ffmpeg, 220 bytes each, asserted
            within a tolerance since JPEG is lossy. PNG, GIF and WebP are
            encoded inside the tests; BMP and ICO are written byte by byte.
- rejected: adding a JPEG encoder as a dev-dependency only to make a
            fixture, or a committed WebP when the encoder is already here.
- blast:    none.

## D3 — Leave CMYK JPEG untested

- status:   pending
- task:     2.2
- commit:   b4103e91
- trigger:  task 2.2 named CMYK output; the decoder is asked for RGBA and
            converts CMYK itself, and the code reads whatever colour space
            it reports back, so there is no CMYK path of ours to test.
            Neither ffmpeg nor anything else on the machine writes one.
- decided:  test colour and grayscale, the two the reporting path branches
            on; CMYK goes through the same `output_colorspace` answer.
- rejected: committing a CMYK fixture from the internet, of unknown origin.
- blast:    a CMYK JPEG that decodes wrongly would show wrong colours, not
            fail; caught by hand in 4.4.

## D4 — Refuse an image over 64 Mpixels from its header

- status:   pending
- task:     2.2–2.6
- commit:   b4103e91
- trigger:  the 32 MiB read cap bounds the file, not what it decodes to: a
            small file can declare an image of 100k × 100k.
- decided:  every decoder reads dimensions first and returns `None` above
            64 Mi pixels (256 MiB of RGBA), before allocating.
- rejected: relying on each crate's own limits, which differ per crate and
            are absent in ours.
- blast:    an image larger than that is "not text"; raising the constant
            is the whole revert.

## D5 — Sample at most 8×8 source pixels per half-block cell

- status:   pending
- task:     3.1
- commit:   f4403cd8
- trigger:  moving Sixel off the drawing thread left the half blocks there,
            which averaged every source pixel on every frame: up to 16 Mi
            reads per redraw for a 4096-pixel image, on each pointer move.
- decided:  `graphics::sample` takes at most 8 evenly spaced taps per axis
            of the span a cell stands for. The Sixel path does not use it.
- rejected: caching the half blocks per placement, which would give the
            immediate-mode renderer state to keep in step with the view.
- blast:    none; at two pixels per cell the difference is not visible.
