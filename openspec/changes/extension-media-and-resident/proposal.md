## Why

After change 6, a declared extension can stand in every slot, be told what
agents do and type into them. Three gallery entries still cannot be
built, because the catalog only draws text and the shapes it composes:

- `pets` draws a creature over each agent's pane, playing a frame sheet by
  the agent's state;
- `screen` shows an image a program writes, refreshed on a clock;
- `snake` is a game: it keeps state between frames, reads raw keys and
  draws a grid every tick.

This change is change 7 of `extension-platform`'s split (*Proposed
split*): the media and the resident tier (D13, D19, R-A8, R-S12).

## What Changes

- **Cells**: a `Cells` node draws a grid of `{ch, role}` with `regions`
  that bind rectangles to declared actions and items. Colour comes from the
  host's roles; a declared extension never sends a colour value.
- **Images**: an `Image` node shows a PNG, JPEG or GIF packaged with the
  extension or written by its program into its scratch (never a checkout
  path it was not granted). The decoders move from `code/picture.rs` to
  `shared/`; a section, a band and an overlay may hold a picture; the
  painter keeps several images per frame keyed by slot, each fitted to its
  slot. Budgets per extension (R-S12): 8 MiB per file before decoding,
  32 MiB of decoded pixels, one decode at a time, all off the drawing
  thread. Where the terminal draws no images, the host's block rendering
  stands in.
- **Sprites**: a manifest's `sprite "<id>" sheet=… frame=WxH columns=… {
  state "<name>" frames=… fps=… loop=… }` with a fallback state; a `Sprite`
  node plays one state, chosen by a field (an agent's `state` from the
  `agents` source, a source's own field), scaled and composited by the
  host.
- **Resident tier**: `source "<id>" { resident "game.rhai"; }` under the
  `resident` grant keeps the script's scope alive while a surface showing
  it is open.
  - Entry points: `start(context)`, `event(event)`, `input(input)`,
    `render(size)` answering a catalog tree, and `stop()`; `event` and
    `input` answer whether to redraw.
  - `timer(ms)` (host function) asks for a tick event, at most 30 a second.
  - A focused resident surface receives raw keys and the pointer, except
    the host's escape chord (`esc`, which leaves it), F1 and F8.
  - Budgets: the script engine's per-call operations and deadline, one
    render per frame and 30 a second; a breach stops it and its surface
    names the bound; a late render keeps the last frame.
- **Gallery**: `pets`, `screen` and `snake` as fixture plugins with tests.

## Capabilities

### New Capabilities
- `extension-media`: cells, images and sprites a declared extension draws.
- `extension-resident`: a script that keeps its state, reads raw input and
  renders every frame it asks for.

### Modified Capabilities
- None in `openspec/specs/`: the catalog and the scripts change 4 and 6
  brought are not archived; their deltas are written when they are.

## Impact

- `uze-contract`: `NodeName::{Cells, Image, Sprite}`, `SpriteDecl`,
  `SourceKind::Resident`, `Grant::Resident`, the envelope's `timer`.
- `uze-core`: the decoder and the check (a sheet inside the extension,
  frame sizes dividing it, states naming frames that exist).
- `uze-script`: the resident runtime and `timer`.
- `uze-extensions`: the engine's resident path; `shared/picture.rs`.
- `uze-application`: the runner hands a resident session to the host.
- `src/ui`: the painter's several images, the block fallback, raw input to
  a focused resident surface, the extensions rule amended (18.4).
- Fixtures `pets`, `screen`, `snake`; docs, invariants, a journey.
