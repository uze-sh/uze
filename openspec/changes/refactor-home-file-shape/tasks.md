## 1. Reports survive maintenance

- [x] 1.1 Add `runtime/agents` to `runtime_tenants` and an owned entry for it; register a workspace `Collectable` that removes the directory of an agent no project records
- [x] 1.2 Move the reports lock to `state/reports.lock` (one machine-wide guard) and take it in `uze-workspace/src/reports.rs`
- [x] 1.3 Test: `doctor` while a recorded agent holds the reports lock leaves its reports and the exclusion intact; an unrecorded agent's reports are collected

## 2. Carry steps (after `earlier-release-support` lands)

- [x] 2.1 Every renamed record is a `retired(..)` entry naming the builder it became (`.renamed_to`); its old lock is a retired, unswept entry (an older build still takes it); all `Unbounded`
- [x] 2.2 One carry, `uze_core::earlier::carry_renamed(home, project)`, moves a project's or the machine's renamed records where the new name is free; run by `UzeApplication`'s constructor, `MutationLock::acquire`, `record::ensure` and the three project readers
- [x] 2.3 `theme-overrides.json` is carried into `[appearance.colors|symbols|syntax]` key by key, never over a key the operator has, and removed; `[memory] harness` becomes `native` keeping its comment; a `config.toml` that does not parse stops the carry and leaves the file
- [x] 2.4 A home seeded with the beta.22 names comes out renamed with the same content, twice; a record already at its new name is never overwritten

## 3. New tenants and builders

- [x] 3.1 `runtime_extensions_dir` is the tenant; `compiled_extensions_dir` is `runtime/extensions/compiled/`; views an earlier release kept loose are a retired swept family the compiled-views collector removes through `application/earlier.rs`
- [x] 3.2 `runtime/packages/` → `runtime/servers/`; earlier releases' copies under `runtime/packages/` are a retired tenant, kept while a receipt names one and collected through `application/earlier.rs`; `remove_package` takes a package's old copy through `package/earlier.rs`
- [x] 3.3 `setup_log_path` (`logs/setup/<harness>.log`) and `gate_log_path` (`logs/gate/<agent>.log`); the loose names an earlier release wrote are retired entries the logs' collector removes through `src/earlier.rs`; the map test anchors on `logs_dir()`; the Lab image tails the new path
- [x] 3.4 The runtime projection marker is `root.json`; an earlier release's `project.json` is read through `machine/earlier.rs` until the projection is next prepared, which removes it
- [x] 3.5 Catalogue entries for `runtime_extensions_dir` and `setup_log_path`; retired paths are answered by the Retired subject from the map

## 4. Rename records and their guards

- [x] 4.1 `agents.*` → `tasks.*`, `approved-commands.json` → `approvals.json`, `prompt-history.*` → `prompts.*`
- [x] 4.2 `update.*` → `release.*`, `launch-overrides.*` → `launches.*`, `machine-usage.*` → `usage.*`
- [x] 4.3 `extensions.*` → `extensions/consent.*`
- [x] 4.4 `marketplace-fetch.lock` → `fetch.lock`, `marketplace-mirrors/` → `mirrors/`
- [x] 4.5 Journeys, `journey.py`'s task globs and the conformance seeds spell the new names; `07-upgrade/01` proves beta.22's home carried across (every scene held), reading the release's own `agents.json` only where the release wrote it
- [x] 4.6 `journey validate` green; `cargo test --workspace` green but for the five agent-sandbox tests (and journey 16's same "installed disabled" line) that fail identically on main here

## 5. Rename the remembered tier

- [x] 5.1 `harness_detection.json` → `detection.json`, `machine-overview.json` → `overview.json`, `machine-usage.json` → `transcripts.json`, `release-notes.md` → `changelog.md`; the old names are retired swept entries lasting until beta.22, removed by the retired collector on every bootstrap

## 6. Settings file

- [x] 6.1 `uze_theme::parse_value` reads an override layer already read by its owner, the theme file's shape and checks, naming where it came from; no path
- [x] 6.2 `src/theme.rs` takes the layer from `Themes::overrides` (`config.toml [appearance]`) and names it in a problem; `theme_overrides_path` is a retired authored entry, gone from the catalogue, `uninstall`, `disk` and the docs
- [x] 6.3 Overrides written in `config.toml` survive a theme change; one that does not resolve is reported naming `config.toml [appearance]` and the theme draws without it
- [x] 6.4 `[memory] native` in `src/shim.rs` and `docs/architecture/agent-memory.mmd`

## 7. The name rule and its docs

- [ ] 7.1 Test over `owned_paths()`: no file name in two tiers, no file beside a directory of its stem in one directory (opaque and placeholders skipped)
- [ ] 7.2 `.agents/rules/persistence-tiers.md`: the naming and lock rules and the `config.toml` shape under the authored tier
- [ ] 7.3 Update `docs/architecture/invariants.md` and `docs/versioning.md` where they name old paths or the carry-across contract
- [ ] 7.4 `make check` and `openspec validate --all --strict` green
