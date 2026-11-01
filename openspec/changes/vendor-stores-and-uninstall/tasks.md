## 1. Evidence first

- [ ] 1.1 Make the 2026-11-01 sandbox measurements re-runnable as `conformance/experiments/{claude,codex,antigravity}/source-tie.py` (design.md, Context):
  - Claude loads in place, and a same-name `add` repoints it.
  - Codex needs its local root, and needs `remove` then `add` to repoint, with plugins staying enabled across it.
  - `codex plugin add` over an installed plugin refreshes its cache.
  - `file://` is refused on both.
- [ ] 1.2 Record each fact from 1.1, and Codex's command-and-index-keyed hook trust (`codex/trust.rs`), as a `HarnessFact` with the version it was measured on.
- [ ] 1.3 Write `tests/integrations/vendor_neutral.rs::no_harness_artifact_names_outside_its_store`. After the canonical fixtures are installed, it walks every configuration and discovery file under the fake home, not transcripts. It must be red on the current code.

## 2. Tiers, layout and the fenced move

- [ ] 2.1 `UzeHome`:
  - `sources_dir()` and `sources/superseded/`;
  - `store_dir(vendor)` with its `local/`, `generated/` and `.uze/`;
  - `runtime_attachments_dir` reduced to staging;
  - `delivered_packages_dir` kept only for `delivered_root::materialize` in UZE's own extension MCP launcher, documented as UZE-internal;
  - `ensure_layout` creates `sources/`, never `store/plugins`.
- [ ] 2.2 Extend the owned-paths layering test (`every_path_uze_owns_is_named_in_the_map`) to cover `sources_dir` and `store_dir(vendor)`.
- [ ] 2.3 The move: per package, under the mutation lock, rename `store/plugins/<id>` and `store/superseded/<id>` into `sources/`. It runs whenever any of them exists, so it is resumable.
- [ ] 2.4 The fence: bump `RawPackageRegistry::SHAPE` and the `attachments.json` shape in the same step, each with a ladder rung that carries every entry across. Test that an older shape's reader refuses both by name.
- [ ] 2.5 Cache the package digest in `packages.json` at ingest.
- [ ] 2.6 "Bytes missing": catalogue membership comes from the registry and build keys, never from `sources/`. `prune_ghost_registrations` and orphan reconciliation skip an entry that still has a build. `doctor` reports the entry as missing its bytes.

## 3. Builds and publication

- [ ] 3.1 The Codex and Antigravity builds mirror the whole package through `shared::tree`, with the exclusions Claude's build uses.
- [ ] 3.2 `shared::package_root` resolves `${PLUGIN_ROOT}` to the build directory on every route and file kind of a harness.
- [ ] 3.3 The `.uze-build` manifest records the build key (package digest, UZE build version, dialect version, build root, active name) and the digest of every file UZE wrote.
- [ ] 3.4 Publication per marketplace root (`local/`, `generated/`):
  - stage and swap only a build whose key differs;
  - swap the catalogue last;
  - receipt `IntegrationOwned { kind: "build" }` per (vendor, origin, package), inspected by per-file digest, where a hook-added file is not drift;
  - detached when the last scope referencing the package removes it.
- [ ] 3.5 Registration:
  - repoint only when the registered root differs, and never while a session of that harness runs;
  - Claude repoints with a same-name `add`;
  - Codex runs `remove` then `add`, re-adding the old root if `add` fails;
  - refresh caches (Codex: `codex plugin add <id>`; Antigravity: uninstall then install);
  - delete the old root only when no receipt names it.
- [ ] 3.6 Rewrite `the_delivered_root_is_a_copy_a_write_cannot_carry_into_the_store` and its invariant for builds. Add unit tests that:
  - only a changed key rebuilds;
  - a hook-added file survives an unchanged package;
  - an edited file is `Drifted` and is kept on update;
  - a UZE upgrade re-renders.

## 4. Hooks, rules, OpenCode, the bundled plugin

- [ ] 4.1 Move the wrapper (`exec`, `exec.ps1`) and the rules engine and its sessions to `store/<vendor>/.uze/`. Their existing receipts (`HookConfigEntry.wrapper`, `SETUP_OWNER`) name the new paths.
- [ ] 4.2 Move OpenCode's hook and session documents to `store/opencode/.uze/`, and give `uze.ts` a receipt. The setup sweep and `uze-workspace/conversation.rs` ask the integration for the directory.
- [ ] 4.3 The Codex integration owns `~/.agents/skills`, and its copies resolve into `store/codex`. Document that OpenCode reads it too.
- [ ] 4.4 Codex: UZE's hook groups are always written after the operator's groups in each event array. Add a test that removing them leaves every operator trust key unchanged.
- [ ] 4.5 Attach the bundled `uze` plugin with receipts. `remove` refuses it, and only `uninstall --purge` detaches it.
- [ ] 4.6 The bundled skills that run `uze` open with the line saying they do not apply when `uze` is not on `PATH`.
- [ ] 4.7 Show a blocking Codex trust notice in install, `doctor` and the TUI. It names each UZE hook that Codex holds back and the review to do, and clears once `codex/trust.rs` reads the hook as trusted.
- [ ] 4.8 `no_harness_artifact_names_outside_its_store` (1.3) is green.

## 5. `uze uninstall`

- [ ] 5.1 Skip bootstrap, like `upgrade` in `src/main.rs`. Refuse inside a workspace pane and while agents, clients, the terminal runtime with panes, or a background upgrade are alive.
- [ ] 5.2 Add `UzeApplication::uninstall_plan(purge)`, the read model of design D6. Run interactively without flags, it asks which mode.
- [ ] 5.3 Add `UzeApplication::uninstall(plan)`. It runs the design D6 steps under the mutation lock, after copying the binary's provenance, each step idempotent. With `--purge`:
  - plugins are removed before their marketplaces, which drops Codex's `[plugins]` keys;
  - per harness, readers are removed before the material they read.
- [ ] 5.4 `install.ps1` records its user `Path` edit in `install.json`. On Windows, uninstall undoes the edit and removes the running binary on exit.
- [ ] 5.5 `install.sh` and `install.ps1` uninstall delegates to `uze uninstall --yes [--purge]` when the binary is present. Otherwise it removes only what the installer placed and prints the reinstall-then-purge path. It never deletes `~/.uze` itself. Update their headers.
- [ ] 5.6 Add the `uze uninstall [--yes] [--purge]` grammar, a report following the `design-system` skill, `JustifiedSlow` in `src/command_performance.rs`, and help text.
- [ ] 5.7 Add `tests/acceptance/uninstall.rs` with one test per scenario in `specs/machine-uninstall/spec.md`, using fake harnesses and fake processes for the refusals.
- [ ] 5.8 Add `tests/acceptance/migration.rs`: the move, an interrupted move, an older build refusing the fenced documents, and an upgraded install keeping every plugin enabled.

## 6. Conformance

- [ ] 6.1 Contract check, every vertical: with `runtime/` and `cache/` moved away, plugins list and load, a deny hook denies, and an `always_on` rule reaches the session.
- [ ] 6.2 The same contract check with `sources/` moved away.
- [ ] 6.3 The same contract check after the default `uze uninstall --yes`.
- [ ] 6.4 Contract check, every vertical: after `uze uninstall --yes --purge`, no configuration or discovery file names `$UZE_HOME`, and no vendor registry lists a UZE entry.
- [ ] 6.5 Contract check: an install made by the previous release keeps every plugin installed and enabled once upgraded and installed again.

## 7. Docs and diagrams

- [ ] 7.1 Update `docs/architecture/containers.mmd`, `core-components.mmd`, `install-pipeline.mmd`, `install-sequence.mmd` and `attachment-lifecycle.mmd`:
  - harnesses read `store/<vendor>`;
  - the bytes are in `sources/`;
  - the generated tier is UZE's alone.

  Run `make artifacts` and `cargo test -p uze-extensions`.
- [ ] 7.2 Update `docs/architecture/invariants.md`:
  - the `${PLUGIN_ROOT}` entry;
  - the Store entries (now `sources/`);
  - the derived-artifact entry (rebuilt when its build key changes);
  - a new entry, "a harness reads only its own vendor store", tied to 1.3.
- [ ] 7.3 Update `docs/capabilities/portable-hooks.md` and the stale header comments in `claude.rs` and `shim.rs`. Document that removing `~/.uze` by hand is unsupported, naming `uze uninstall` and `uze doctor --repair`.
- [ ] 7.4 Run `make check` and `openspec validate --all --strict`.
