# Decisions — extension-box

## D1 — `Box` is a node beside `Stack` and `Row`, not their replacement

- status:   pending
- task:     1.1
- commit:
- trigger:  The operator asked for a flexible component; whether `Stack` and `Row` become shorthands for it was asked and not answered.
- decided:  `Box` joins the catalog; `Stack` and `Row` stay as they are, so no view changes. They can become shorthands later without breaking one.
- rejected: Replacing them now: every view and test in the tree would move for no visual gain.
- blast:    `uze-contract::catalog`.

## D2 — A size is cells or a percentage, checked when the view is decoded

- status:   pending
- task:     1.2
- commit:
- trigger:  The timesheet's cards are a fifth of the drawer whatever its width; a count alone cannot say that.
- decided:  `PropKind::Size`: a whole number of cells, or `"N%"` from 1 to 100, refused otherwise with its own message; a reference is resolved at bind and an answer that is neither is left out. A box sizes itself against what holds it, and a box inside a box is sized once, by its parent.
- rejected: Fractions (`1fr`): one more unit to learn for what a percentage already says.
- blast:    `uze-contract::catalog::Size`, the view decoder, the schema, the language server.

## D3 — A ground is a role's hue, solid or soft, and an empty one is none

- status:   pending
- task:     2.3
- commit:
- trigger:  The operator kept colour values out for now: only the theme's roles.
- decided:  `ground` is a role; `tone="soft"` mixes it toward the surface as the TUI's softened controls do. An empty value (`""`, a lookup's default) draws no ground, so one `each` can mark some rows and leave the rest bare.
- rejected: New roles for soft hues (`info-soft`): twelve more names for one modifier.
- blast:    `bind.rs::box_layout`, `tree.rs::ground_colours`.

## D4 — Text on a ground takes the host's ink

- status:   pending
- task:     2.3
- commit:
- trigger:  A role's hue on another's ground can be unreadable in some theme.
- decided:  Every line drawn on a ground takes the theme's background colour when the ground is light and its brightest text when dark, the border included. Roles inside a ground lose their hue.
- rejected: Keeping each role's hue: legibility would depend on the pair of roles and on the theme.
- blast:    `Painter::line` (`ink`).

## D5 — The border is the host's, drawn inside the box

- status:   pending
- task:     2.3
- commit:
- trigger:  A border the extension drew could reach the slot's edge or the attribution band.
- decided:  `border` is `line`, `round` or `heavy`, drawn by the host in `border-role` (`faint` by default) on the box's own cells, its top and bottom only where they are in the window. A box never draws outside the cells its parent gave it.
- rejected: A border colour or glyph of the extension's own.
- blast:    `Painter::border`.

## D6 — `Progress style="line"` draws a thin line over a quiet track

- status:   pending
- task:     1.3, 2.4
- commit:
- trigger:  The operator could not tell a full block bar was a progress bar.
- decided:  `style="line"` draws `progress.line` (`━`, `=` in the ASCII theme) in the segment's role for its filled cells and in `faint` for the rest, to the whole cell. `block` stays the default.
- rejected: Making the line the default: every existing view would change look.
- blast:    `tree.rs::thin_progress_line`, `uze-theme` (`ProgressLine`).
