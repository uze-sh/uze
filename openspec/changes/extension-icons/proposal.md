## Why

The timesheet's bar item wrote `⏱` into a `Text`: a pictograph a terminal
draws from its emoji font, two cells or one depending on the terminal, and
nothing an operator who chose ASCII glyphs can read. UZE's own chrome never
writes a glyph where it is drawn (`no_chrome_glyph_is_written_where_it_is_drawn`);
it names a `Symbol` and the operator's glyph set decides. An extension had
no such name to give, so the operator asked for a vocabulary of icons, a
way to declare more, and the check to refuse a glyph written loose.

## What Changes

- An `Icon "<name>"` catalog node, with a `role`, drawn by the host in the
  glyph set the operator chose (`[appearance] glyphs`).
- A vocabulary of 49 icons the host carries in every set (`clock`,
  `git-branch`, `check`, `warning`, `rocket`…), as `icon.*` symbols of
  the theme, so a theme may redraw them like any symbol.
- A manifest may declare more: `icon "<name>" nerd="…" unicode="…"
  ascii="…"`, all three required, each held to its set; a declared name
  never shadows the host's.
- The check refuses a view or lookup table that writes an icon glyph
  loose: a Nerd Font's private use areas, and the blocks of pictographs,
  dingbats and technical symbols a terminal draws from a font it may not
  have. Arrows, box drawing, geometric marks and typography stay text.
- A theme file says which glyph set it is (`glyphs`), so the host knows
  which spelling of a declared icon to draw.

## Capabilities

### New Capabilities
- `extension-icons`: the `Icon` node, the vocabulary, declared icons and
  the refusal of loose glyphs.

### Modified Capabilities
None.

## Impact

`uze-contract` (`icons`, `IconDecl`, `NodeName::Icon`), `uze-theme`
(`icon.*` symbols in the three sets, `GlyphSet`, `Theme::glyphs`, the
schema), the manifest decoder and the view compiler, the binder and the
tree renderer, the gsd fixture's branch line, `docs/extensions/`, the
`uze:author` extension reference.
