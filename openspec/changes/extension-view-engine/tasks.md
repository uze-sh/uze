## 1. Foundation (first, one commit, before the parallel streams)

- [x] 1.1 Create `crates/uze-contract`: `Manifest`, `SurfaceDecl`, `SourceDecl`, `CompiledView` (`Node`, templates, lookups), `Binding`, `Condition`, `Clean` and the sanitizer, catalog node and property names, reserved ids and protocol version; serde only, no I/O, no `uze_*`, no `kdl`
- [x] 1.2 Add the new catalog types to `crates/uze-extensions/src/view.rs`: a `Content::Tree` of layout and data nodes (`Stack`, `Row`, `Text`, `Badge`, `Marks`, `Pips`, segmented `Progress`, `List`, `Table`, `Tabs`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`), the section row with several marks, and the `ViewHit`s for a tab, a toggle and a button
- [x] 1.3 Add the `Extension` contract to `uze-extensions` with the A1 hooks (lanes, fallback answers, `orphaned`, memory beside place) and the declared engine's entry-point signature
- [x] 1.4 Amend ADR-041 and AGENTS.md (`uze-extensions` may depend on `uze-contract` only), add layering rules (no I/O, `uze_*` or rendering crates in `uze-contract`; no `kdl` in `uze-extensions` or `src/ui`), and add `uze-contract` to `crate-layering.mmd`
- [x] 1.5 `cargo build --workspace`, `cargo clippy --all-targets -- -D warnings`, the architecture suite, `make artifacts`

## 2. Decoder (parallel, `uze-core` `capability/extension/`)

- [x] 2.1 Dependency review of `kdl` (`default-features = false`, `span`): publisher, `cargo tree`, `cargo deny check`, licences; record the trade-off in the PR
- [x] 2.2 The bounded parse: 64 KiB cap, a KDL-aware nesting pre-scan refusing above 32, a worker thread with an explicit stack and a time limit; tests with a 400-deep nest and kdl-rs#177's input
- [x] 2.3 The hand-written decoder of `extension.kdl` and views into `uze-contract` types with spans: unknown nodes, properties and arguments, repeated properties, type annotations, wrong case and v1 spellings refused with file, line, column and a suggestion
- [x] 2.4 The compiler: references, scopes, `each` nesting ≤ 2, identity properties literal, roles checked, templates acyclic, lookups with defaults and locales; diagnostics collected, not first-only
- [x] 2.5 Discovery of `extensions/*/extension.kdl` in a plugin directory, with reserved ids and protocol refused per extension and the rest of the plugin unaffected

## 3. Engine (parallel, `uze-extensions/src/declared/`)

- [x] 3.1 The binder: compiled view + JSON data + UI state + locale → `View`/`Section`, with typed lone references, mixed text, `each`, `when`/`unless`, `show`/`show-any`, `use`, lookups, and every string through `Clean`
- [x] 3.2 The node cap with a folded notice, and memoization per (data digest, size, UI state)
- [x] 3.3 The "why absent" trace for the inspector, and recorded actions with the arguments they would receive
- [x] 3.4 The declared engine as an `Extension`: slots `view` and `section`, UI state (`ui.<id>`) for `Tabs`, `Toggle` and selection, place and memory
- [x] 3.5 Engine tests from compiled fixtures: each binding rule, the cap, the trace, UI state switching without a rebuild of data

## 4. Renderers (parallel, `src/ui/extension_view*`, `uze-theme`)

- [ ] 4.1 `Symbol`s for bar steps, pips, marks and the activity mark, in every built-in theme and the schema
- [ ] 4.2 A renderer per catalog node from the widget vocabulary, colour through `Token`s, glyphs through `theme::glyph`
- [ ] 4.3 Host-animated `Activity` and ticking `Elapsed` without rebuilding the view
- [ ] 4.4 The attribution strip on a declared `view`, and folding a declared `section` that does not fit
- [ ] 4.5 TestBackend tests per node in a dark and a light theme, following the `design-system` skill's checklist

## 5. Host (parallel, `src/ui/orchestrator*`, `uze-application`, `uze-core`)

- [ ] 5.1 `installed_extensions()` in `uze-application`: identity, directories (the linked working tree when linked, by making `materialize_from_link`'s placement public in `uze-core`), linked or not
- [ ] 5.2 The compile service off the drawing thread, the compiled-view cache in the generated tier named in `UzeHome`, and fixtures read bounded at 1 MiB
- [ ] 5.3 The generic slot path: `ExtensionSlot` map, `ExtensionHit::Declared`, `WorkspaceHit::OpenDeclared`, `Scope::Declared`, one spawn/absorb pair inside `thread::spawn` in `reads.rs`, one outcome follow-up; open, close, keys, pointer, render, tab strip and sidebar for declared surfaces; the extension switch by identity
- [ ] 5.4 Reload of linked extensions: poll every 250 ms, digest of modification times, debounce 150 ms, keep the place
- [ ] 5.5 The last good draw with the error over it, for every declared surface
- [ ] 5.6 The inspector drawer: compiled tree with why a node is absent, data, UI state, recorded actions, last errors
- [ ] 5.7 Orchestrator tests: open and close, tab switching runs nothing, a stale answer dropped, reload keeps the place, an error keeps the last draw

## 6. Highlighting (parallel, `uze-extensions/grammars`)

- [x] 6.1 Try the MIT `eugenesvk/sublime-KDL` KDL2 grammar under `fancy-regex`; if it does not compile, write a small KDL v2 grammar
- [x] 6.2 Vendor it with its licence beside `PowerShell.sublime-syntax`, map `.kdl`, and test it colours `#true`, raw and multi-line strings, slashdash and type annotations in the editor, the preview and Markdown fences

## 7. Integration and proof

- [ ] 7.1 `uze agent plugin check` validates a plugin's extensions (manifest, views, references against fixtures) with the install's rules
- [ ] 7.2 `--extension <id>` on `uze agent plugin create`: manifest, view and fixture in a linked marketplace that pass the check, classified in `command_performance.rs`
- [ ] 7.3 The `gsd-band` fixture plugin (board view and section, fixture from the blueprint) and a journey: install from a linked marketplace → open → switch tab → edit the view → see it redrawn
- [ ] 7.4 `make check`, `openspec validate --all --strict`, and the operator's validation by hand
