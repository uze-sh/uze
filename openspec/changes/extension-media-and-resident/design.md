## Context

The platform's design is `add-extension-platform/design.md`: D13 (tiers),
D15 (catalog), D19 (the resident tier), D20 (gallery), D28 (budget), and
the second review's R-A8 (several images per frame) and R-S12 (a
per-extension memory budget).

What exists after change 6:

- `uze-script` runs a declared script per question, bounded by operations
  and a 100 ms deadline, with host functions registered by grant.
- `code/picture.rs` decodes images for the code surface;
  `src/ui/graphics.rs` probes the terminal's image protocol and places one
  picture per frame.
- The declared engine answers sources through jobs on the pool and draws
  views the host renders; `Activity` and `Elapsed` animate from the host's
  clock.

## Goals / Non-Goals

**Goals:** `pets`, `screen` and `snake` work as the gallery describes,
inside the budgets, with no colour value and no terminal byte coming from an
extension.

**Non-Goals:** video, audio, a WebAssembly runtime (D27), images from the
network (`net` is change 8), a resident program (only scripts are
resident).

## Decisions

### M1. Media are catalog nodes; bytes never reach the terminal from an extension

`Cells`, `Image` and `Sprite` are nodes like any other: an extension names a
file or a grid, and the host decodes, scales and writes. The decoders live
in `uze-extensions::shared::picture` (moved from `code/`), the protocol
writing stays in `src/ui/graphics.rs`.

### M2. An image comes from the extension's own files or its program's scratch

A path in an `Image` is relative to the extension's directory, or names a
file in the run's scratch the extension's program wrote (`scratch:` prefix),
copied out when the run ends. Nothing is read from a checkout through an
image: `read:checkout` returns text.

### M3. Several images per frame, keyed by slot, under a per-extension budget

The painter holds one placement per slot. Decoding happens on the pool, one
at a time per extension; a file over 8 MiB or a decode past 32 MiB of
pixels for that extension is refused with the bound named.

### M4. A resident source is a session the engine keeps, asked by the host's clock

`SourceKind::Resident(file)` opens a session when a surface showing it is
in view and closes it (`stop`) when none is. The session lives on one
thread of its own (the script's scope is not `Send` across the pool), and
every call has the engine's bounds. The host calls `render(size)` at most
once per frame it draws and 30 times a second, and only after `start`,
`event` or `input` answered `true` or a `timer` fired.

### M5. Raw input reaches only a focused resident surface, and never the host's chords

While a resident surface is focused, keys and pointer events are handed to
`input` as `{key, mods}` or `{x, y, button}`, relative to the surface,
except `esc` (which unfocuses it), F1 and F8. The extensions rule is
amended for this tier only.

## Risks / Trade-offs

- [A resident script burns CPU] → per-call operations, 30 renders a second,
  a breach stops it.
- [An image bomb] → R-S12's bounds before and after decoding.
- [A terminal without images] → the block fallback, as the code surface.
