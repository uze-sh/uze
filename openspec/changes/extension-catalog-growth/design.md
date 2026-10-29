## Context

Each node of the catalog is one entry in `uze-contract`'s `CATALOG` (the
decoder, the check's nearest name and the language server read it), one
`NodeKind` in `uze-extensions` `view.rs` with its binding in
`declared/bind.rs`, and its width, height and drawing in
`src/ui/extension_view/tree.rs`. The code surface already renders Markdown
(`uze_extensions::code::markdown`, folded by `prose_rows`) and highlights
fenced code through the same path.

## Goals / Non-Goals

**Goals:** the nodes the ten ports on the catalog page wanted that draw
data alone, each with a TestBackend test in both themes and a test in the
reference gallery.

**Non-Goals:** nodes that need host state beyond what exists (`Split`'s
edge, a grouped `List`'s folds), an action (`Path`, inputs) or an open
decision (`Heatmap`'s scale, `Diff`'s source); a theme-level redesign of
existing nodes.

## Decisions

### G1. Bound like the others, drawn by the host
Every new node takes its values through the same binding rules (D23).
Arguments and properties are typed in the catalog, so a `Chart` given a
string where a list was asked draws nothing and is said by the check, like
a `Progress` today. No node takes a colour.

### G2. Markdown and code reuse the code surface's rendering
`Markdown` keeps its text in the `NodeKind` and is folded at draw time with
`uze_extensions::code::markdown` and `prose_rows`, so wrapping follows the
node's width and fenced code is highlighted the same way as in the editor.
`Code` is drawn as a fenced block of its `language` through the same call,
cut at the width rather than wrapped, because a wrapped line of code reads
as two. Both are clamped to a height of 200 rows so a fixture cannot grow
a view without limit.

### G3. Levels and rules are `Symbol`s
A sparkline's eight levels and a bar's top cell come from a new
`Symbol::ChartLevel` whose frames are the levels, as `ProgressFill`'s are
the eighths of a cell; a `Rule` uses `Symbol::Rule` already in the theme.
A theme can redraw both.

### G4. A gauge's thresholds choose its role
`Gauge` takes `role` for its normal state and `warning=`/`danger=`
thresholds (ratios). The host picks the role, so a meter turns at the
same point in every theme, and the author writes no lookup for it.

### G5. A grid fits columns, then rows
`Grid` computes `columns = max(1, (width + gap) / (min-width + gap))`,
gives each tile an equal share and lays the tiles row by row, each row as
tall as its tallest tile. With no `min-width`, a tile is 24 columns.

## Risks / Trade-offs

- [Markdown inside a narrow drawer wraps a lot] → it follows the width it
  is given; the cap on rows keeps a long document from taking the view.
- [More nodes, more to keep consistent] → each joins with its doc, its
  completion (from the catalog) and its gallery test, in the same commit.

## Migration Plan

Additive: no existing node changes. A view naming a new node is refused
by an older build as an unknown node, which the protocol number does not
yet distinguish; that is recorded for the next protocol bump.
