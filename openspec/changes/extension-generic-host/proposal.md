## Why

The workspace hosts its three built-in extensions (`code`, `architect`,
`spec`) through about 590 hand-wired sites: model fields, channels, typed
`spawn_*`/`absorb_*` pairs, per-extension hit variants, places and
renders. Declared extensions already go through one generic path. This is
change 3 of `extension-platform`'s split: move the built-ins onto that
path, so a new extension, built-in or declared, costs no orchestrator code,
and the contract the declared engine implements is proven by the hardest
extensions UZE has.

## What Changes

- The `Extension` contract's jobs become real: the host runs an
  extension's `jobs()` on a thread with the workspace's `Host`, and hands
  each answer back to `absorb`, through one `spawn_extension_job` /
  `absorb_extension_answer` pair, which replaces the typed pairs.
- `architect`, then `spec`, then `code` implement `Extension` and are
  hosted in the same slot map as declared extensions. Their reads
  (artifacts, specs and their summary, diffs, changes, files, the map's
  measures) become jobs.
- `ExtensionHit` becomes `{ slot, surface, hit }`; the per-extension
  variants go.
- One `(extension, checkout) -> Place` map replaces `code_places`,
  `architect_places` and `spec_places`.
- **No behaviour changes**: every orchestrator, TestBackend and journey
  test stays green, unchanged except where it names a removed type.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
<!-- none: no observable behaviour changes -->

## Impact

- `crates/uze-extensions`: an `Extension` implementation per built-in, and
  the `ExtensionHit` struct.
- `src/ui/orchestrator*`: the per-extension fields, channels, reads,
  answers, hit routing and places are removed in favour of the slot map;
  `reads.rs` gains the generic job runner (the only place `WorkspaceHost`
  is named outside a typed read, inside `thread::spawn`).
- `docs/architecture/invariants.md` and the extensions rule: `ExtensionHit`
  and the one-variant-per-surface rule change.
