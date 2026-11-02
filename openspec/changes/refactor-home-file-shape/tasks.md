## 1. Reports survive maintenance

- [x] 1.1 Add `runtime/agents` to `runtime_tenants` and an owned entry for it; register a workspace `Collectable` that removes the directory of an agent no project records
- [x] 1.2 Move the reports lock to `state/reports.lock` (one machine-wide guard) and take it in `uze-workspace/src/reports.rs`
- [x] 1.3 Test: `doctor` while a recorded agent holds the reports lock leaves its reports and the exclusion intact; an unrecorded agent's reports are collected

## 2. Carry steps (after `earlier-release-support` lands)

- [ ] 2.1 For every rename in groups 3–6, add the old path as a `retired(..)` entry; records get `.carried_by(step)` (`Unbounded`), run from `MutationLock::acquire` or `record::ensure`; locks are swept (`Until(Beta(22))`)
- [ ] 2.2 One carry step per project directory moves `agents.*`, `approved-commands.json` and `prompt-history.*` together
- [ ] 2.3 Carry `theme-overrides.json` into `[appearance.*]` and `[memory] harness` to `[memory] native`, stopping and reporting on a `config.toml` that does not parse
- [ ] 2.4 Test: a home seeded with the beta.22 names comes out with the new names and the same content; a rerun changes nothing

## 3. New tenants and builders

- [ ] 3.1 Add a `runtime_extensions_dir` tenant; `compiled_extensions_dir` becomes `runtime/extensions/compiled/`
- [ ] 3.2 `runtime/packages/` → `runtime/servers/` (builder, collector, MCP materialize, uninstall)
- [ ] 3.3 Add `setup_log_path` (`logs/setup/<harness>.log`) used by `src/cli/setup.rs`; `gate_log_path` → `logs/gate/<agent>.log`; anchor the map test on `logs_dir()`; check `Journals` still filters by prefix
- [ ] 3.4 Runtime projection marker `project.json` → `root.json`
- [ ] 3.5 Catalogue entries for every new or moved builder

## 4. Rename records and their guards

- [ ] 4.1 `projects/<p>/agents.{json,lock}` → `tasks.{json,lock}`; `approved-commands.json` → `approvals.json`; `prompt-history.{json,lock}` → `prompts.{json,lock}`
- [ ] 4.2 `update.{json,lock}` → `release.{json,lock}`; `launch-overrides.{json,lock}` → `launches.{json,lock}`; `machine-usage.{json,lock}` → `usage.{json,lock}`
- [ ] 4.3 `extensions.{json,lock}` → `extensions/consent.{json,lock}`
- [ ] 4.4 `marketplace-fetch.lock` → `fetch.lock`; `marketplace-mirrors/<entry>.lock` → `mirrors/<entry>.lock`
- [ ] 4.5 Update journeys (globs and seeds) and conformance seeds (`conformance/contract/{continuity,instructions}.py`) that spell the old names; `07-upgrade/01` proves the beta.22 home is carried across
- [ ] 4.6 `journey validate` and `cargo test --workspace` green

## 5. Rename the remembered tier

- [ ] 5.1 `harness_detection.json` → `detection.json`, `machine-overview.json` → `overview.json`, `machine-usage.json` → `transcripts.json`, `release-notes.md` → `changelog.md`

## 6. Settings file

- [ ] 6.1 `uze-theme`: resolve an override layer from an already-read value (colors, symbols, syntax, glyphs), resolving no path
- [ ] 6.2 `src/theme.rs` builds the override layer from `config.toml` `[appearance.*]` and names `config.toml [appearance.<field>]` in a problem; remove `theme_overrides_path` from the map, catalogue, `uninstall` and `disk`
- [ ] 6.3 Tests for the two new `agent-finished-chime` scenarios: overrides survive a theme change; an unresolvable override is reported and skipped
- [ ] 6.4 `[memory] harness` → `[memory] native` in `src/shim.rs` and its docs

## 7. The name rule and its docs

- [ ] 7.1 Test over `owned_paths()`: no file name in two tiers, no file beside a directory of its stem in one directory (opaque and placeholders skipped)
- [ ] 7.2 `.agents/rules/persistence-tiers.md`: the naming and lock rules and the `config.toml` shape under the authored tier
- [ ] 7.3 Update `docs/architecture/invariants.md` and `docs/versioning.md` where they name old paths or the carry-across contract
- [ ] 7.4 `make check` and `openspec validate --all --strict` green
