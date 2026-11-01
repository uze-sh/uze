## 1. Evidence first

- [ ] 1.1 Make the 2026-11-01 sandbox measurements re-runnable as `conformance/experiments/{claude,codex,antigravity}/source-tie.py` (design.md, Context):
  - Claude loads in place;
  - Codex needs its local root;
  - `file://` is refused on both;
  - Claude repoints with a same-name `add`;
  - Codex needs `remove` then `add`, and plugins stay enabled across it.
- [ ] 1.2 Add the cache-refresh measurement to the Codex experiment: `codex plugin add <id>` over the installed plugin rewrites its cache in place (design D5, measured 2026-11-01).
- [ ] 1.3 Record each fact from 1.1–1.2, and Codex's command-keyed hook trust (`codex/trust.rs`), as a `HarnessFact` with the version it was measured on.
- [ ] 1.4 Write `tests/integrations/vendor_neutral.rs::no_harness_artifact_names_outside_its_store`. It walks the whole fake home except `$UZE_HOME` after the canonical fixtures are installed, and must be red on the current code.

## 2. The tiers

- [ ] 2.1 `UzeHome`:
  - `sources_dir()` for the bytes;
  - `store_dir(vendor)` for the vendor stores;
  - `runtime_attachments_dir` reduced to staging;
  - `delivered_packages_dir` kept only for UZE's extension MCP launcher (`extensions/mcp.rs`), documented.
  - Update the owned-paths layering test to cover these paths.
- [ ] 2.2 Ladder step: move `store/` to `sources/` with one rename under the mutation lock, resumable, with no re-acquisition (persisted-state ADDED requirement).
- [ ] 2.3 Cache the package digest in `packages.json` at ingest (`serde` default, no rung).

## 3. Builds and publication

- [ ] 3.1 The Codex and Antigravity builds mirror the whole package through `shared::tree`, with the exclusions Claude's build uses.
- [ ] 3.2 `shared::package_root` resolves `${PLUGIN_ROOT}` to the build directory in `store/<vendor>` on every route and file kind of a harness.
- [ ] 3.3 `shared::marketplace` publishes per package:
  - stage and swap only a build whose digest differs;
  - write the `.uze-build` marker;
  - swap the catalogue last;
  - own each build by an `IntegrationOwned { kind: "build" }` receipt, whose inspection is "the directory exists and its marker matches".
- [ ] 3.4 Registration:
  - Claude repoints with a same-name `add`;
  - Codex runs `remove`, then `add`;
  - each repointed plugin's vendor cache is refreshed (Codex: `codex plugin add` over the installed plugin; Antigravity: uninstall then install);
  - the old source is deleted only when every receipt that named it has moved.

## 4. Hooks, rules, OpenCode, the bundled plugin

- [ ] 4.1 Move the wrapper (`exec`, `exec.ps1`) to `store/<vendor>/.uze/hooks/`, still owned by `HookConfigEntry.wrapper`.
- [ ] 4.2 Move the rules engine and its sessions to `store/<vendor>/.uze/rules/`, with a receipt for the engine file.
- [ ] 4.3 Move OpenCode's hook documents and session documents to `store/opencode/.uze/`, and give `uze.ts` a receipt. The setup sweep and `uze-workspace/conversation.rs` ask the integration for the directory.
- [ ] 4.4 Attach the bundled `uze` plugin with receipts. `remove` refuses it, and only `uninstall --purge` detaches it.
- [ ] 4.5 The bundled skills that run `uze` open with the line saying they do not apply when `uze` is not on `PATH`.
- [ ] 4.6 Report the Codex hooks that need review after the wrapper moves, in the install report and in `doctor`.
- [ ] 4.7 `no_harness_artifact_names_outside_its_store` (1.4) is green.

## 5. `uze uninstall`

- [ ] 5.1 Skip bootstrap, like `upgrade` in `src/main.rs`. Refuse inside a workspace pane and while agents, clients, the terminal runtime with panes, or a background upgrade are alive.
- [ ] 5.2 Add `UzeApplication::uninstall_plan(purge)`, the read model of design D6.
- [ ] 5.3 Add `UzeApplication::uninstall(plan)`. It runs the steps of design D6 under the mutation lock, after copying the binary's provenance, each step idempotent.
- [ ] 5.4 `install.ps1` records its user `Path` edit in `install.json`. On Windows, uninstall undoes the edit and schedules removal of the running binary on exit.
- [ ] 5.5 Add the `uze uninstall [--yes] [--purge]` grammar, a report following the `design-system` skill, `JustifiedSlow` in `src/command_performance.rs`, and help text.
- [ ] 5.6 Add `tests/acceptance/uninstall.rs` with one test per scenario in `specs/machine-uninstall/spec.md`, using fake harnesses and fake processes for the refusals.

## 6. Conformance

- [ ] 6.1 Contract check, every vertical: with `sources/`, `runtime/` and `cache/` moved away, plugins list and load, a deny hook denies, and an `always_on` rule reaches the session.
- [ ] 6.2 Contract check, every vertical: after the default `uze uninstall --yes`, the same holds.
- [ ] 6.3 Contract check, every vertical: after `uze uninstall --yes --purge`, a grep of `$HOME` finds no `$UZE_HOME`, and no vendor registry lists a UZE entry.
- [ ] 6.4 Contract check: an install made by the previous release keeps every plugin installed and enabled once it is upgraded and installed again.

## 7. Docs and diagrams

- [ ] 7.1 Update `docs/architecture/containers.mmd`, `core-components.mmd`, `install-pipeline.mmd`, `install-sequence.mmd` and `attachment-lifecycle.mmd`: harnesses read `store/<vendor>`, the bytes are `sources/`, and the generated tier is UZE's alone. Run `make artifacts` and `cargo test -p uze-extensions`.
- [ ] 7.2 Update `docs/architecture/invariants.md`: the `${PLUGIN_ROOT}` entry, the Store entries (now `sources/`), and a new "a harness reads only its own vendor store" entry tied to 1.5.
- [ ] 7.3 Update `docs/capabilities/portable-hooks.md`, and the stale header comments in `claude.rs` and `shim.rs`. Document that removing `~/.uze` by hand is unsupported, and name `uze uninstall` and `uze doctor --repair` instead.
- [ ] 7.4 Run `make check` and `openspec validate --all --strict`.
