## 1. The catalogue

- [ ] 1.1 Tag every `UzeHome` path with its tier in `uze-core`: bytes, store, record, generated, remembered.
- [ ] 1.2 Add the `HealthCheck` catalogue in `uze-application`. Each check has a subject, a tier, `observe` and an optional `repair` with its preconditions. Cover:
  - every `UzeHome` path;
  - every receipt kind, through `IntegrationPort` inspection;
  - every vendor store and registration;
  - the mutation lock, the terminal-runtime endpoint and the shims.
- [ ] 1.3 Add `every_owned_path_has_a_health_check` beside `every_path_uze_owns_is_named_in_the_map`.
- [ ] 1.4 Plain `doctor`, `doctor --repair` and the TUI read the catalogue. Automatic maintenance keeps exactly its ADR-037 subset.

## 2. Recovery before the application

- [ ] 2.1 Add `recover_home(&UzeHome)`:
  - finish interrupted layout moves;
  - run each record's ladder and floor;
  - clear a mutation lock whose holder is dead, through a `uze-platform` process-liveness concept.
- [ ] 2.2 Wire `src/main.rs`:
  - `doctor` runs `recover_home` before `from_env`;
  - `--repair` skips `ensure_default_plugins`;
  - other commands that fail in `from_env` name `uze doctor --repair`.

## 3. Repair

- [ ] 3.1 Add `UzeApplication::repair_plan()` from live inspection, and `repair(plan)`. They run in dependency order (design D5), under the mutation lock, then the catalogue re-observes.
- [ ] 3.2 Store repairs:
  - rebuild builds from `sources/`;
  - re-register or repoint the marketplace (Claude: a same-name `add`; Codex: `remove`, then `add`);
  - refresh the vendor cache;
  - re-seed the bundled plugin.
- [ ] 3.3 Bytes repair: acquire again at the receipt's pinned revision, only when the authorization record exists. Otherwise report `NeedsHuman` with the `add` command. Offline, report `NeedsNetwork`.
- [ ] 3.4 Record repair: ladder, then floor. Report what the vendor stores and registrations still show, with the commands that re-adopt it.
- [ ] 3.5 Process state: clear a stale terminal-runtime endpoint and recreate the shims.
- [ ] 3.6 Report each finding as `Repaired`, `NeedsHuman`, `NeedsNetwork` or `Unrepairable`, with its reason and command. Exit status 0, 1 or 2 (design D6). The report follows the `design-system` skill.
- [ ] 3.7 Add `doctor --repair` to the grammar and help text, and classify it `JustifiedSlow`. Plain `doctor` keeps its budget.
- [ ] 3.8 Findings beyond the automatic lane name `uze doctor --repair` in `doctor` and in the TUI.

## 4. Proof

- [ ] 4.1 Add `tests/acceptance/repair.rs` as a damage matrix. For each owned thing, delete it or corrupt it, then run `doctor --repair` and assert the machine is healthy or the right `NeedsHuman` is reported. A second run repairs nothing.
- [ ] 4.2 Add negative tests. Repair never overwrites a drifted skill, never changes a pinned revision, never re-authorizes, never deletes a record, and never touches a project.
- [ ] 4.3 Add a test that `doctor` starts and repairs on a home where `from_env` fails.
- [ ] 4.4 Add a conformance check on every vertical. Delete `sources/` and the vendor store and corrupt `attachments.json`, then run `uze doctor --repair`. Plugins must load, a deny hook must deny, and an `always_on` rule must reach the session.
- [ ] 4.5 Run `make check` and `openspec validate --all --strict`.
