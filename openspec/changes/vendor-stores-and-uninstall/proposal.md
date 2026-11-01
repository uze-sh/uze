## Why

What UZE delivers to a harness depends on paths under `$UZE_HOME` that
persisted-state does not describe. On a fresh Lab machine (Claude Code
2.1.316, Codex 0.166.0, OpenCode 2.2.5, Antigravity 1.4.12, uze
1.0.0-beta.22), 14 files under the four harness homes name
`~/.uze/runtime`. These are marketplace sources, `${PLUGIN_ROOT}`, hook
wrappers, the rules engine and session files. Persisted-state calls `runtime/`
the generated tier, "deleting it costs nothing", and every harness reads it:

- Claude loads a local marketplace in place. Without the source, the plugin
  is `✘ failed to load`.
- Codex fails its whole `plugin list` when a local marketplace root is
  missing.
- A deny hook whose wrapper is gone blocks every matching tool call.

No vendor offers a way out of this. Claude and Codex treat a local
marketplace as a reference, not an import. Both refuse `file://` (measured),
so they never take a snapshot of their own. Only Antigravity copies on
install. Whatever a harness installs from must therefore stay, and the
question is where it stays and what it is called.

There is also no way to take UZE off a machine. `remove` undoes one package at
a time, and nothing undoes `setup`.

## What Changes

- **`~/.uze/store/<vendor>/` is what each harness reads.**
  - Each harness gets one directory, `store/<vendor>/`. It is the one
    marketplace UZE registers with that harness, and it holds only builds:
    `store/<vendor>/<id>/` is everything one package needs there, including
    the plugin, its `${PLUGIN_ROOT}` and its hook wrapper. The rules engine
    ships in the bundled `uze` plugin's build.
  - No file a harness reads and no configuration entry UZE writes names a
    path outside that directory. The workspace's per-process launch context
    is the one exception.
  - Deleting `store/<vendor>` uninstalls that harness's plugins, and
    persisted-state says so as a tier of its own.
- **BREAKING (on-disk): the canonical package bytes move from `store/` to
  `sources/`.** The bytes are unchanged, and no harness reads them. The
  package registry and the receipt ledger change shape in the same step, so
  an older build refuses them by name instead of writing into the new
  layout. The supported way back is `uze uninstall --purge` followed by a
  reinstall.
- **One marketplace per vendor.** `uze-local`, for author envelopes, merges
  into `uze-store`, and whether a build came from the author's envelope or
  was generated becomes a field of the build (ADR-013's precedence was
  already per package and per harness). The merge moves each
  `<p>@uze-local` to `<p>@uze-store`, and was measured:
  - **Claude:** carries the disabled state.
  - **Codex:** has no disable verb, so a plugin the operator disabled is
    left out, not re-enabled, and the report names it.
- **Missing bytes never cost a delivery.** A package whose `sources/` entry
  is gone keeps its builds, its catalogue entries and its receipts, and is
  reported as missing its bytes.
- **One build per harness, with no extra copy.** Each build carries the whole
  package and is its own root. `runtime/packages` is removed.
- **`runtime/` and `cache/` are disposable for real.** They hold staging,
  launch context and UZE's own extension processes.
- **Codex hook trust is respected.** UZE's groups always come after the
  operator's own, so adding or removing them never shifts the operator's
  trust keys. The one-time move of the wrapper is announced with a blocking
  notice, because Codex holds back a hook whose command changed until the
  person reviews it.
- **Machine-level deliveries are ordinary deliveries.** The rules engine,
  OpenCode's `uze.ts`, its hook documents and the bundled `uze` plugin are
  attached with receipts. Skills that call `uze` say they do not apply when
  it is absent.
- **New `uze uninstall`.** A machine-scope command that runs no bootstrap,
  shows a plan, and acts only once confirmed. Run interactively, it asks
  which mode.
  - **Default** removes the program: the binary, `runtime/`, `cache/`,
    `shims/` and the workspace links. It keeps `sources/`, `store/`,
    `state/` and the operator's authored files. Everything delivered keeps
    working, and a reinstall resumes offline with no new delivery.
  - **`--purge`** also uninstalls every delivered plugin through its
    vendor's CLI, removes UZE's configuration entries (keeping the
    operator's Codex hook trust intact), and removes `$UZE_HOME`.
  - **In either mode** it refuses inside a workspace pane and while UZE
    agents, clients or the runtime are alive. It never touches a project or
    unlanded work, and it leaves edited artifacts in place and lists them.
  - The installers' `-Uninstall`/`--uninstall` delegate to it, instead of
    deleting `~/.uze` themselves.
- **Removing `~/.uze` by hand is unsupported and documented as such.**
  `uze uninstall` is the way out. `doctor-repair` (a separate change) is the
  way back from a damaged home.

## Capabilities

### New Capabilities
- `machine-uninstall`: removing UZE from a machine, with or without what it
  delivered. Covers the plan, the receipt-driven detach, what each mode keeps,
  the refusals, and the binary.

### Modified Capabilities
- `transparent-harness-attachment`: a harness reads only its own vendor store
  under `$UZE_HOME`. A marketplace UZE registers is that store.
- `package-delivery-fidelity`: `${PLUGIN_ROOT}` is the package's build for
  that harness, which holds the whole package.
- `portable-hooks`: the wrapper lives in the harness's vendor store and keeps
  working after `uze uninstall`.
- `persisted-state`: a new **store** tier (what harnesses read; deleting it
  uninstalls their plugins), and the bytes tier is renamed `sources/`.

## Impact

- `crates/uze-core`:
  - `machine/home.rs` gains `sources_dir`, and `store_dir(vendor)` now names
    the vendor stores;
  - the ladder step that moves `store/` to `sources/`;
  - `runtime/packages` is reduced to UZE's own use;
  - the package digest is cached at ingest.

  The `Store` type keeps its name and its rule: it never writes a harness
  artifact.
- `crates/uze-integrations`:
  - each vertical publishes into `store/<vendor>`;
  - `shared/package_root.rs` resolves to the build;
  - Codex and Antigravity builds carry the whole package;
  - the wrapper, the rules engine and OpenCode's documents move into the
    vendor store.
- `crates/uze-application` and `src/cli/`:
  - an uninstall service (plan, then execute);
  - receipts for setup's machine artifacts;
  - the `uninstall` verb.
- Tests:
  - `vendor_neutral.rs` walks the whole fake home and allows only
    `store/<vendor>`;
  - `tests/acceptance/uninstall.rs` covers both modes;
  - conformance moves `sources/`, `runtime/` and `cache/` away and proves
    every harness still works.
- Docs:
  - ADR-052 amended (the store tier);
  - `invariants.md`;
  - `portable-hooks.md`;
  - the architecture diagrams that show harnesses reading the generated tier.
