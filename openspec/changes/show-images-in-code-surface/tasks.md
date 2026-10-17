## 1. Already on `feat/image-render` (PNG)

- [x] 1.1 `Host::read_bytes` with a refusing default; `WorkspaceHost` reads up to 32 MiB
- [x] 1.2 `view::Content::Picture` and `view::Picture` (straight-alpha RGBA, pixels in an `Arc`); every match on `Content` covers it
- [x] 1.3 `code/picture.rs`: PNG decode (all colour types normalised to 8-bit RGBA), cap at 4096 a side; tests for colour types, a false `.png`, the cap
- [x] 1.4 Files half: `FileAnswer::Pictured`, `OpenFile::picture` beside `error`; test that an image is drawn and `e` opens no editor
- [x] 1.5 Changes half: `DiffRead::picture` / `Changes::diff_picture` carried through every place `diff_binary` is
- [x] 1.6 Half blocks in `extension_view/picture.rs`, transparency over the theme ground (`theme::pixel`); fit tests
- [x] 1.7 `graphics.rs`: probe (`CSI 16 t` + DA1), `Placement::fit` capped at the image's size, covered-cell check, `Painter` repaint rule, Lanczos-3, median cut, Sixel encoder; tests for each
- [x] 1.8 `png` added to `uze-extensions`, `CREDITS.md` regenerated, `cargo deny check` passes

## 2. Remaining formats

- [x] 2.1 Run `cargo info` and `cargo tree -p` for `zune-jpeg`, `gif` and `image-webp`; confirm publisher, repository, no C build script, and default features to turn off
- [x] 2.2 JPEG (`.jpg`, `.jpeg`) through `zune-jpeg`, to RGBA from whatever colour space it reports; test colour and grayscale against committed fixtures (decisions.md D2, D3)
- [x] 2.3 GIF through `gif`, first frame composited into the logical screen with its transparency; test a two-frame GIF draws the first
- [x] 2.4 WebP (lossy and lossless) through `image-webp`, first frame; test with an image its own lossless encoder writes (decisions.md D2)
- [x] 2.5 BMP in-house: `BITMAPINFOHEADER` and later headers, 24 and 32 bits, bottom-up and top-down rows, `BI_RGB` and `BI_BITFIELDS` for 32-bit; anything else returns `None`. Tests for each orientation and a truncated file
- [x] 2.6 ICO in-house: read the directory, take the largest entry, decode it as PNG or as a header-less BMP with its AND mask as alpha. Tests for a PNG entry and a BMP entry
- [x] 2.7 `is_picture` answers for every extension above, case-insensitively, and only those; extend the existing test
- [x] 2.8 Regenerate `CREDITS.md` (`make attributions`), run `cargo deny check`, and record each decoder's provenance (in `crates/uze-extensions/Cargo.toml` and the commit)

## 3. Off the drawing thread

- [x] 3.1 Move resampling and Sixel encoding into a thread: the painter asks with the placement's identity and writes the answer on the next frame only if that identity is still the one shown; drop a stale answer
- [x] 3.2 Test that a placement replaced while its encode is out never has the old bytes written

## 4. Verification

- [x] 4.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`
- [x] 4.2 `openspec validate show-images-in-code-surface --strict`
- [x] 4.3 Measure the release binary against `main` and record the difference (design.md, Risks)
- [x] 4.4 By hand, on Windows Terminal (Sixel) and on a terminal without Sixel: each format in the files half, a changed PNG in the changes half, a menu opened and closed over an image, a theme switch while one is shown
