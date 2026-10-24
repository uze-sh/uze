## Context

This change is change 1 of the split in `add-extension-platform` (design.md,
*Proposed split*). Its decisions are that change's D2 (the contract), D15
(the catalog), D22 (KDL, bounded parsing, compiled views), D23 (binding),
D24 (UI state), D25 (linked extensions, inspector, fixtures) and D28 (the
budget); this document records only what is specific to building the first
slice in this codebase. The map it rests on:

- **A built-in extension is hand-wired in about 22 places**: a module and
  `CATALOG`, an `ExtensionHit` variant per surface, a `Symbol`, a `Scope`
  and `Action::Toggle*` with bindings, model fields and `Remembered` places
  in `src/ui/orchestrator.rs`, channels, `open_*`/`close_*` in
  `orchestrator/surfaces.rs`, `spawn_*`/`absorb_*` in `reads.rs` and
  `answers.rs`, routing in `session/keys.rs`, `session/surface.rs` and
  `session/mouse.rs`, rendering in `render.rs`, `render/tab_strip.rs` and
  `render/sidebar.rs` (the `spec` surface is the template:
  `openspec/changes/archive/2026-10-03-add-the-spec-surface/tasks.md`).
- **The view contract has no layout, table, badge, tabs, button, marks or
  segmented progress** (`crates/uze-extensions/src/view.rs`): `Content` is
  one of `Message`, `Lines`, `Picture`; a `Section` row has one `RowMark`.
- **Plugin bytes live at `$UZE_HOME/store/plugins/<marketplace>/<plugin>/`**
  (`UzeHome::plugin_dir`); `app.plugins().list()` gives each `store_path`
  and a `Linked { checkout }` freshness, but the plugin's directory inside a
  linked working tree is computed only in the private
  `materialize_from_link` (`uze-core` `marketplace.rs:181-209`), and the
  Store keeps install-time bytes even for a linked marketplace.
- **Rules that bite** (`tests/architecture/layering.rs`): `src/` never names
  `uze_core::`; `uze-extensions` names neither `uze_core`, `uze_application`,
  `std::fs`, `std::process` nor `crossterm`; `WorkspaceHost` and
  `tui_application` appear only inside `thread::spawn` in `reads.rs`;
  chrome glyphs only through `theme::glyph(Symbol)`; chrome only from
  `src/ui/widget/`; colour only through `Token`s.

## Goals / Non-Goals

**Goals:**
- The `gsd-band` board drawn from a fixture in a `view` and a `section`,
  edited live from a linked marketplace.
- The generic path every later extension (and, in change 3, the built-ins)
  uses, with nothing hand-wired for the engine.

**Non-Goals:**
- Running anything an extension ships: scripts, programs, grants, consent
  and confinement are changes 4 and 5. A `Button` is recorded, not run.
- Bands, the status row, overlays, media, tests and the LSP.
- Migrating `code`, `architect` and `spec` onto the contract (change 3).

## Decisions

### E1. The order of work, and what can run in parallel

The contract comes first, written once, because every other part compiles
against it:

1. **Foundation** (one hand, before anything else): `uze-contract` (manifest
   and compiled-view types, the binding grammar's types, the catalog's
   names, the sanitizer), the new catalog types in `view.rs`, the
   `Extension` contract with the A1 hooks (E3), the declared engine's entry
   point as a signature, the ADR-041 and AGENTS.md amendment, and the
   layering rules, in one commit.
2. **In parallel**, each in its own checkout (`uze agent work split`):
   - *decoder*: the bounded KDL parse, the hand-written decoder and the
     compiler into `uze-contract` types, discovery of `extensions/*/` in a
     plugin directory, in `uze-core` `capability/extension/`;
   - *engine*: the binder in `uze-extensions/src/declared/` (compiled view
     + data + UI state → `View`/`Section`, the node cap, lookups and
     locale, the "why absent" trace for the inspector);
   - *renderers*: each new catalog type drawn in `src/ui/extension_view*`
     from the widget vocabulary, with its `Symbol`s and TestBackend tests in
     a dark and a light theme;
   - *host*: the generic slot path in the orchestrator, the
     `uze-application` read model of installed extensions with their
     directories (the linked working tree when linked), the compile
     service, reload polling, the error overlay and the inspector drawer;
   - *highlighting*: the KDL v2 grammar for the code surface.
3. **Integration**: the scaffold (`uze agent plugin create --extension`),
   `uze agent plugin check` for extensions, the `gsd-band` fixture plugin,
   and the end-to-end tests.

### E2. `uze-contract` is the only shared vocabulary

- Plain serde types and pure functions: `Manifest`, `SurfaceDecl`
  (`id`, `slot`, `view`, `source`, `title`), `SourceDecl` (`id`,
  `fixture`), `CompiledView` (a tree of `Node { kind, props, children }`
  plus templates and lookups), `Binding` (literal, lone reference, mixed
  text), `Condition` (`when`, `unless`, `show`, `show-any`), `Clean` (a
  sanitized string, built only by the sanitizer), the catalog's node and
  property names, the reserved ids, and the protocol version.
- No I/O, no `uze_*` dependency, no rendering library; `kdl` is not a
  dependency of it, so the client never links a KDL parser it must not
  call.

### E3. The `Extension` contract carries what migration will need

D2's trait, plus what the first review found missing (A1), so change 3 does
not reshape it:
- `lanes()`: whether its jobs run in one serial lane or in parallel;
- every job has a required fallback answer, used when its thread panics;
- `orphaned(answer) -> Option<Outcome>` for an answer whose surface closed;
- `memory()` / `restore_memory()`: state that outlives a surface (a cache,
  a measure), separate from `place()`.
The declared engine implements it; no built-in does yet.

### E4. The generic slot path, used once

- One `ExtensionSlot` map in the orchestrator model, keyed by
  (extension identity, surface id), holding a `Box<dyn Extension>`, its
  place and its last good draw.
- One `ExtensionHit::Declared { slot, surface, hit }` variant, one
  `WorkspaceHit::OpenDeclared(slot)`, one `Scope::Declared` with the generic
  commands (`Command`'s navigation, `NextView` for tabs, `Activate`,
  `Close`), one `spawn_extension_job`/`absorb_extension_answer` pair inside
  `thread::spawn` in `reads.rs`, one `follow_extension_outcome`.
- The tab strip lists each enabled declared `view` beside the built-ins; the
  sidebar draws each declared `section` above the timeline, as `spec`'s is.
- Declared extensions are switched on and off by the same extension switch
  as built-ins (`extension_switch.rs`), by identity.

### E5. Finding an extension's files

A new `uze-application` read model, `installed_extensions()`, answers for
each installed plugin carrying `extensions/*/extension.kdl`: the plugin's
identity, each extension's directory, and whether it is linked. For a linked
marketplace the directory is the plugin's place in the working tree (the
computation `materialize_from_link` does today, made public in `uze-core`),
so a save is seen without reinstalling; otherwise it is the Store's copy.
It runs on the read thread, like every other application read.

### E6. Compiling, caching and reloading

- The compile service (`uze-application`, backed by `uze-core`) parses and
  validates a manifest and its views off the drawing thread and answers a
  compiled extension or its diagnostics. Compiled views are cached in the
  generated tier, keyed by the digest of the files, at a path named in
  `UzeHome`.
- Reload: the client polls the directories of linked extensions every
  250 ms from the existing one-second watch's thread, comparing a digest of
  modification times, and recompiles on change, debounced by 150 ms.
- The last good compiled view and data are kept per slot; an error is drawn
  over them.

### E7. Fixtures

A source's `fixture` is read by the host from the extension's directory,
bounded at 1 MiB, parsed as JSON, and handed to the engine as data; saving
it triggers the same reload as a view.

## Risks / Trade-offs

- [The generic path is laid before any built-in uses it, so it can be wrong
  in ways only migration shows] → E3 carries the first review's A1 list;
  change 3 is where it is proven, behind the built-ins' existing tests.
- [Five parallel streams over one contract] → the foundation commit lands
  first and is not changed by the streams; a needed change goes back
  through it and the streams rebase.
- [The `kdl` dependency] → reviewed under the dependency rule before the
  decoder stream merges; the decoder's guards (pre-scan, size, timed
  worker) are tested against a 400-deep nest and kdl-rs#177's input.
- [`Scope::Declared` is one scope for every declared extension] → actions
  are not keyed in this change (buttons are recorded only); per-extension
  actions and suggested keys arrive with the keymap's second table (change
  4).

## Migration Plan

Additive. The compiled-view cache is new, generated and disposable. An
installed plugin without `extensions/` is unaffected. Rollback is reverting
the change; a plugin's `extensions/` directory is then inert bytes.

## Candidate ADRs

- None new: ADR-041's amendment for `uze-contract` and the KDL dependency
  are already candidates of `add-extension-platform`.
