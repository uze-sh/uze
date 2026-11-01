## 1. Evidence first

- [ ] 1.1 Turn the 2026-11-01 sandbox measurements (design.md, Context) into `conformance/experiments/{claude,codex,antigravity}/source-tie.py`. Each experiment does these steps:
  1. install;
  2. move the marketplace source or `$UZE_HOME` away;
  3. record `plugin list`;
  4. for Claude, repoint with `marketplace add` of the same name;
  5. for Codex, try a same-name add from another root (refused), then remove and add.
- [ ] 1.2 Measure which vendor command refreshes a cached plugin after a repoint, so the cached `SKILL.md` and `.mcp.json` lose the old path: `claude plugin update`, Codex reinstall, Antigravity reinstall.
- [ ] 1.3 Measure Codex hook trust when a hook command changes (design D6).
- [ ] 1.4 Record each fact from 1.1–1.3 as a `HarnessFact` with the version it was measured on.
- [ ] 1.5 Write `tests/integrations/vendor_neutral.rs::no_harness_artifact_names_uze_home`. It installs the canonical fixtures on the four fake harnesses, then walks the whole fake home except `$UZE_HOME` for the prefix. It must be red on the current code.

## 2. The harness's UZE directory

- [ ] 2.1 Add `IntegrationPort::owned_directory` with one answer per integration. Extend the layering test behind "every path UZE owns is named" to accept `UzeHome` or `owned_directory` and nothing else. Add a test that each integration's directory sits under its own harness root.
- [ ] 2.2 Add the package-root receipt as `IntegrationOwned { kind: "package-root" }`, implemented in `uze-integrations/shared`:
  - attach: build, then write the `.uze-root` marker last;
  - inspect: check the root and its marker;
  - detach: remove the directory whole;
  - a write by a hook is not drift.
- [ ] 2.3 Cache the package digest in `packages.json` at ingest (`serde` default, no rung). Delivery decides by that cached digest and never walks the Store.
- [ ] 2.4 Retire receipts that name `runtime/packages` or `runtime/attachments` by inspect-before-detach. A drifted one is kept and reported.

## 3. Marketplace roots and package roots

- [ ] 3.1 `shared::marketplace` publishes into `owned_directory()/marketplace/<market>`, one staged swap per changed package and the catalogue swapped last.
- [ ] 3.2 Register marketplaces as follows:
  - Claude repoints with a same-name `marketplace add`;
  - Codex runs `marketplace remove` then `add`;
  - each repointed plugin's vendor cache is refreshed by the command 1.2 found.
- [ ] 3.3 Delete the old source only when every receipt that named it is repointed and inspects `MATCHED`.
- [ ] 3.4 `shared::package_root` resolves `${PLUGIN_ROOT}` to `owned_directory()/packages/<id>` on every route and file kind of a harness.
- [ ] 3.5 Remove `UzeHome::delivered_packages_dir` for harness use. `extensions/mcp.rs` keeps a UZE-only copy under `runtime/` and documents why. Narrow `runtime_attachments_dir` to staging that is empty after publication.

## 4. Hooks, rules, OpenCode, the bundled plugin

- [ ] 4.1 Move the wrapper (`exec` and `exec.ps1`) to `owned_directory()/bin/`, still owned by `HookConfigEntry.wrapper`.
- [ ] 4.2 Move the rules engine file to `owned_directory()/rules/` with a receipt. Its sessions go to `rules/sessions`.
- [ ] 4.3 OpenCode: move the hook-group documents to `owned_directory()/hooks/` and the session documents to `owned_directory()/sessions/`. Give `uze.ts` a receipt. The setup sweep and `uze-workspace/conversation.rs` ask the integration for the session directory.
- [ ] 4.4 Attach the bundled `uze` plugin with receipts. `remove` still refuses it, and only uninstall may detach it.
- [ ] 4.5 The bundled skills that run `uze` open with the line saying they do not apply when `uze` is not on `PATH`.
- [ ] 4.6 The doctor sweep from `plugin-freshness-and-linked-marketplaces` treats references into `owned_directory()` as UZE's.
- [ ] 4.7 Report the Codex hooks that need review after the wrapper moves, in the install report and in `doctor`.
- [ ] 4.8 `no_harness_artifact_names_uze_home` (1.5) is green.

## 5. `uze uninstall`

- [ ] 5.1 `uze uninstall` skips bootstrap, like `upgrade` in `src/main.rs`. It refuses inside a workspace pane and while agents, clients, the terminal runtime with panes, or a background upgrade are alive.
- [ ] 5.2 Add `UzeApplication::uninstall_plan()`, a read model holding:
  - per harness, the receipts with their inspection;
  - vendor-written registrations;
  - setup artifacts;
  - the fate of each `$UZE_HOME` top-level entry;
  - records and their size;
  - per project, checkouts and shelves holding unlanded work, and links into `$UZE_HOME`;
  - provisioned harnesses;
  - the binary's provenance;
  - missing vendor CLIs.
- [ ] 5.3 Add `UzeApplication::uninstall(plan, purge)`. It holds the mutation lock and copies the binary's provenance first, then runs these steps in order:
  1. per harness, readers before their material;
  2. workspace links;
  3. the binary, including the Windows `Path` undo and removal on exit;
  4. without `--purge`, clear the registry and ledger and keep the records; with `--purge`, remove `$UZE_HOME` last.

  Each step is idempotent, and the plan is recomputed on every run.
- [ ] 5.4 Record the user `Path` edit in `install.json` when `install.ps1` makes it.
- [ ] 5.5 Add the `uze uninstall [--yes] [--purge]` grammar, a report following the `design-system` skill, `JustifiedSlow` in `src/command_performance.rs`, and help text.
- [ ] 5.6 Add `tests/acceptance/uninstall.rs` with one test per scenario in `specs/machine-uninstall/spec.md`, using fake harnesses and fake processes for the refusals.

## 6. Conformance

- [ ] 6.1 Contract check, every vertical: with `$UZE_HOME` moved away, plugins list and load, a deny hook denies, and an `always_on` rule reaches the session.
- [ ] 6.2 Contract check, every vertical: after `uze uninstall --yes --purge`, a grep of `$HOME` finds no `$UZE_HOME`, and the vendor registries list no UZE marketplace, plugin or MCP entry.
- [ ] 6.3 Contract check: decoy `SKILL.md`, agent and `hooks.json` placed in the UZE directory never load.
- [ ] 6.4 Contract check: an install made by the previous release, upgraded and installed again, keeps every plugin installed and enabled.

## 7. Docs and diagrams

- [ ] 7.1 Update `docs/architecture/containers.mmd`, `core-components.mmd`, `install-pipeline.mmd`, `install-sequence.mmd` and `attachment-lifecycle.mmd`. Harnesses read their UZE directory, and the generated tier is UZE's alone. Run `make artifacts` and `cargo test -p uze-extensions`.
- [ ] 7.2 Update `docs/architecture/invariants.md`:
  - the `${PLUGIN_ROOT}` entry;
  - the owned-paths entry;
  - a new "a harness reads nothing under the UZE home" entry tied to 1.5.
- [ ] 7.3 Update `docs/capabilities/portable-hooks.md` for the wrapper location, and the stale header comments in `claude.rs` and `shim.rs`.
- [ ] 7.4 Run `make check` and `openspec validate --all --strict`.
