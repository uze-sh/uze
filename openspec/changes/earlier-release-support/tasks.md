## 1. Vocabulary, suite and rule

- [x] 1.1 `uze-document`: `Release` (alpha, beta, stable; ordered), `Lasts { Until, Unbounded }`, `FLOOR = Release::Alpha(1)`, `Earlier { name, lasts, spelled, site }` and the `earlier!` macro; `Step` gains `lasts`; widen the crate doc and `document-ladder.md`
- [x] 1.2 Re-export the vocabulary: `uze_core::document::earlier` already reaches `uze-integrations`; `uze_application::earlier` is the door from `src/`
- [x] 1.3 Every `Step` literal (state.rs, store.rs, task.rs, conversation.rs, terminal persist.rs) declares `lasts: Unbounded`; each crate exposes `pub fn earlier()` including its ladders
- [x] 1.4 `tests/architecture/earlier.rs`: floor; `FLOOR <= CARGO_PKG_VERSION`; `lasts` non-decreasing up a ladder; ladders contiguous; every `impl … Shaped for` covered by its crate's `earlier()`
- [x] 1.5 Layering rules: `spelled` literals only in their entry's file; `legacy_`/`retired_`/`pre_`/`earlier_` identifiers outside the homes with today's sites as exact budgets; `serde(alias` anywhere; `serde(default` on a `Shaped` record field without an `Additive` doc line; `Standing::Retired` outside `owned.rs` and its readers. Every message names the rule file and the remedy
- [x] 1.6 `.agents/rules/earlier-releases.md`; point `document-ladder.md` and `persistence-tiers.md` at it (the floor replaces "no supported build reads it"); `uze agent rules check` green

## 2. Home paths

- [x] 2.1 `OwnedPath` gains `.carried_by(fn)`, `.lasts(..)` and `.spelled(..)`; `pre_sources_layout`, `scattered_project_records` and `retired_profiles_path` declare theirs
- [x] 2.2 `carry_into_sources` (still run from `MutationLock::acquire`) and `carry_across_the_move` (still run from `record::ensure`) read their paths from the map; the registry and ledger fences move into `package/earlier.rs` with them
- [x] 2.3 `setup.rs` `RETIRED`, `catalogue.rs` `Subject::Retired`, `schema.rs` and `uze-official` derive from the map
- [x] 2.4 `runtime/attachments` and its per-vendor directories become retired owned entries (unbounded, still a runtime tenant); the integrations' cleanups of what lies inside move with group 3
- [x] 2.5 Lower the budgets; `tests/performance.rs` budgets still hold

## 3. Harness files

- [x] 3.1 `shared/earlier.rs`: the retired `uze-local` marketplace, the runtime-tree leftovers (one `remove_runtime_leftover` for the two detach paths), the staging root and its sweep, and the Skill-wrapper root and cleanup the four verticals share; `uze_integrations::earlier::earlier()` gathers the entries
- [x] 3.2 `opencode/earlier.rs`: the plugin files of their own (`uze-agent-context.ts`, `uze-rules.ts`) and the per-package hook bridge path, read by setup, `rules.rs` and `opencode.rs` where they run today
- [x] 3.3 `antigravity/earlier.rs` (plugins an earlier build installed straight from the Store); the runtime-tree rules engine joins `shared/earlier.rs`; Claude and Codex keep nothing of their own beyond what `shared/earlier.rs` holds (D-8)
- [x] 3.4 Receipt retirement in `lifecycle/attach.rs` stays a general reconciliation, not an entry (D-8)
- [x] 3.5 Budgets unchanged by this group (no marks moved); the Lab passes for every harness: 8/8 legs green (contract and vendor, four harnesses)

## 4. Project files and checkouts

- [x] 4.1 `retired_region` becomes part of `project/earlier.rs` (`uze_core::project_earlier`), the plan and removal move to `application/earlier/regions.rs`, and the CLI, TUI and workspace readers ask for `leftover_regions`; `StaleProjection` stays as the status read model (D9)
- [x] 4.2 `checkout/earlier.rs`: `agent-<n>` slots (`made_before_slots`), `record_on_sight`, `restore_parents` and ending every agent but a shared slot's holder; `slot_owner` and occupancy's release of ended holders stay, as the general rule (D10)
- [x] 4.3 Marks budgets at zero: the list is empty

## 5. Wire and command spelling

- [x] 5.1 `uze-terminal/src/runtime/earlier.rs`: the persisted workspace's rungs and the server older than the claimant record; finding a server of another protocol or at an older endpoint stays in `endpoint.rs`, as the `persisted-state` addressability rule (D11)
- [x] 5.2 `src/earlier.rs` (the library half, which the suite reaches): the refused `--instructions`; `--resumed` and `uze terminal` stay, read by every later release (D11)
- [x] 5.3 Marks at zero; the budget list is gone and any mark fails

## 6. Releases

- [x] 6.1 Each entry's `lasts` reviewed against what carries it out: two bounded at beta.20, the rest `Unbounded` (D12)
- [ ] 6.2 `journeys/suites/07-upgrade/01` runs `1.0.0-beta.22` against this build; `.github/workflows/journeys.yml` path filters cover the `earlier` modules
- [ ] 6.3 Raise `FLOOR` to `Release::Beta(22)` and delete every entry the floor test names, with its tests, in one commit
- [ ] 6.4 Update tests cited by `docs/architecture/invariants.md` that moved or went; `invariants_citations.rs` green
- [ ] 6.5 `make check`, `openspec validate --all --strict`, the journeys gate and the Lab green
