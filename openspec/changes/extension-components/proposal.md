## Why

An extension is a manifest, a `.kdl` file per view and a `.rhai` file per
script. The sandbox timesheet came to seven files for one drawer and one
bar item: two of its scripts are three lines each, one per button, and the
logic that fills the drawer sits in a file apart from the view it fills.
The operator asked for one file that holds a view and the script beside
it, as Vue and Svelte hold a template and a script, written in what UZE
already reads: KDL and Rhai.

## What Changes

- **BREAKING** A component file, `<name>.uze`, holds a `<view>` block (the
  KDL a `.kdl` view held) and a `<script>` block (the Rhai a `.rhai` script
  held), each at most once, in either order.
- **BREAKING** A surface's `view`, a source's `script` and `resident`, and
  an action's `script` name `.uze` files; `.kdl` views and `.rhai` scripts
  are refused, naming the `.uze` that replaces them.
- An action may `call "<function>"`: a function of the `<script>` in the
  file of the surface the action is `on`, called with the same
  `(input, context)` an `answer` takes.
- `extension.kdl` and `tests/*.test.kdl` stay KDL, unchanged.
- UZE highlights `.uze` in its own code viewer and inspector, each block
  in its language. Editors other than UZE's own are out of scope.
- The gallery, the journeys, the sandbox, the scaffold and the docs move
  to `.uze`.

## Capabilities

### New Capabilities
- `extension-components`: the `.uze` file, its blocks, `call`, and the
  host reading them.

### Modified Capabilities
None.

## Impact

`uze-core` (a block reader, the manifest's file checks, the view and
script compiles), `uze-contract` (`Does::Call`), `uze-script` (calling a
named function), `uze-application` (the runner), `uze-extensions` (the
highlighter), `src/cli/lsp` (completion by block), the scaffold
(`uze agent plugin create`), every fixture and journey that writes an
extension, `docs/extensions/`, the `uze:author` reference.
