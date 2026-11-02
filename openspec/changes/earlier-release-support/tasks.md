## 1. Vocabulary, suite and rule

- [x] 1.1 `uze-document`: `Release` (alpha, beta, stable; ordered), `Lasts { Until, Unbounded }`, `FLOOR = Release::Alpha(1)`, `Earlier { name, lasts, spelled, site }` and the `earlier!` macro; `Step` gains `lasts`; widen the crate doc and `document-ladder.md`
- [x] 1.2 Re-export the vocabulary: `uze_core::document::earlier` already reaches `uze-integrations`; `uze_application::earlier` is the door from `src/`
- [ ] 1.3 Every `Step` literal (state.rs, store.rs, task.rs, conversation.rs, terminal persist.rs) declares `lasts: Unbounded`; each crate exposes `pub fn earlier()` including its ladders
- [ ] 1.4 `tests/architecture/earlier.rs`: floor; `FLOOR <= CARGO_PKG_VERSION`; `lasts` non-decreasing up a ladder; ladders contiguous; every `impl … Shaped for` covered by its crate's `earlier()`
- [ ] 1.5 Layering rules: `spelled` literals only in their entry's file; `legacy_`/`retired_`/`pre_`/`earlier_` identifiers outside the homes with today's sites as exact budgets; `serde(alias` anywhere; `serde(default` on a `Shaped` record field without an `Additive` doc line; `Standing::Retired` outside `owned.rs` and its readers. Every message names the rule file and the remedy
- [ ] 1.6 `.agents/rules/earlier-releases.md`; point `document-ladder.md` and `persistence-tiers.md` at it (the floor replaces "no supported build reads it"); `uze agent rules check` green

## 2. Home paths

- [ ] 2.1 `OwnedPath` gains `.carried_by(fn)`, `.lasts(..)` and `.spelled(..)`; `pre_sources_layout`, `scattered_project_records` and `retired_profiles_path` declare theirs
- [ ] 2.2 `carry_into_sources` (still run from `MutationLock::acquire`) and `carry_across_the_move` (still run from `record::ensure`) read their paths from the map; the registry and ledger fences move into `package/earlier.rs` with them
- [ ] 2.3 `setup.rs` `RETIRED`, `catalogue.rs` `Subject::Retired`, `schema.rs` and `uze-official` derive from the map
- [ ] 2.4 `runtime/attachments`, `rules/engine` and the runtime-tree detach removals become retired entries; the integrations' cleanups read them
- [ ] 2.5 Lower the budgets; `tests/performance.rs` budgets still hold

## 3. Harness files

- [ ] 3.1 `shared/earlier.rs`: retired marketplace names (`uze-local`), skill-wrapper roots (one implementation for four verticals); `IntegrationRegistry::earlier()` gathers every vertical's entries
- [ ] 3.2 `opencode/earlier.rs`: runtime-plugin retired files, `uze-rules.ts`, `hooks-<pkg>.ts` bridges, read by `bridge.rs`, `rules.rs` and `opencode.rs` where they run today
- [ ] 3.3 `claude/earlier.rs`, `antigravity/earlier.rs`, `codex/earlier.rs`; earlier-build artifacts accepted as matched (`hooks.rs`, `rules.rs`) declare entries
- [ ] 3.4 Receipt retirement in `lifecycle/attach.rs` declares its entries
- [ ] 3.5 Lower the budgets; the Lab passes for every harness

## 4. Project files and checkouts

- [ ] 4.1 `retired_region` becomes `project/earlier.rs`; `StaleProjection` folds into `RetiredRegionsPlan`; CLI, TUI and workspace readers consume the plan only
- [ ] 4.2 `checkout/earlier.rs`: `agent-<n>` slots, several agents naming one slot, `record_on_sight`, `restore_parents`, `slot_owner`, occupancy's tolerance
- [ ] 4.3 Lower the budgets

## 5. Wire and command spelling

- [ ] 5.1 `uze-terminal/src/runtime/earlier.rs` from `endpoint.rs`'s older-protocol paths
- [ ] 5.2 `src/cli/earlier.rs`: `--resumed`, `uze terminal`, the refused `--instructions`
- [ ] 5.3 Budgets at zero; remove the budget lists

## 6. Releases

- [ ] 6.1 Derive each entry's `lasts` from `git tag --contains` (version order); `Until` only where its runner runs on every run, `Unbounded` otherwise
- [ ] 6.2 `journeys/suites/07-upgrade/01` runs `1.0.0-beta.22` against this build; `.github/workflows/journeys.yml` path filters cover the `earlier` modules
- [ ] 6.3 Raise `FLOOR` to `Release::Beta(22)` and delete every entry the floor test names, with its tests, in one commit
- [ ] 6.4 Update tests cited by `docs/architecture/invariants.md` that moved or went; `invariants_citations.rs` green
- [ ] 6.5 `make check`, `openspec validate --all --strict`, the journeys gate and the Lab green
