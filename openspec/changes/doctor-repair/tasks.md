## 1. The catalogue

- [ ] 1.1 Make `UzeHome` enumerable: one list of owned paths, each with its tier (bytes, store, record, authored, generated, remembered). `every_path_uze_owns_is_named_in_the_map` reads this list.
- [ ] 1.2 Add the `HealthCheck` catalogue in `uze-application`, with the subjects, observations and lanes of design D2.
- [ ] 1.3 Add `every_owned_path_has_a_health_check`, covering every `UzeHome` path, every `ManagedArtifact` kind and every vendor registration kind.
- [ ] 1.4 Plain `doctor`, `doctor --repair` and the TUI read the catalogue. The automatic lane keeps exactly its ADR-037 subset. A finding whose lane is `--repair` shows `uze doctor --repair` as its action.
- [ ] 1.5 Make `doctor` isolate every check's failure. Add a test that corrupts each record in turn and still gets every other finding.
- [ ] 1.6 Add a read-only `[marketplaces.*]` reader to `codex/inventory.rs`, so plain `doctor` checks Codex's registration without spawning the vendor CLI.

## 2. Repair

- [ ] 2.1 Add `UzeApplication::repair_plan()` from live inspection, and `repair(plan)` in design D5 order, under the mutation lock. The catalogue then observes again.
- [ ] 2.2 Add re-acquisition from the registry's provenance (Git at the recorded commit, local path, or linked target). Keep the bytes only on a digest match. Otherwise report `NeedsHuman` with `uze update <id>`. Offline, report `NeedsNetwork`. Embedded packages are re-ingested from the binary's snapshot, recording its digest.
- [ ] 2.3 Repoint a marketplace whose registered root differs from its vendor store, never during a session of that harness, reusing the vendor-stores registration code.
- [ ] 2.4 Clear a stale terminal-runtime endpoint, and recreate the shims.
- [ ] 2.5 Report each finding as `Repaired`, `NeedsHuman`, `NeedsNetwork` or `Unrepairable`, with its reason and command. Exit status 0, 1 or 2. The report follows the `design-system` skill.
- [ ] 2.6 Add `doctor --repair` to the grammar and help text. It skips bootstrap and is classified `JustifiedSlow`. `doctor_meets_the_budget` covers plain `doctor` reading the catalogue.

## 3. Proof

- [ ] 3.1 Add `tests/acceptance/repair.rs`, a damage matrix. For each subject in design D2, damage it, run `doctor --repair`, and assert that the machine is healthy or that the expected `NeedsHuman` or `NeedsNetwork` is reported. A second run repairs nothing.
- [ ] 3.2 Add negative tests. Repair never:
  - overwrites an edited build;
  - keeps bytes whose digest differs;
  - re-authorizes;
  - touches an unreadable ledger;
  - repoints during a session;
  - touches a project.
- [ ] 3.3 Add a conformance check on every vertical. Delete `sources/`, register Codex's marketplace at an old root, and leave a stale endpoint. Then run `uze doctor --repair`. Plugins must load, a deny hook must deny, an `always_on` rule must reach the session, and every package's digest must be unchanged.
- [ ] 3.4 Run `make check` and `openspec validate --all --strict`.
