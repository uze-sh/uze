## 1. Evidence first

- [ ] 1.1 Turn the 2026-11-01 sandbox measurements (design.md, Context and Migration Plan) into `conformance/experiments/{claude,codex,antigravity}/source-tie.py`, so they are re-runnable. Each script installs, moves the marketplace source or `$UZE_HOME` away, and records `plugin list`.
- [ ] 1.2 Add `HarnessFact`s for the facts this change relies on, each with the version it was measured on:
  - Codex needs its local marketplace root to exist;
  - Codex `marketplace remove` uninstalls the marketplace's plugins;
  - Claude `marketplace add` with the same name repoints the source;
  - Antigravity's plugin copy is at `config/plugins/<name>`.
- [ ] 1.3 Write the failing test `tests/integrations/vendor_neutral.rs::no_harness_artifact_names_uze_home`. It walks every receipt artifact and configuration entry of the four fake harnesses for the `$UZE_HOME` prefix, and must be red on the current code.

## 2. The harness's UZE directory

- [ ] 2.1 Each integration names its one UZE directory under its harness root. Core names none. Extend `every_integration_owns_its_skill_root` into an equivalent test for this directory.
- [ ] 2.2 Add the `PackageRoot { path, source_digest }` receipt variant: attach, O(1) inspect by digest, whole-directory detach, and a write by the package's own hook is not drift. Add its ladder rung in `uze-document`.
- [ ] 2.3 Retire receipts in the old shape (paths under `runtime/packages`, `runtime/attachments`) by inspect-before-detach, then attach in the new location.

## 3. Marketplace sources and package roots

- [ ] 3.1 `shared::marketplace` stages into `<harness>/uze/marketplace/<market>` and swaps whole. Claude and Codex register or repoint through `marketplace add` and never remove first.
- [ ] 3.2 `shared::package_root` gets one answer per integration:
  - Claude: `${CLAUDE_PLUGIN_ROOT}`;
  - Codex: the plugin directory in its UZE marketplace;
  - Antigravity: `config/plugins/<name>`;
  - loose routes: `<harness>/uze/packages/<id>`.
- [ ] 3.3 The Codex and Antigravity envelopes mirror the whole package through `shared::tree`, using the Claude envelope's exclusions.
- [ ] 3.4 Remove `UzeHome::delivered_packages_dir` and its callers. Narrow `runtime_attachments_dir` to staging that is empty after each publication.

## 4. Hooks, rules, OpenCode, the bundled plugin

- [ ] 4.1 Move the hook wrapper to `<harness>/uze/bin/exec`, with a receipt. Hook entries name it, and Claude plugin hooks name `${CLAUDE_PLUGIN_ROOT}`.
- [ ] 4.2 Move the rules engine and its per-session files under `<harness>/uze/rules/`, with a receipt. Create the session directory on demand.
- [ ] 4.3 Move OpenCode's hook-group documents and session files under `~/.config/opencode/uze/`. `uze.ts` reads them there and is attached with a receipt.
- [ ] 4.4 Attach the bundled `uze` plugin with receipts. `remove` still refuses it, and only uninstall may detach it.
- [ ] 4.5 The bundled skills that run `uze` open with the line saying they do not apply when `uze` is not on `PATH`.
- [ ] 4.6 `no_harness_artifact_names_uze_home` (1.3) is green.

## 5. `uze uninstall`

- [ ] 5.1 Add `UzeApplication::uninstall_plan()` as a read model containing:
  - receipts per harness with their inspection;
  - setup artifacts;
  - records directory and size;
  - live agents;
  - checkouts with unlanded work;
  - provisioned harnesses;
  - how the binary was installed.
- [ ] 5.2 Add `UzeApplication::uninstall(plan, purge)`. It runs these steps in order, each idempotent:
  1. refuse if agents are live;
  2. stop the terminal runtime, following the `ending-processes` rule;
  3. detach the matched receipts;
  4. remove each now-empty harness UZE directory;
  5. remove `runtime/`, `cache/` and `store/`, plus `state/` under `--purge`;
  6. remove the binary when the installer placed it, and otherwise print the command for the way it was installed.
- [ ] 5.3 Add the `uze uninstall [--yes] [--purge]` grammar, the report following the `design-system` skill, the `JustifiedSlow` class in `src/command_performance.rs`, and help text.
- [ ] 5.4 Add `tests/acceptance/uninstall.rs` with four cases:
  - a clean uninstall leaves no reference and no UZE directory;
  - an edited skill is kept and listed;
  - a second run is clean;
  - a live agent refuses.

## 6. Conformance

- [ ] 6.1 Contract check, every vertical: with `$UZE_HOME` moved away, plugins list and load, a deny hook denies, and an `always_on` rule reaches the session.
- [ ] 6.2 Contract check, every vertical: after `uze uninstall --yes --purge`, nothing under the harness home names the UZE home, and vendor registries list no UZE marketplace, plugin or MCP entry.
- [ ] 6.3 Contract check: a harness ignores its UZE directory, so it loads nothing it was not pointed at.

## 7. Docs and diagrams

- [ ] 7.1 Update `docs/architecture/containers.mmd`, `core-components.mmd` and `install-pipeline.mmd`. The generated tier is no longer read by a harness, and harness-side UZE directories become the delivery target. Run `make artifacts` and `cargo test -p uze-extensions`.
- [ ] 7.2 Update `docs/architecture/invariants.md`: the `${PLUGIN_ROOT}` entry, the Codex envelope entry, and a new "a harness reads nothing under the UZE home" entry tied to 1.3.
- [ ] 7.3 Update `docs/capabilities/portable-hooks.md` for the wrapper location and the OpenCode fail-open-on-missing-directory note, and update the header comments in `claude.rs` and `shim.rs` that still describe links and the `CLAUDE.md` bridge.
- [ ] 7.4 Run `make check` and `openspec validate --all --strict`.
