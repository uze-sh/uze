## 1. Contract and decoder

- [x] 1.1 `uze-contract`: `TestFile`, `TestDecl`, the world (surface, data per source as fixture path or inline JSON, size, theme, locale) and `Step` with spans; the step names; a one-line `doc` per catalog node, property, role, slot and step
- [x] 1.2 `uze-core` `capability/extension/test.rs`: decode `tests/*.test.kdl` through the bounded parse with the decoder's refusals; discover test files beside the manifest
- [ ] 1.3 Nearest names (T5): `strsim` as a direct `uze-core` dependency, used by every unknown-name diagnostic (nodes, properties, roles, slots, steps, declared lookups and templates); tests for `Tabel`, `expect-txt`, `rol`
- [ ] 1.4 `locate(text, offset) -> Place` with the last-good-parse fallback, tested at a node name, after a node, inside a property value and inside `{…}`

## 2. One analysis

- [ ] 2.1 Move fixture binding from `src/cli/extension_check.rs` into a pure function in `uze-extensions` `declared/`, binding every combination of tab choices up to 64 and once per tab past that
- [ ] 2.2 `uze-application` `extensions().analyse(dir, overlay)`: diagnostics with path, line, column, severity and suggestion for the manifest, the views, the tests and the bindings; `check` reduced to it with no overlay
- [ ] 2.3 Tests: a field under a second tab is warned about at its reference; an overlay's unsaved text is what is analysed

## 3. The test runner

- [ ] 3.1 `src/cli/extension_test/`: build the world, draw a surface in its slot's frame (view with attribution, section at sidebar width, drawer in its card) into a `TestBackend`, and keep the renderer's hit rects
- [ ] 3.2 Gestures: `click` and `press-button` by drawn text through the hit map, `press` through `uze-keys` in `Scope::Declared`, a redraw after each; a target not drawn fails naming what was drawn
- [ ] 3.3 Expectations: `expect-text`, `expect-no-text`, `expect-absent` with `because` against the trace, `expect-action` with arguments against the recorded actions
- [ ] 3.4 Snapshots (T3): text and token grids with a legend and the protocol version, colour mapped to tokens through the palette, a row diff on mismatch, `--update-snapshots` writing and naming each file
- [ ] 3.5 `uze agent plugin check` runs every test and fails on a failing one, naming the test, the file and the step's line; the report follows the `design-system` skill (read it first, run its checklist)
- [ ] 3.6 One orchestrator test proving a test's click and the workspace's click on the same surface reach the same node

## 4. The language server

- [ ] 4.1 Dependency review of `lsp-server` and `lsp-types` under the `dependencies` rule; add them to the root crate; `cargo deny check`; `make attributions`
- [ ] 4.2 `uze agent plugin lsp`: stdio loop, `initialize` with capabilities, full document sync, file kind by path, debounced `analyse` on a worker, diagnostics published for every file of the extension; classified in `command_performance.rs`
- [ ] 4.3 Completion: nodes valid at the place, a node's properties, allowed values (roles, slots, declared lookups as `{table:…}`), fixture keys inside `{…}` at the `each` depth, steps in tests
- [ ] 4.4 Hover from the catalog's docs for nodes, properties, roles and steps
- [ ] 4.5 Tests driving the server over in-memory channels: initialize, open a view with `Tabel`, receive the check's diagnostic with its suggestion; completion after `Progress `; hover on `Segment`

## 5. Scaffold, fixtures and docs

- [ ] 5.1 `uze agent plugin create --extension` writes a passing test and records its snapshot; the scaffold test proves the check runs it
- [ ] 5.2 Tests for `gsd-band` and `gsd-panel` in `tests/_fixtures/canonical/gsd/` (tab switch, the next-step button recorded, a snapshot per theme)
- [ ] 5.3 The `uze:author` Skill: "Test an extension" and "Point your editor at the LSP" (Helix, Neovim, VS Code)
- [ ] 5.4 `docs/architecture/crate-layering.mmd`: the `lsp-server` edge from the root crate; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 5.5 Mark tasks 23.5 and 23.6 of `add-extension-platform` done and note what this change decided in D26

## 6. Proof

- [ ] 6.1 `make check`, `openspec validate --all --strict`, and the operator's validation by hand: a test failing and passing, a snapshot updated, and the LSP in the operator's editor
