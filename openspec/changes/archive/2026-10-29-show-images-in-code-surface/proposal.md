## Why

The code surface lists every file in the checkout, and the tree already
marks images with their own icon, but selecting one shows only "is not a
text file". Screenshots, logos and posters are what a reviewer most needs
to *see* in a change, and today they have to leave the workspace to do it.
A terminal can show an image: in half blocks on any terminal with true
colour, and at its own pixel resolution on one that speaks Sixel.

## What Changes

- **An image is drawn where its text would be.** Selecting PNG, JPEG, GIF,
  WebP, BMP or ICO in the files half draws the image, fitted to the
  content column and centred, under its path. GIF and APNG show their
  first frame.
- **A changed image is drawn in the changes half** instead of "Git
  records that it changed, but has no lines to compare in it": the file
  as it stands in the working tree.
- **Every terminal gets the image; a capable one gets its pixels.** Half
  blocks (two pixels per cell, true colour) everywhere. Where the
  terminal says it speaks Sixel and how large its cells are, the image is
  also sent as Sixel at that resolution, over the half blocks, and again
  after anything drawn over it goes away.
- **Sharp rather than merely present**: one resample from the decoded
  original with a Lanczos-3 filter, never enlarged past its own size,
  transparency laid over the theme's ground.
- **An image is never editable.** It stays a file that is not text, so
  `e` does nothing and nothing can save an empty buffer over it.
- **Anything that cannot be decoded** keeps today's "is not a text file"
  message: an unsupported format, a damaged file, a file larger than the
  read limit.

## Capabilities

### New Capabilities
<!-- None. -->

### Modified Capabilities
- `code-surface`: adds what the files and changes halves show for an
  image, how it reaches the terminal at the best resolution the terminal
  allows, and that an image is never opened for editing.

## Impact

- `crates/uze-extensions`: `Host::read_bytes` (a new grant, bytes of a
  file, bounded like `read_file`), `view::Content::Picture` and
  `view::Picture` in the view contract, decoding in `code/picture.rs`,
  and the image carried through the read and diff answers.
- `src/ui`: `extension_view/picture.rs` (half blocks), `graphics.rs`
  (terminal probe, fit, Lanczos-3, median-cut palette, Sixel encoder,
  repaint after a frame), `theme::pixel`/`pixels_rgb`,
  `extension_host::read_bytes`, the probe in `ui::run`.
- **New dependencies**, all pure Rust with no C: `png`, `gif` and
  `image-webp` (image-rs organization), `zune-jpeg` (zune-image, mature,
  used by `image`). BMP and ICO are decoded in-house: both are a header
  and uncompressed rows (an ICO entry may also be a PNG).
- One more terminal round trip when the workspace opens (`CSI 16 t` +
  DA1), bounded by the same 500 ms patience as the background probe.
- Binary size grows by the decoders; measured in the PR.
