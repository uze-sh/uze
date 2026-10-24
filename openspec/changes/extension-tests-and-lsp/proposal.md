## Why

An extension author can now draw a view from a fixture and watch it redraw
on save, but cannot prove it still draws right after the next edit, and
writes KDL with no completion, hover or inline errors. This is change 2 of
`add-extension-platform`'s split (D26). It closes the loop for visual
extensions and runs no code a plugin ships, like change 1.

## What Changes

- `uze agent plugin check` runs an extension's `tests/*.test.kdl`
  headlessly. It uses the engine and the workspace's own renderers on a test
  backend, and fails the check when a test fails.
- A test gives data from a fixture or inline, a slot, a size, a theme and a
  locale. It drives gestures (press a key, switch a tab, click a node by its
  text, press a button). It expects drawn text, an absent node with its
  reason, and a recorded action with its arguments. It compares a cell
  snapshot; `--update-snapshots` rewrites the snapshots.
- `uze agent plugin lsp` is a language server over stdio for
  `extension.kdl`, views and tests. Its completion and hover are generated
  from the `uze-contract` catalog, and it publishes the same diagnostics
  as the check.
- The check suggests the nearest name for a misspelled node, property, role
  or lookup (`Tabel` → `Table`).
- The check binds every tab of a view against its fixture, not only the
  first, so a field missing under a second tab is warned about.
- `uze agent plugin create --extension` also scaffolds a passing test.

## Capabilities

### New Capabilities
- `extension-tests`: test files for declared extensions, how they run in the
  check, and their snapshots.
- `extension-lsp`: the language server for manifests, views and tests.

### Modified Capabilities
- `extension-views`: the check suggests the nearest name and binds every
  tab, and the scaffold includes a test.

## Impact

- `uze-contract`: test declaration types; the catalog grows the docs that
  hover shows.
- `uze-core` `capability/extension/`: decoding `*.test.kdl` through the same
  bounded parse; nearest-name suggestions.
- `uze-extensions` `declared/`: a headless driver over the engine (gestures
  in, outcomes and traces out); the engine stays pure.
- `src/cli/`: the test runner rendering through `src/ui/extension_view*` on
  ratatui's `TestBackend`, the snapshot format, and `uze agent plugin lsp`,
  classified in `command_performance.rs`.
- New dependencies: `lsp-server` and `lsp-types` (rust-analyzer's), judged
  under the `dependencies` rule. `strsim` is already in the lock through
  `clap`.
- `plugins/uze` `uze:author` Skill: how to write and run a test, and how to
  point an editor at the LSP.
