## Why

Protocol 1 ships 18 of the roughly 40 nodes `extension-platform`
maps (D15). Ten ports of community tools, drawn while writing the catalog
page, each wanted a node that is not there: a chart or a gauge for usage
and system meters, key/value details, Markdown for a plan, code for a
diff hunk, a big number for a total. None of these needs code to run:
they draw data a fixture already holds. So they can land now, before
scripts, and make what an author can build visibly wider.

## What Changes

- New display nodes, bound like the others and drawn in the active theme:
  - `Rule`: a hairline, with an optional label.
  - `Details` with `Entry` children: aligned key/value pairs.
  - `Markdown`: prose, lists, quotes and fenced code, wrapped.
  - `Code`: a block highlighted for its `language`.
  - `Gauge`: a labelled meter whose role follows `warning=` and `danger=`
    thresholds.
  - `Chart`: a `sparkline` row, or `bar`s `height` rows tall, over a list
    of numbers.
  - `Stat`: a figure with its label and an optional delta.
  - `Kbd`: a key, drawn as the workspace draws its own keys.
  - `Grid`: tiles in as many columns as fit `min-width`.
- The language server's completion and hover, and the check's nearest
  name, cover them because they come from the same catalog.
- **Not here:** `Split`, grouped or tree `List`, `Heatmap`, `Chip`,
  `Diff`, `Path` and every input node. They need host state, an action or
  a decision this change does not make.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `extension-views`: the catalog requirement lists the new nodes, with
  scenarios for each family.

## Impact

- `uze-contract` `catalog`: nine nodes and one child node, with docs.
- `uze-extensions` `view.rs` and `declared/bind.rs`: their `NodeKind`s and
  how each binds.
- `src/ui/extension_view/tree.rs` (and a module per family when it grows):
  their widths, heights and drawing, reusing the Markdown and highlighting
  the code surface already has.
- `uze-theme`: a `Symbol` for a chart's levels, and one for a rule.
- The reference gallery: tests and snapshots for each node.
