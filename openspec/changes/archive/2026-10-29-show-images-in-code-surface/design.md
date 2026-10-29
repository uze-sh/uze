## Context

See proposal.md for why. The code surface is an extension
(`uze-extensions::code`) that describes a `view::View` and never draws;
the client (`src/ui/extension_view`) renders it with ratatui, and every
machine access goes through `uze_extensions::Host`. Before this change a
file that is not UTF-8 came back from `Host::read_file` as
`Unreadable::NotText` and the view drew a message.

PNG with half blocks and Sixel is already implemented on
`feat/image-render`; this design records it and extends it.

## Goals / Non-Goals

**Goals:**
- The six common raster formats, decoded in pure Rust.
- The best picture each terminal can show: Sixel at its own cell
  resolution where it says it can, half blocks everywhere else.
- No new grant beyond reading a file's bytes; no image ever editable.

**Non-Goals:**
- SVG (needs a vector renderer: `resvg` and its font stack) and AVIF
  (no mature pure-Rust decoder). Both keep "is not a text file".
- The kitty graphics protocol and iTerm2's inline images. Sixel covers
  Windows Terminal, WezTerm, foot, xterm, Konsole and iTerm2; kitty's
  protocol is a later addition behind the same `Placement`.
- Images inside a pane: the terminal runtime (`alacritty_terminal`)
  does not parse graphics protocols, so a program's own image output
  never reaches the client.
- The previous side of a changed image: `Host::git` answers text, and a
  blob is not text. The changes half draws the working tree's version.
- Zoom and pan at 100%: a follow-up once this lands.

## Decisions

**Decoding stays in the extension; the host only reads bytes.** Which
formats a surface can show is the surface's decision, so
`code/picture.rs` decodes and hands the host straight RGBA
(`view::Picture`, pixels in an `Arc` because the view is rebuilt on every
frame). `Host::read_bytes` is the one new grant, bounded at 32 MiB (an
image is compressed and routinely past the 2 MiB text cap), with a
default that answers `NotText` so a host that grants nothing changes
nothing. Alternative rejected: a `Host::read_image` that decodes in the
host, which would put format knowledge in the client and make every
future host re-implement it.

**An image is still `Unreadable::NotText`.** The decoded picture sits in
`OpenFile::picture` *beside* `error`, not instead of it, so every guard
that refuses to edit or save a file with an error keeps refusing an
image without being touched. The read answer gains a variant
(`FileAnswer::Pictured`) rather than a field, because it answers the
same request and releases the same reservation.

**Format by extension, then by content.** The extension says which
decoder to try (`is_picture`); the bytes decide whether it was right
(`decode` returns `None` and the file stays "not text"). Sniffing magic
bytes for every non-text file would make an extensionless binary try
six decoders for nothing.

**Decoders, by provenance (AGENTS.md "Dependencies").**
`png`, `gif`, `image-webp`: image-rs organization, pure Rust,
`#![forbid(unsafe_code)]` in the decoders. `zune-jpeg`: single
maintainer, mature, the JPEG decoder the `image` crate itself moved to;
pure Rust. Each with default features off where that drops encoders.
BMP and ICO in-house: a BMP is a header, a palette or bit masks, and
uncompressed rows (1/4/8 bits paletted, 16/32 bits masked, 24 bits), and
an ICO is a directory whose largest entry is either a BMP without its
file header or a PNG, which reuses the PNG decoder. RLE-compressed BMPs
stay "not text". Every decoder refuses an image over 64 Mi pixels from
its header, before allocating (see decisions.md D4). Alternative rejected: the `image` crate, which brings
every format, its own resampling and a far larger dependency tree for
what is six decoders and one filter.

**Two outputs from one placement.** `extension_view/picture.rs` always
draws half blocks (`▀`, foreground the upper pixel, background the
lower). Where the probe found Sixel, it also returns a
`graphics::Placement`: the cells the image covers and its size in
pixels, fitted at the reported cell size and centred by whole cells.
After `terminal.draw`, the orchestrator checks the placement's cells
still hold half blocks in the finished buffer (nothing was drawn over
it) and asks `graphics::Painter` for bytes to write. The painter sends
only when the placement's identity (pixel buffer, cells, pixel size,
theme generation) differs from what it last sent, and forgets on a
covered or absent frame so the image is sent again once uncovered.
Alternative rejected: writing the Sixel string into the first cell's
symbol and marking the rest `skip` (how `ratatui-image` does it), which
leaves a hole when an overlay closes, because the skipped cells never
differ from the previous buffer.

**Probe once, before input is read.** `graphics::probe` asks
`CSI 16 t` (cell size) followed by DA1 through
`uze_platform::stdio::ask_terminal`, next to the background probe in
`ui::run`, ending on the DA1 reply. Sixel is attribute `4` of DA1; the
cell size falls back to the kernel's window size in pixels over cells.
Without both, no placement is made.

**One Lanczos-3 resample, never enlarging.** Decoding keeps up to 4096
pixels a side, only to bound memory; the client resamples once, from
that, to the placement's pixel size, separably, after laying each pixel
over the theme's ground. The fit is capped at the image's own size. A
box average over integer spans (the first implementation) makes text
strokes uneven whenever the ratio is not whole.

**Palette by median cut.** Sixel carries up to 256 colour registers. The
image is bucketed at 5 bits a channel and the bucket set median-cut to
256 boxes, each drawn as its mean; flat interface colours come out exact.
No dithering: it trades the crispness of text for gradients, and
screenshots of interfaces are the main case.

**Encoding leaves the drawing thread.** Resampling and encoding cost
about 60 ms for a 1260×688 placement. The painter runs them on a thread
and writes the bytes on the frame after they arrive, if the placement
is still the one they were made for — the same question-carrying answer
the workspace's Git reads use.

No diagram under `docs/architecture/` changes: no container or
relationship is added (the TUI already draws to the developer's
terminal), and the dependency additions are below the containers'
level.

## Candidate ADRs

- Images in the workspace through Sixel over half blocks: commits the
  client to writing terminal graphics outside ratatui's buffer, with the
  covered-cell rule as the contract.
- `Host::read_bytes`: the first grant handing an extension a file's raw
  bytes.

## Risks / Trade-offs

- [Windows Terminal reports a virtual 10×20 cell for Sixel whatever the
  font] → the resolution ceiling there is 10×20 per cell; documented,
  and the zoom follow-up is the way past it.
- [A terminal lists `4` in DA1 but draws Sixel badly, or a multiplexer
  answers for the terminal behind it] → the probe result is logged
  (`terminal graphics sixel=… cell=…`); half blocks remain underneath, so
  a terminal that ignores the Sixel still shows the image.
- [Sixel scrolls the screen if it reaches the last row] → a placement is
  always inside the content column, above the footer.
- [Binary size] → the four decoders, default features off where that
  drops an encoder. Measured on x86_64-unknown-linux-gnu, plain
  `cargo build --release` as `release.yml` builds it: `main` 26,049,744
  bytes (9,385,084 as `.tar.gz`), this change 26,817,352 (9,717,277):
  +750 KiB raw, +324 KiB compressed, about 3.5% of the download.
- [A malicious image] → decoders are pure Rust with bounded sizes; the
  read is capped at 32 MiB and decoded dimensions at 4096 a side after
  decode, and the PNG decoder's own allocation limit applies.
