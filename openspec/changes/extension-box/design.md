## Context

See proposal.md. The renderer (`src/ui/extension_view/tree.rs`) lays a view
out itself: `natural` gives a node's width, `height` its rows at a width,
and the painter draws each node into a window it scrolls and folds. No node
paints a ground: a `Badge` draws its role's ink on a neutral raised surface.

## Goals / Non-Goals

**Goals:** one container general enough to draw the cardinal timesheet's
cards and similar visuals; colours from roles only; the host still draws
every pixel, clips to the slot and keeps the attribution band.

**Non-Goals:** colour values; absolute positioning; overlapping children;
a border that can reach outside the Box; replacing `Stack` and `Row`.

## Decisions

- **`Box` beside `Stack` and `Row`** (D1): additive, so no view changes.
- **Sizes are cells or a percentage** (D2): a new property kind, `size`,
  checked when the view is decoded.
- **A ground is solid or soft** (D3): `tone="soft"` lays the role's hue over
  the surface at a fixed share, as the theme already mixes its tokens.
- **Text on a ground takes the host's ink** (D4): bright or dark by the
  ground's lightness, so a role never becomes unreadable on another.
- **The border is the host's, inside the Box** (D5): it takes a row and a
  column on each side out of the inner area.
- **`Progress style="line"`** (D6): `━` in the role over a track in `faint`.

## Risks / Trade-offs

- A ground forces one ink for everything inside it, so roles inside a
  ground lose their hue. Accepted: legibility first; an author marks inside
  a ground with `strong`.
