## Why

The extension platform (`add-extension-platform`, frozen) is large, and the
part that decides whether authors will want it is the part that has never
been tried: writing a view in KDL and seeing it on screen as you save. This
change is the first of that plan's eight (*Proposed split*, change 1): the
view engine alone, with data from fixtures, so it runs no third-party code
and none of the plan's open execution findings gate it.

## What Changes

- **A leaf crate for the contract.** `uze-contract` holds the manifest and
  view types, the catalog's names, the binding grammar and the sanitizer:
  serde data with no I/O. It becomes the one UZE crate `uze-extensions` may
  depend on, by a narrow amendment to ADR-041 that lands with it.
- **Views in KDL v2, parsed with bounds.** `uze-core` reads an extension's
  `extension.kdl` and its views behind a size cap, a nesting pre-scan and a
  timed worker thread, with a hand-written decoder that reports file, line,
  column and a suggestion, and compiles them into validated views. The
  workspace client draws from compiled views and never parses KDL.
- **A starting catalog and its renderers.** The nodes the `gsd-band` board
  and a plain panel need: `Stack`, `Row`, `Text`, `Badge`, `Marks`, `Pips`,
  segmented `Progress`, `List`, `Table`, `Tabs`, `Button`, `Activity`,
  `Elapsed`, `Log`, drawn from the widget vocabulary in the active theme.
- **Binding with no expressions.** References, `each`, `when`/`unless`,
  `show`, templates and lookup tables with locales bind a fixture's data to
  a view; UI state (`Tabs`, `Toggle`, selection) is held by the host and
  changes what is drawn without running anything.
- **Fixtures as the only data.** A source declares a fixture; actions are
  recorded with the arguments they would receive and never run.
- **The generic extension path.** The `Extension` contract, one slot map
  and one job pair in the orchestrator, used by the view engine only, with
  no per-extension wiring added for it.
- **Authoring through linked marketplaces.** An extension installed from a
  linked marketplace reloads on save; every extension keeps its last good
  draw on an error and has an inspector showing why a node is absent.
  `uze agent plugin create --extension` scaffolds one and installs it.
- **KDL highlighted** in the code surface's editor, preview and Markdown.
- **Proof:** the `gsd-band` board drawn in a `view` and a `section` from a
  fixture, edited live.

Not in this change: programs and scripts, grants and consent, confinement,
the agent bridge, bands and the status row, media, tests and the LSP. Each
is a later change of the plan.

## Capabilities

### New Capabilities
- `extension-views`: declaring an extension's surfaces and views in KDL, how they are parsed, compiled and bound to data from fixtures, the host-held UI state, reload of linked extensions, the error overlay and the inspector.

### Modified Capabilities

## Impact

- **New crate** `crates/uze-contract`; `crates/uze-core`
  (`capability/extension/`: discovery, the bounded KDL decoder, compilation);
  `crates/uze-application` (a service that finds installed extensions and
  hands compiled views to the client); `crates/uze-extensions` (`declared/`
  engine, the generic contract, catalog types in `view.rs`, the KDL
  grammar); `crates/uze-theme` (symbols for bars and marks); `src/ui`
  (renderers, the generic slot path, reload polling, inspector, error
  overlay); `src/cli` (`--extension` on `uze agent plugin create`).
- **Dependency**: `kdl` (`default-features = false`, `span`), reviewed under
  the dependency rule.
- **Docs and rules**: ADR-041 and AGENTS.md amended for `uze-contract`;
  `tests/architecture/layering.rs`; `docs/architecture/crate-layering.mmd`.
- **Persisted state**: compiled views in the generated tier, keyed by
  digest, named in `UzeHome`.
