## 1. Vocabulary, floor and enforcement

- [ ] 1.1 `uze-document`: `Release` (ordered, beta below stable), `FLOOR = 1.0.0-beta.22`, `Earlier`; `Step` gains `last_written_by`
- [ ] 1.2 `tests/architecture/earlier.rs`: gather every registry's `earlier()`, every ladder and every retired owned path; fail below the floor naming entry and site; ladders contiguous and declared
- [ ] 1.3 Layering rules: vocabulary scan with today's sites as exact budgets; names written once; no `serde(alias` and no `serde(default` on a `Shaped` record field; `Standing::Retired` only in `owned.rs`; each failure names the rule file and the remedy
- [ ] 1.4 `.agents/rules/earlier-releases.md`; point `document-ladder.md` and `persistence-tiers.md` at it; `uze agent rules check` green
- [ ] 1.5 Every existing ladder rung declares a provisional `last_written_by` (the release before the commit that added the rung); suite green

## 2. Record shapes

- [ ] 2.1 `marketplace_catalogue.rs` `listings` default and the terminal pane-size clamp become rungs of their records
- [ ] 2.2 Lower the budgets to the new counts

## 3. Home paths

- [ ] 3.1 `OwnedPath` gains `.carried_by(fn)` and `.last_written_by(r)`; one `earlier::carry_home` runs at bootstrap under the mutation lock
- [ ] 3.2 `carry_into_sources` (out of `MutationLock::acquire`) and `carry_across_the_move` (out of `record::ensure`) become carried retired entries; the registry and ledger fences move with them
- [ ] 3.3 `setup.rs` `RETIRED`, `catalogue.rs` `Subject::Retired` and `schema.rs` derive from the map
- [ ] 3.4 `runtime/attachments`, `rules/engine` and the runtime-tree detach removals become retired entries collected by one collector
- [ ] 3.5 Lower the budgets

## 4. Harness files

- [ ] 4.1 `IntegrationPort::retire_earlier` (default empty), called once from the attach and detach lifecycle
- [ ] 4.2 `shared/earlier.rs`: retired marketplace names (`uze-local`; `uze-official` from `setup.rs`), skill-wrapper roots (one implementation for four verticals)
- [ ] 4.3 `opencode/earlier.rs`: runtime-plugin `RETIRED` files, `uze-rules.ts`, `hooks-<pkg>.ts` bridges out of `bridge.rs`, `rules.rs` and `opencode.rs`
- [ ] 4.4 `claude/earlier.rs`, `antigravity/earlier.rs`, `codex/earlier.rs` for what each alone handles; earlier-build artifacts accepted as matched (`hooks.rs:349`, `rules.rs:423-450`)
- [ ] 4.5 Receipt retirement in `lifecycle/attach.rs` declares its entries
- [ ] 4.6 Lower the budgets; the Lab passes for every harness

## 5. Project files and checkouts

- [ ] 5.1 `retired_region` becomes `project/earlier.rs`; `StaleProjection` folds into `RetiredRegionsPlan`; CLI, TUI and workspace readers consume the plan only
- [ ] 5.2 `checkout/earlier.rs`: `agent-<n>` slots, several agents naming one slot, `record_on_sight`, `restore_parents`, `slot_owner`, occupancy's tolerance
- [ ] 5.3 Lower the budgets

## 6. Wire and command spelling

- [ ] 6.1 `uze-terminal/src/runtime/earlier.rs` from `endpoint.rs`'s older-protocol paths
- [ ] 6.2 `src/cli/earlier.rs`: `--resumed`, `uze terminal`, the refused `--instructions`
- [ ] 6.3 Budgets at zero; remove the budget lists

## 7. Apply the floor

- [ ] 7.1 For every entry, set `last_written_by` from `git tag --contains` of the commit that stopped writing what it handles
- [ ] 7.2 Delete every entry the floor test names, with its tests
- [ ] 7.3 `journeys/suites/07-upgrade` runs `1.0.0-beta.22` against this build
- [ ] 7.4 `make check`, `openspec validate --all --strict`, the journeys gate and the Lab green
