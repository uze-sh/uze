## Context

Change 2 of `extension-platform`'s split; its decision there is D26.
What change 1 (`archive/2026-10-24-extension-view-engine`) left to build on:

- `uze-core` `capability/extension/` holds the only KDL decoder, the
  bounded parse and `Diagnostic` (offset, message, suggestion). It reads
  files from an extension's directory.
- `uze-contract` holds the catalog (`NodeName`, `PropKind`, `RoleName`)
  with names and kinds but no descriptions.
- `uze-extensions` `declared/` holds `DeclaredExtension`. It is pure: data
  and UI state in, `View`/`Section` out, with `trace()` for why a node is
  absent and the recorded actions.
- The renderers are in `src/ui/extension_view*` (ratatui, root crate only).
  The workspace drives a surface through hits (`ViewHit::Node`) and
  `Command`s resolved from `uze-keys` in `Scope::Declared`.
- `uze agent plugin check` is `uze-application`'s `extensions().check()`
  plus `src/cli/extension_check.rs`. That second part binds fixtures in the
  engine and turns missing fields into warnings, but only for the default
  first tab of each `Tabs`.
- `src/` may not name `uze_core::`. Whatever the CLI needs from the decoder
  arrives through `uze-application`.

## Goals / Non-Goals

**Goals:**
- A test drives the same engine, renderer, hit map and keymap the workspace
  uses, so a passing test means the workspace draws the same thing.
- One analysis behind the check and the LSP, so their messages cannot
  drift.

**Non-Goals:**
- `--run` and anything that executes plugin code (change 4/5). Recorded
  actions are the only outcome a test can expect.
- Tests for built-in extensions (`code`, `architect`, `spec`): they keep
  their Rust tests until change 3.
- Formatting, rename, go-to-definition, semantic tokens in the LSP. The
  editor's KDL grammar colours the text.

## Decisions

### T1. Test files are decoded like views, into `uze-contract` types

`tests/*.test.kdl` goes through the same bounded parse (64 KiB, depth 32,
timed worker) and a hand-written decoder in `capability/extension/test.rs`.
The result is `uze_contract::TestFile { tests: Vec<TestDecl> }`, where
`TestDecl` is a name, a world (surface, data per source, size, theme,
locale) and a list of `Step`s, each with its span:

```kdl
test "the next step is offered" {
    surface "panel"
    given "session" fixture="fixtures/session.json"
    size 46 40
    locale "pt"
    expect-text "Fases"
    click "Activity"                 // a tab, a row or any drawn text
    press "down"                     // a chord, through the keymap
    press-button "▶ /gsd-discuss-phase 43"
    expect-action "next" command="/gsd-discuss-phase 43"
    expect-absent "Button" because="next"
    snapshot "panel"
}
```

Steps are lowercase directives, as in manifests. Inline data is `given
"session" json=#"{…}"#`. A misspelled step gets the nearest name (T5).

*Alternative:* tests in Rust or JSON. Rejected because D22 already settled
one format for everything an author writes.

### T2. A headless driver lives in the root crate, beside the renderer

`src/cli/extension_test/` runs a `TestDecl`. It builds a
`DeclaredExtension` with the world's data and locale, and draws the
surface with the real renderer into a ratatui `TestBackend` buffer, in the
slot's frame:
- a `view` with its attribution strip;
- a `section` at the sidebar's width;
- a `drawer` inside its card.

It collects the hit rects the renderer registers. A gesture is found in
what was drawn and routed exactly as the workspace's mouse and key paths
route it:
- `click`/`press-button` take the rect of the drawn text;
- `press` resolves the chord through `uze-keys` in `Scope::Declared`.

After every gesture the surface is drawn again. Expectations read the
drawn buffer, the engine's `trace()` and its recorded actions.

The driver is not in `uze-extensions` because rendering is `src/ui`'s and
ratatui is in the root manifest only. It is not in `src/ui/` because it is
a command's work. It may name `crate::ui::extension_view`, as the shim and
the matrix binary already share the crate.

*Alternative:* assert on the `View` tree without rendering. Rejected
because then a test would not prove what the operator sees: wrapping,
truncation, the fold notice and hit geometry all live in the renderer.

### T3. A snapshot is text plus roles, never RGB

A snapshot file (`tests/snapshots/<name>.<slot>.<w>x<h>.<theme>.snap`)
holds the drawn rows, then the same grid as one letter per cell naming the
`Token` that coloured its foreground (with a legend), and a header with
the UZE protocol version.

Colours are mapped back to tokens through the active palette. Where two
tokens share a colour in one theme, the first in `Token` order wins, which
is deterministic. A mismatch prints the differing rows of both grids,
before and after. `--update-snapshots` writes what differs or is missing,
and the report names each file it wrote.

*Alternative:* RGB per cell (like `insta` over a buffer debug). Rejected
because every palette tweak would break every plugin's snapshots; a role
is what the author chose.

### T4. One analysis service, for the check and the LSP

`uze-application` gains `extensions().analyse(dir, overlay)`, where
`overlay` maps paths to unsaved text. It returns every diagnostic for the
manifest, the views, the tests and the fixture bindings, each with its
path, line, column, severity and suggestion.

- The check calls it with no overlay.
- The LSP calls it with its open documents.

The fixture-binding warnings move from `src/cli/extension_check.rs` into
this service (into `uze-extensions` `declared/`, as a function the service
calls), so both callers get them. Binding runs once for each combination
of tab choices, up to a cap of 64 combinations. Past the cap it binds each
tab once with the others at their defaults and says so.

### T5. Nearest names come from the catalog with `strsim`

Every "unknown X" diagnostic offers the closest valid name, taken by
Damerau–Levenshtein ≤ 2 (or ≤ 1 for names of 4 characters or fewer) and
case-insensitive. The candidates are catalog nodes, a node's properties,
roles, slots, steps, and the lookups and templates the file declares.
When none is that close, the diagnostic lists the valid names as today.
`strsim` 0.11 is already in the lock through `clap`; making it a direct
dependency of `uze-core` adds no new crate.

### T6. The LSP is `lsp-server` over stdio, in the root crate

`uze agent plugin lsp` runs a synchronous loop on `lsp-server` and
`lsp-types` from the rust-analyzer project. Neither needs an async
runtime; `crossbeam-channel` is already in the lock.

- **Lifecycle:** it keeps open documents in memory (full sync). A change
  re-runs `analyse` on the document's extension, debounced by 150 ms on a
  worker thread, and publishes diagnostics for every file of that
  extension.
- **File kind:** taken from the path: `extension.kdl`; a file a surface
  names as its view; `tests/*.test.kdl`.
- **Completion and hover:** they need to know what is at the cursor, so
  `uze-core` exposes `locate(text, offset) -> Place` (the node path, the
  property being written, inside `{…}` or not), which tolerates a document
  that does not parse by falling back to the last good parse of that file.
  Candidates and docs come from the catalog, which grows a one-line `doc`
  per node, property, role and step in `uze-contract`, the only source.
- **Fixture fields:** completion inside `{…}` offers the keys of the
  source's fixture at the current `each` depth.

*Alternative:* `tower-lsp`. Rejected because it pulls in `tokio`, which
UZE does not otherwise need on this path. *Alternative:* a separate
`uze-lsp` binary. Rejected because it would be a second artifact to
install and version.

### T7. Docs and the scaffold

- The scaffold writes `tests/board.test.kdl` with a text expectation and a
  snapshot recorded at creation.
- The `uze:author` Skill gains "Test an extension" and "Point your editor
  at the LSP", with Helix, Neovim and VS Code configurations.
- `crate-layering.mmd` gains the `lsp-server` edge from the root crate.

## Candidate ADRs

- **`lsp-server`/`lsp-types` and a language server inside the `uze`
  binary.** It is a new external dependency and a long-lived protocol
  surface that UZE now maintains.

## Risks / Trade-offs

- [Colour → token mapping is ambiguous when tokens share a colour] →
  deterministic first-in-order rule; snapshots are per theme, and a palette
  change that merges two tokens shows as a reviewed snapshot diff.
- [The cross-product of tabs can grow] → a cap of 64 combinations with a
  stated fallback (T4).
- [Renderer changes in UZE break plugins' snapshots] → snapshots carry the
  protocol version; a renderer change that moves cells is called out in the
  release notes, and `--update-snapshots` is one command.
- [`locate` on a broken document] → the last good parse of the file;
  completion degrades to node names at the line's indentation.
- [Hit geometry in a test differs from the workspace's] → the driver uses
  the renderer's own hit registration, and one orchestrator test checks
  that a recorded test and the workspace agree on a click.

## Migration Plan

Additive. A plugin without `tests/` checks exactly as before. Rollback is
reverting the change; test files and snapshots are then inert bytes.
