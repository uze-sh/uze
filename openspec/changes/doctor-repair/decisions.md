## R1 — The automatic lane rebuilds a missing build, and what a delivery wrote inside it

- status:   pending
- task:     2.1
- commit:   2acd6402
- trigger:  design D2, D5 step 2 and decision 4 say a deleted vendor-store build is already rebuilt by the automatic lane. It is not: `Health::maintain` only probes symlinks, text regions and generated files, and no integration's `repair_missing_receipt` rebuilds a build receipt. Deleting `store/` left every build missing until the next install. A build re-rendered in place also loses what a delivery wrote inside it (OpenCode's hook document).
- decided:  maintenance also takes build receipts (first, per package) and every receipt with a resource identity whose artifact lies in the store tier. A `Missing` build is rendered again through `UzeApplication::publish_build`; a `Missing` in-store artifact is attached again from its resource. Both are skipped while the package's bytes are missing. On plain `doctor` a build's `Matched` verdict comes from the inspection cache (its inspection digests every file); a repair inspects live (`maintain_locked(.., Inspection::Live)`).
- rejected: making builds `--repair`-only (contradicts decision 4); re-delivering the whole package (runs vendor installs beyond the receipt that is missing).
- blast:    `maintenance.rs` (`maintain_locked`, `attach_again`), `application.rs` (`inspect_receipt_cached`).

## R2 — A repoint is not held back for a running session

- status:   pending
- task:     2.3
- commit:   cbba425d, 2acd6402
- trigger:  design D4 and the spec scenario "A running session" ask for no repoint while a session of that harness runs. `vendor-stores-and-uninstall` decisions D8/D13 found that unimplementable (no census of harness processes UZE did not start) and accepted the repoint with no guard; the brief for this change says the same.
- decided:  `--repair` repoints through `IntegrationPort::repoint_registration`, which calls the vendor-stores `marketplace::ensure_registered` unchanged: only when the registered root differs, Codex's old root registered again when the `add` fails. No session check.
- rejected: a guard over UZE's own agents only (a session started outside UZE passes it).
- blast:    `shared/marketplace.rs` (`registration`, `repoint`); the spec's "A running session" scenario and design D4 need the same amendment D13 made.

## R3 — The map's list: one file, tiers by place, locks and retired paths named

- status:   pending
- task:     1.1
- commit:   204d7f94
- trigger:  the task asks for an enumerable `UzeHome` with tiers; parameterised builders, locks and retired paths had no place in "a tier".
- decided:  `machine/home/owned.rs` lists every builder as an `OwnedPath` (name, `Tier`, `Form` = directory/document/lock, `Standing` = current/retired), composed with placeholder segments (`{project}`). `Tier::of` derives a tier from the top-level entry, and a test holds every current entry to it. `every_path_uze_owns_is_named_in_the_map` also fails when a builder of `home.rs` and the list disagree. Disposability (3.1) is left for group 3 to add to `OwnedPath`.
- rejected: a tier only per top-level directory (the catalogue needs each path by name); a disposable flag now (group 3's).
- blast:    `home.rs`, `home/owned.rs`, `tests/architecture/layering.rs`, the `persistence-tiers` rule (bytes are `sources/`, a store tier added).

## R4 — Coverage is by name

- status:   pending
- task:     1.2, 1.3
- commit:   99f5a9d8
- trigger:  a check that covered a whole tier would cover a new path silently, which is what the coverage test is meant to prevent.
- decided:  each `HealthCheck` names the owned paths, artifact kinds (`ManagedArtifact::KINDS`, spelled by `ManagedArtifact::kind`) and registration kinds (`VendorRegistrationKind::ALL`) it answers for, and the lanes it takes with why. Every owned path belongs to exactly one check.
- rejected: deriving coverage from tiers.
- blast:    `catalogue.rs`, `exposure.rs`, `integration.rs`.

## R5 — doctor lists a catalogue finding once, and missing bytes name the repair

- status:   pending
- task:     1.4
- commit:   99f5a9d8
- trigger:  receipts, the registry, the ledger and the setup records already have rows of their own in `doctor`; missing bytes were a `StoreHealth::Quarantined` sentence naming `uze add <id>`, which returns the existing registration without acquiring anything.
- decided:  `HealthFinding::has_own_row` keeps those out of the list; every other finding is a problem or a warning with its action (`uze doctor --repair` for the repair lane). Missing bytes moved out of `StoreHealth` into a `PackageBytes` finding. The TUI raises one alert per such finding (`headline`, detail ending `Run uze doctor --repair`).
- rejected: rendering every finding (says the ledger and each receipt twice).
- blast:    `doctor.rs`, `src/cli/status.rs`, `src/ui/view/health.rs`.

## R6 — Records: every one is the operator's, eight are observed

- status:   pending
- task:     1.2, 1.5
- commit:   99f5a9d8
- trigger:  design D2 gives "rebuildable classes → automatic (existing floor)", but no record under `state/` is set aside automatically today: every `state.rs` reader errors on a record it cannot read.
- decided:  the records check's lane is operator. It observes the machine-wide records through their readers (registry, ledger, provisioning, marketplaces, undelivered, hosts) or as JSON (extension consent, launch overrides); per-project records are declared and not observed, since repair never reaches into a project.
- rejected: an automatic lane no code implements.
- blast:    `catalogue.rs` (`unreadable_records`).

## R7 — Each check is isolated, panics included

- status:   pending
- task:     1.5
- commit:   99f5a9d8
- trigger:  "every check isolates its own failure".
- decided:  each check, and each record read, runs under `catch_unwind`; an error or a panic is a finding of its own (`could not be checked`). Tested by corrupting each of the eight records in turn.
- rejected: isolating errors only (a reader that panics on a malformed record would still end the report).
- blast:    `catalogue.rs`.

## R8 — Codex's registration is read from `[marketplaces.<name>] source`

- status:   pending
- task:     1.6
- commit:   cbba425d
- trigger:  the task names the table but not the key; the vendor-stores design records `[marketplaces.uze-store] source = <root>`.
- decided:  `codex::inventory::recorded_marketplaces` reads each table's `source`. A `config.toml` with no `marketplaces` table says nothing, so plain `doctor` reports nothing rather than guessing (the fake CLIs keep their own record). Claude's `registered_root` now reads through the same `recorded_root`.
- rejected: treating a missing table as "not registered".
- blast:    `codex/inventory.rs`, `codex/plugin.rs`, `claude/plugin.rs`.

## R9 — `doctor --repair` is classified as a flag

- status:   pending
- task:     2.6
- commit:   2acd6402
- trigger:  `CLASSIFICATION` is one entry per leaf command, and `--repair` is a flag of `doctor`, which stays `Budgeted`.
- decided:  `FLAG_CLASSIFICATION` holds `("doctor", "repair", JustifiedSlow(..))`, and a test fails when a listed flag is not a flag of its command.
- rejected: a `doctor repair` subcommand (D1 chose a flag).
- blast:    `src/command_performance.rs`.

## R10 — Restored bytes are staged under `sources/restoring/`

- status:   pending
- task:     2.2
- commit:   2acd6402
- trigger:  `Store::ingest` returns an existing same-origin registration without copying bytes, so nothing could put bytes back; a digest compared after the move would leave unverified bytes in place.
- decided:  `UzeStore::restore_bytes(id, materialized, RecordedDigest::{Kept, Replaced})` copies into `UzeHome::restoring_dir(id)` (a new owned path, bytes tier), digests it with the acquisition's links, and renames it into place only on a match (`Kept`) or always, recording the digest (`Replaced`, the embedded snapshot). No recorded digest is `NothingToCompare`, reported as needing the person.
- rejected: staging inside `plugins/` (read as an unregistered package); the system temp dir (a rename across filesystems).
- blast:    `store.rs`, `home.rs`, `owned.rs`, the catalogue.

## R11 — Where bytes are acquired again from

- status:   pending
- task:     2.2
- commit:   2acd6402
- trigger:  D3 names four sources; the registry records provenance, not always a marketplace record.
- decided:  embedded → `bootstrap::materialize`; Git with its marketplace registered → the mirror at the recorded commit (`materialize_cached`), whose failure is `NeedsNetwork`, as pin restoration treats it; Git with none → `acquire` at the commit; linked → `materialize_from_link`; local → `acquire` of the path. A source that cannot be read is `NeedsHuman` with `uze remove -m <id>`; a digest mismatch is `NeedsHuman` with `uze update <id>`. No trust authority is consulted.
- rejected: telling offline from a vanished local marketplace (Git answers both the same way).
- blast:    `repair.rs`, `marketplace.rs` (`materialize_from_link` visibility).

## R12 — How a repair decides each outcome

- status:   pending
- task:     2.1, 2.5
- commit:   2acd6402
- trigger:  "the observation after is the report" leaves open how a finding before is matched to one after.
- decided:  findings are matched by subject, what they are about, harness, location and owned path. Gone after is `Repaired`; still there takes what its step said, else `NeedsHuman` (operator lane) or `Unrepairable`. A receipt that is `Blocked` only because its package's bytes are missing is in the repair lane and takes the bytes' outcome. Before observing again the repair forgets the vendor CLIs' memoised answers (`uze_integrations::forget_vendor_answers`), since it wrote what they read itself. Pins are restored before the mutation lock is taken, as `maintain` does; every other step runs under one lock.
- rejected: reporting only what the steps did (an edited build would vanish from the report).
- blast:    `repair.rs`, `shared/process.rs`.

## R13 — Which launchers are expected, and what "this binary" is

- status:   pending
- task:     2.4
- commit:   2acd6402
- trigger:  shims exist only after `uze setup <harness>`; nothing records that one existed.
- decided:  a launcher is expected for a detected harness that supports runtime integration and is set up (`status` not `NotConfigured`). It must exist and start `current_exe` (`uze_platform::executable::is_launcher_of`: the link target on Unix, a same-length same-mtime copy on Windows). Repair calls `ensure_runtime_shim`. Both findings are warnings.
- rejected: expecting a launcher for every detected harness (one never set up has none by design).
- blast:    `catalogue.rs`, `repair.rs`, `uze-platform/executable.rs`.

## R14 — The endpoint is the runtime's to judge and clear

- status:   pending
- task:     2.4
- commit:   bcf78a80, 2acd6402
- trigger:  `uze-application` must not depend on `uze-terminal`.
- decided:  `uze_terminal::stale_endpoint` (something at the endpoint, nothing answers, no server claims the workspace) and `clear_stale_endpoint` (under the exclusive workspace lock; a held lock leaves it alone). `uze-application` declares `TerminalEndpoint`, which `src/cli/repair.rs` implements; plain `doctor` adds the finding in `src/main.rs`.
- rejected: deleting the socket file from the application (would race a starting server).
- blast:    `uze-terminal/runtime/endpoint.rs`, `src/cli/repair.rs`, `src/main.rs`.
