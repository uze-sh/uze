## 1. Evidence first

- [x] 1.1 Make the 2026-11-01 sandbox measurements re-runnable as `conformance/experiments/{claude,codex,antigravity}/source-tie.py` (design.md, Context):
  - Claude loads in place, and a same-name `add` repoints it.
  - Codex needs its local root, and needs `remove` then `add` to repoint, with plugins staying enabled across it.
  - `codex plugin add` over an installed plugin refreshes its cache.
  - `file://` is refused on both.
- [x] 1.2 Record each fact from 1.1, and Codex's command-and-index-keyed hook trust (`codex/trust.rs`), as a `HarnessFact` with the version it was measured on.
- [x] 1.3 Write `tests/integrations/vendor_neutral.rs::no_harness_artifact_names_outside_its_store`. After the canonical fixtures are installed, it walks every configuration and discovery file under the fake home, not transcripts. It must be red on the current code.

## 2. Tiers, layout and the fenced move

- [x] 2.1 `UzeHome`:
  - `sources_dir()` and `sources/superseded/`;
  - `store_dir(vendor)`: one marketplace of builds (`<id>/`), with no structural subdirectories;
  - `runtime_attachments_dir` reduced to staging;
  - `delivered_packages_dir` kept only for `delivered_root::materialize` in UZE's own extension MCP launcher, documented as UZE-internal;
  - `ensure_layout` creates `sources/`, never `store/plugins`;
  - `delivered_package_dir_of` (today it recognises only `store/plugins`) and `services/disk.rs` (`HomeTier::Packages`) follow `sources_dir`.
- [x] 2.2 Extend the owned-paths layering test (`every_path_uze_owns_is_named_in_the_map`) to cover `sources_dir` and `store_dir(vendor)`.
- [x] 2.3 Add the fence, which runs first. Bump `RawPackageRegistry::SHAPE` and the `attachments.json` shape, each with a ladder rung that carries every entry across, and write both documents before any byte moves. Test that an older shape's reader refuses both by name before it prunes or detaches anything.
- [x] 2.4 Add the move and backfill, under the mutation lock, per package:
  - rename `store/plugins/<id>` and `store/superseded/<id>` into `sources/`;
  - compute the digest from the moved bytes and write it into the registry;
  - remove an empty `store/plugins` left by an older build.

  The step runs whenever any such directory exists, so it is resumable.
- [x] 2.5 Cache the package digest in `packages.json` at ingest, for packages installed after the move.
- [x] 2.6 "Bytes missing": catalogue membership, and each package's origin (from its build receipt), come from the registry and build keys, never from `sources/`. `prune_ghost_registrations` and orphan reconciliation skip an entry that still has a build. `doctor` reports the entry as missing its bytes.

## 3. Builds and publication

- [x] 3.1 The Codex and Antigravity builds mirror the whole package through `shared::tree`, with the exclusions Claude's build uses.
- [x] 3.2 `shared::package_root` resolves `${PLUGIN_ROOT}` to the build directory on every route and file kind of a harness.
- [x] 3.3 The build receipt has its own resource identity (`build`), and `attach.rs` filters receipts by kind, so a build receipt never reads as an installed plugin. Its `detail` records `origin`, `source: pinned | linked`, and the build key (package digest, UZE build version, dialect version, build root, active name) and the digest of every file UZE wrote.
- [x] 3.4 Publication into the one vendor marketplace (`uze-store`). `origin` is decided per package and per harness by ADR-013's precedence:
  - stage and swap only a build whose key differs;
  - swap the catalogue last;
  - receipt `IntegrationOwned { kind: "build" }` per (vendor, package), inspected by per-file digest, where a hook-added file is not drift;
  - detached with the package's other receipts on removal.
- [x] 3.5 Registration:
  - repoint only when the registered root differs, and never while a session of that harness runs;
  - Claude repoints with a same-name `add`;
  - Codex runs `remove` then `add`, re-adding the old root if `add` fails;
  - refresh caches (Codex: `codex plugin add <id>`; Antigravity: `agy plugin install` again, which replaces its copy);
  - delete the old root only when no receipt names it.
- [ ] 3.5b Linked sources:
  - a linked package's build receipt records `source: linked`, and an edit re-renders it in place;
  - the link record keeps the pre-link commit; after `uze market unlink`, the next resolution re-ingests it from the cached clone and re-renders the build as `source: pinned`;
  - offline without that commit's blobs, it reports that it needs the network and keeps the linked build.

  Add tests for each case.
- [x] 3.6 Rewrite `the_delivered_root_is_a_copy_a_write_cannot_carry_into_the_store` and its invariant for builds. Add unit tests that:
  - only a changed key rebuilds;
  - a hook-added file survives an unchanged package;
  - an edited file is `Drifted` and is kept on update;
  - a UZE upgrade re-renders.

## 4. Hooks, rules, OpenCode, the bundled plugin

- [x] 4.1 Hook wrapper (`uze-exec`, `uze-exec.ps1`) goes inside every build that has hooks, at the reserved `hooks/uze-exec`. Ingest refuses a package with a file at that path. Each entry names its own build's wrapper. Order the build before its entries on attach, and the reverse on detach. `materialize_wrapper`/`prune_shared_wrapper` and the "last entry" logic go.
- [x] 4.2 Rules engine: becomes a real file of `plugins/uze`, rendered into its build. The `SETUP_OWNER` entries name it there. Rule sessions stay in `runtime/rules/sessions` as the declared on-demand scratch exception.
- [x] 4.3 OpenCode: hook-group documents go inside each package's build, and `uze.ts` reads `store/opencode/*/`. Session documents stay in `runtime/sessions/opencode` (the on-demand scratch exception, since one shared server serves every launch). `uze.ts` gets a receipt.
- [x] 4.4 The Codex integration owns `~/.agents/skills`, and its copies resolve into `store/codex`. Document that OpenCode reads it too.
- [x] 4.5 Codex: UZE's hook groups are always written after the operator's groups in each event array.
  - Test that the trust keys of operator groups placed first never change.
  - Removing a package's groups, or purging, reports every hook whose trust key shifts, through `codex/trust.rs`.
- [x] 4.6 Attach the bundled `uze` plugin with receipts. `remove` refuses it, and only `uninstall --purge` detaches it.
- [x] 4.7 The bundled skills that run `uze` open with the line saying they do not apply when `uze` is not on `PATH`.
- [ ] 4.8 Show a blocking Codex trust notice in install, `doctor` and the TUI. It names each UZE hook that Codex holds back and the review to do, and clears once `codex/trust.rs` reads the hook as trusted.
- [x] 4.9 `no_harness_artifact_names_outside_its_store` (1.3) is green.

## 5. `uze uninstall`

- [x] 5.1 Skip bootstrap, like `upgrade` in `src/main.rs`. Refuse inside a workspace pane and while agents, clients, the terminal runtime with panes, or a background upgrade are alive.
- [x] 5.2 Add `UzeApplication::uninstall_plan(purge)`, the read model of design D6. Run interactively without flags, it asks which mode.
- [x] 5.3 Add `UzeApplication::uninstall(plan)`. It runs the design D6 steps under the mutation lock, after copying the binary's provenance, each step idempotent. With `--purge`:
  - plugins are removed before their marketplaces, which drops Codex's `[plugins]` keys;
  - per harness, readers are removed before the material they read.
- [x] 5.4 `install.ps1` records its user `Path` edit in `install.json`. On Windows, uninstall undoes the edit and removes the running binary on exit.
- [x] 5.5 `install.sh` and `install.ps1` uninstall delegates to `uze uninstall --yes [--purge]` when the binary is present. Otherwise it removes only what the installer placed and prints the reinstall-then-purge path. It never deletes `~/.uze` itself. Update their headers.
- [x] 5.6 Add the `uze uninstall [--yes] [--purge]` grammar, a report following the `design-system` skill, `JustifiedSlow` in `src/command_performance.rs`, and help text.
- [x] 5.7 Add `tests/acceptance/uninstall.rs` with one test per scenario in `specs/machine-uninstall/spec.md`, using fake harnesses and fake processes for the refusals.
- [ ] 5.8 Add `tests/acceptance/migration.rs` covering:
  - the move and digest backfill;
  - an interrupted move;
  - an older shape refusing the fenced documents with no package or receipt lost, and the leftover empty `store/plugins` removed;
  - an upgraded install keeping every plugin enabled.
- [ ] 5.9 Add acceptance tests for the remaining scenarios:
  - a Codex repoint whose `add` fails re-registers the old root (fake vendor CLI);
  - a workspace launch puts its `$UZE_HOME` context only in the process's arguments or environment;
  - deleting one vendor store stops that harness's plugins until the next delivery.

## 5b. Retiring `uze-local`

- [x] 5b.1 `uze-local` retirement (design D5 step 3.2):
  - `receipt_serves` matches `uze-store` by name;
  - each matched `@uze-local` receipt is detached and `@uze-store` attached;
  - a `Drifted` (disabled) Codex plugin is left and reported;
  - the `uze-local` marketplace is removed once no receipt names it.

  Add tests for each case, and a changelog line saying enablement is not carried.
- [x] 5b.2 Turn the 2026-11-01 measurement that mixes origins in one marketplace into `conformance/experiments/{claude,codex}/marketplace-merge.py`, and record it as a `HarnessFact`.

## 6. Conformance

- [ ] 6.1 Contract check, every vertical: with `runtime/` and `cache/` moved away, plugins list and load, a deny hook denies, and an `always_on` rule reaches the session.
- [ ] 6.2 The same contract check with `sources/` moved away.
- [ ] 6.3 The same contract check after the default `uze uninstall --yes`.
- [ ] 6.4 Contract check, every vertical: after `uze uninstall --yes --purge`, no configuration or discovery file names `$UZE_HOME`, and no vendor registry lists a UZE entry.
- [ ] 6.5 Contract check: an install made by the previous release keeps every plugin installed and enabled once upgraded and installed again.

## 7. Docs and diagrams

- [x] 7.1 Update `docs/architecture/containers.mmd`, `core-components.mmd`, `install-pipeline.mmd`, `install-sequence.mmd` and `attachment-lifecycle.mmd`:
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
