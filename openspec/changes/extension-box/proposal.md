## Why

A declared extension can only compose the catalog's fixed nodes: no area
takes a ground, a border, padding or a width of its own, and `Progress`
always fills with blocks. Recreating the cardinal timesheet mod showed the
cost: its day cards (a block of colour with the day and its hours) and its
thin progress lines cannot be drawn, so the extension reads far weaker than
the mod it replaces. The operator asked for a flexible component that lets
an author build such visuals, with colours still taken only from the
theme's roles.

## What Changes

- A `Box` node: a flex container whose children run in a `direction`
  (`column` or `row`), optionally `wrap`, with `gap`, a `width` in cells or
  a percentage of its parent, a `height`, `padding`, `justify`, a `ground`
  in a role (solid, or `tone="soft"`) and a host-drawn `border` in a role.
  Text inside a ground is drawn in an ink the host chooses to read on it.
- `Progress style="line"`: a thin line, its track drawn in a quiet role,
  readable as a measure even when full.
- Colours stay roles: no node takes a colour value. A ground, a border and
  an ink are all resolved by the theme.
- `Stack` and `Row` stay as they are.

## Capabilities

### New Capabilities
- `extension-box`: the `Box` node, the `line` style of `Progress`, and the
  rule that every colour they use is a role.

### Modified Capabilities
None.

## Impact

`uze-contract` (catalog, a property kind for sizes, docs and schema),
`uze-core` (the view decoder's size literal), `uze-extensions` (binding),
the TUI's extension renderer, the language server's completion, the
`uze:author` extension reference and `docs/extensions/`.
