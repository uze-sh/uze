## R1 — The automatic lane rebuilds a missing build, and what a delivery wrote inside it

- status:   pending
- task:     2.1
- commit:   a2b219ff
- trigger:  design D2, D5 step 2 and decision 4 say a deleted vendor-store build is already rebuilt by the automatic lane. It is not: `Health::maintain` only probes symlinks, text regions and generated files, and no integration's `repair_missing_receipt` rebuilds a build receipt. Deleting `store/` left every build missing until the next install. A build re-rendered in place also loses what a delivery wrote inside it (OpenCode's hook document).
- decided:  maintenance also takes build receipts (first, per package) and every receipt with a resource identity whose artifact lies in the store tier. A `Missing` build is rendered again through `UzeApplication::publish_build`; a `Missing` in-store artifact is attached again from its resource. Both are skipped while the package's bytes are missing. On plain `doctor` a build's `Matched` verdict comes from the inspection cache (its inspection digests every file); a repair inspects live (`maintain_locked(.., Inspection::Live)`).
- rejected: making builds `--repair`-only (contradicts decision 4); re-delivering the whole package (runs vendor installs beyond the receipt that is missing).
- blast:    `maintenance.rs` (`maintain_locked`, `attach_again`), `application.rs` (`inspect_receipt_cached`).

## R2 — A repoint is not held back for a running session

- status:   pending
- task:     2.3
- commit:   a2bb1a14, 2acd6402
- trigger:  design D4 and the spec scenario "A running session" ask for no repoint while a session of that harness runs. `vendor-stores-and-uninstall` decisions D8/D13 found that unimplementable (no census of harness processes UZE did not start) and accepted the repoint with no guard; the brief for this change says the same.
- decided:  `--repair` repoints through `IntegrationPort::repoint_registration`, which calls the vendor-stores `marketplace::ensure_registered` unchanged: only when the registered root differs, Codex's old root registered again when the `add` fails. No session check.
- rejected: a guard over UZE's own agents only (a session started outside UZE passes it).
- blast:    `shared/marketplace.rs` (`registration`, `repoint`); the spec's "A running session" scenario and design D4 need the same amendment D13 made.

## R3 — The map's list: one file, tiers by place, locks and retired paths named

- status:   pending
- task:     1.1
- commit:   74d90676
- trigger:  the task asks for an enumerable `UzeHome` with tiers; parameterised builders, locks and retired paths had no place in "a tier".
- decided:  `machine/home/owned.rs` lists every builder as an `OwnedPath` (name, `Tier`, `Form` = directory/document/lock, `Standing` = current/retired), composed with placeholder segments (`{project}`). `Tier::of` derives a tier from the top-level entry, and a test holds every current entry to it. `every_path_uze_owns_is_named_in_the_map` also fails when a builder of `home.rs` and the list disagree. Disposability (3.1) is left for group 3 to add to `OwnedPath`.
- rejected: a tier only per top-level directory (the catalogue needs each path by name); a disposable flag now (group 3's).
- blast:    `home.rs`, `home/owned.rs`, `tests/architecture/layering.rs`, the `persistence-tiers` rule (bytes are `sources/`, a store tier added).

## R4 — Coverage is by name

- status:   pending
- task:     1.2, 1.3
- commit:   2f0abd90
- trigger:  a check that covered a whole tier would cover a new path silently, which is what the coverage test is meant to prevent.
- decided:  each `HealthCheck` names the owned paths, artifact kinds (`ManagedArtifact::KINDS`, spelled by `ManagedArtifact::kind`) and registration kinds (`VendorRegistrationKind::ALL`) it answers for, and the lanes it takes with why. Every owned path belongs to exactly one check.
- rejected: deriving coverage from tiers.
- blast:    `catalogue.rs`, `exposure.rs`, `integration.rs`.

## R5 — doctor lists a catalogue finding once, and missing bytes name the repair

- status:   pending
- task:     1.4
- commit:   2f0abd90
- trigger:  receipts, the registry, the ledger and the setup records already have rows of their own in `doctor`; missing bytes were a `StoreHealth::Quarantined` sentence naming `uze add <id>`, which returns the existing registration without acquiring anything.
- decided:  `HealthFinding::has_own_row` keeps those out of the list; every other finding is a problem or a warning with its action (`uze doctor --repair` for the repair lane). Missing bytes moved out of `StoreHealth` into a `PackageBytes` finding. The TUI raises one alert per such finding (`headline`, detail ending `Run uze doctor --repair`).
- rejected: rendering every finding (says the ledger and each receipt twice).
- blast:    `doctor.rs`, `src/cli/status.rs`, `src/ui/view/health.rs`.

## R6 — Records: every one is the operator's, eight are observed

- status:   pending
- task:     1.2, 1.5
- commit:   2f0abd90
- trigger:  design D2 gives "rebuildable classes → automatic (existing floor)", but no record under `state/` is set aside automatically today: every `state.rs` reader errors on a record it cannot read.
- decided:  the records check's lane is operator. It observes the machine-wide records through their readers (registry, ledger, provisioning, marketplaces, undelivered, hosts) or as JSON (extension consent, launch overrides); per-project records are declared and not observed, since repair never reaches into a project.
- rejected: an automatic lane no code implements.
- blast:    `catalogue.rs` (`unreadable_records`).

## R7 — Each check is isolated, panics included

- status:   pending
- task:     1.5
- commit:   2f0abd90
- trigger:  "every check isolates its own failure".
- decided:  each check, and each record read, runs under `catch_unwind`; an error or a panic is a finding of its own (`could not be checked`). Tested by corrupting each of the eight records in turn.
- rejected: isolating errors only (a reader that panics on a malformed record would still end the report).
- blast:    `catalogue.rs`.

## R8 — Codex's registration is read from `[marketplaces.<name>] source`

- status:   pending
- task:     1.6
- commit:   a2bb1a14
- trigger:  the task names the table but not the key; the vendor-stores design records `[marketplaces.uze-store] source = <root>`.
- decided:  `codex::inventory::recorded_marketplaces` reads each table's `source`. A `config.toml` with no `marketplaces` table says nothing, so plain `doctor` reports nothing rather than guessing (the fake CLIs keep their own record). Claude's `registered_root` now reads through the same `recorded_root`.
- rejected: treating a missing table as "not registered".
- blast:    `codex/inventory.rs`, `codex/plugin.rs`, `claude/plugin.rs`.

## R9 — `doctor --repair` is classified as a flag

- status:   pending
- task:     2.6
- commit:   a2b219ff
- trigger:  `CLASSIFICATION` is one entry per leaf command, and `--repair` is a flag of `doctor`, which stays `Budgeted`.
- decided:  `FLAG_CLASSIFICATION` holds `("doctor", "repair", JustifiedSlow(..))`, and a test fails when a listed flag is not a flag of its command.
- rejected: a `doctor repair` subcommand (D1 chose a flag).
- blast:    `src/command_performance.rs`.

## R10 — Restored bytes are staged under `sources/restoring/`

- status:   pending
- task:     2.2
- commit:   a2b219ff
- trigger:  `Store::ingest` returns an existing same-origin registration without copying bytes, so nothing could put bytes back; a digest compared after the move would leave unverified bytes in place.
- decided:  `UzeStore::restore_bytes(id, materialized, RecordedDigest::{Kept, Replaced})` copies into `UzeHome::restoring_dir(id)` (a new owned path, bytes tier), digests it with the acquisition's links, and renames it into place only on a match (`Kept`) or always, recording the digest (`Replaced`, the embedded snapshot). No recorded digest is `NothingToCompare`, reported as needing the person.
- rejected: staging inside `plugins/` (read as an unregistered package); the system temp dir (a rename across filesystems).
- blast:    `store.rs`, `home.rs`, `owned.rs`, the catalogue.

## R11 — Where bytes are acquired again from

- status:   pending
- task:     2.2
- commit:   a2b219ff
- trigger:  D3 names four sources; the registry records provenance, not always a marketplace record.
- decided:  embedded → `bootstrap::materialize`; Git with its marketplace registered → the mirror at the recorded commit (`materialize_cached`), whose failure is `NeedsNetwork`, as pin restoration treats it; Git with none → `acquire` at the commit; linked → `materialize_from_link`; local → `acquire` of the path. A source that cannot be read is `NeedsHuman` with `uze remove -m <id>`; a digest mismatch is `NeedsHuman` with `uze update <id>`. No trust authority is consulted.
- rejected: telling offline from a vanished local marketplace (Git answers both the same way).
- blast:    `repair.rs`, `marketplace.rs` (`materialize_from_link` visibility).

## R12 — How a repair decides each outcome

- status:   pending
- task:     2.1, 2.5
- commit:   a2b219ff
- trigger:  "the observation after is the report" leaves open how a finding before is matched to one after.
- decided:  findings are matched by subject, what they are about, harness, location and owned path. Gone after is `Repaired`; still there takes what its step said, else `NeedsHuman` (operator lane) or `Unrepairable`. A receipt that is `Blocked` only because its package's bytes are missing is in the repair lane and takes the bytes' outcome. Before observing again the repair forgets the vendor CLIs' memoised answers (`uze_integrations::forget_vendor_answers`), since it wrote what they read itself. Pins are restored before the mutation lock is taken, as `maintain` does; every other step runs under one lock.
- rejected: reporting only what the steps did (an edited build would vanish from the report).
- blast:    `repair.rs`, `shared/process.rs`.

## R13 — Which launchers are expected, and what "this binary" is

- status:   pending
- task:     2.4
- commit:   a2b219ff
- trigger:  shims exist only after `uze setup <harness>`; nothing records that one existed.
- decided:  a launcher is expected for a detected harness that supports runtime integration and is set up (`status` not `NotConfigured`). It must exist and start `current_exe` (`uze_platform::executable::is_launcher_of`: the link target on Unix, a same-length same-mtime copy on Windows). Repair calls `ensure_runtime_shim`. Both findings are warnings.
- rejected: expecting a launcher for every detected harness (one never set up has none by design).
- blast:    `catalogue.rs`, `repair.rs`, `uze-platform/executable.rs`.

## R14 — The endpoint is the runtime's to judge and clear

- status:   pending
- task:     2.4
- commit:   c7aca5a6, 2acd6402
- trigger:  `uze-application` must not depend on `uze-terminal`.
- decided:  `uze_terminal::stale_endpoint` (something at the endpoint, nothing answers, no server claims the workspace) and `clear_stale_endpoint` (under the exclusive workspace lock; a held lock leaves it alone). `uze-application` declares `TerminalEndpoint`, which `src/cli/repair.rs` implements; plain `doctor` adds the finding in `src/main.rs`.
- rejected: deleting the socket file from the application (would race a starting server).
- blast:    `uze-terminal/runtime/endpoint.rs`, `src/cli/repair.rs`, `src/main.rs`.

## R15 — `collect(&UzeHome)`, a registry per application, and one process-wide registration

- status:   pending
- task:     3.2
- commit:   31c08c22
- trigger:  design D7 gives `collect(&UzeHome)`, but two owners' rules read what the application holds (the installed extensions, the integrations), and the binary's collector cannot be named by `uze-application`.
- decided:  `Collectable { name, roots, collect(&UzeHome) -> Collected }`. `UzeApplication::collectors()` builds the application's own collectors per call, borrowing what they need (`CompiledViews(&app)`, `IntegrationProjections(&integrations)`), then appends those registered through `register_collector`, a process-wide list the binary fills once in `main` with its log journals (`telemetry::Journals`). `Health::collect` runs each under `catch_unwind`. `every_disposable_root_has_one_collector` lives in `tests/architecture/collection.rs`, where the binary's registration is in the set it checks.
- rejected: `collect(&UzeApplication)` (the binary's collector and the uze-workspace rule need no application); a registry stored in `UzeApplication` (every application a process builds — the TUI makes several — would need the binary to register again).
- blast:    `collection.rs`, `src/telemetry.rs`, `src/main.rs`, `tests/architecture/collection.rs`.

## R16 — Collectors run before the mutation lock; maintenance runs all of them

- status:   pending
- task:     3.3
- commit:   31c08c22
- trigger:  each collector takes its own lock, and `MutationLock` is a non-reentrant `flock`: a collector that takes it inside `maintain_locked` would wait on its own process.
- decided:  `maintain()` and `repair()` run `Health::collect` after restoring pins and before taking the mutation lock. The compiled-views collector takes the mutation lock itself and reports `skipped` while another process holds it, as maintenance skipped it before. The projections collector therefore runs on every `doctor` and health pass too, not only the client's occupancy pass (its inline trigger, `prune_runtime_projections`, now calls the collector).
- rejected: running collectors inside the lock with a "lock held" token (each owner's own lock is the design's point).
- blast:    `maintenance.rs`, `repair.rs`.

## R17 — Codex role files through `IntegrationPort::collect_runtime`

- status:   pending
- task:     3.3
- commit:   6726a965, c958e63e
- trigger:  the role-file sweep ran only inside a Codex launch, with the launch's own list of what it wrote; `uze-application` names no harness.
- decided:  a default-empty `IntegrationPort::collect_runtime(&UzeHome)`; Codex answers it by sweeping, for each projected project whose root is still there, the `.toml` role files no current project agent maps to (the same file names a launch gives, `role_file_names`). A project with no agents any more loses its role files too, which a launch never swept. One application collector, "harness projections", owns `runtime_projection_dir` and asks every integration.
- rejected: a Codex-named collector in `uze-application` (layering); declaring a Codex-only owned path in `UzeHome` (`uze-core` names no vendor).
- blast:    `integration.rs`, `codex.rs`, `codex/runtime.rs`, `maintenance.rs`.

## R18 — A conversation's document lives while a record names its conversation

- status:   pending
- task:     3.3
- commit:   fad80101
- trigger:  only `conversation::forget` removed a session document, so one a killed harness or a replaced conversation left stayed forever; "the session ended" needs a rule a collector can apply.
- decided:  `conversation::collect_session_documents` keeps every `runtime/sessions/<integration>/<session>.md` some agent's conversation record names, and removes the rest. A launch records the conversation it assigned before it writes the document, so a starting launch is never raced. Nothing is removed while any conversation record cannot be read, or while the layout before project records were gathered is still present.
- rejected: an age rule (an OpenCode session can run for days); collecting only documents of projects whose root is gone (misses the replaced conversation).
- blast:    `uze-workspace/src/conversation.rs`, `maintenance.rs`.

## R19 — Pictures keep their count bound at rest; a root builder added

- status:   pending
- task:     3.3
- commit:   31c08c22
- trigger:  `keep_pictures` bounds what one run keeps (16) but nothing bounded an extension's pictures across runs, and the collector needed the parent of every extension's pictures.
- decided:  the pictures collector keeps the newest `KEPT_PICTURES` pictures per extension and removes the rest; `UzeHome::extension_pictures_root` names their parent (listed, generated, with the derived check).
- rejected: a no-op collector (it would own a root and keep no rule).
- blast:    `program.rs`, `home.rs`, `owned.rs`, `catalogue.rs`.

## R20 — The retired list takes no lock of its own

- status:   pending
- task:     3.3
- commit:   31c08c22
- trigger:  the brief says the retired list runs under the mutation lock; the bootstrap never took it, and `a_machine_sheds_the_profiles_record_and_nothing_else_under_state` asserts the retirement leaves nothing else under `state/` (taking the lock writes `mutation.lock`).
- decided:  `Retired` (in `setup.rs`) removes what of `RETIRED` is present with no lock on the bootstrap trigger and in maintenance; `uze setup` removes it under the lock it already holds. Only retired paths marked disposable are on the list: `pre_sources_layout` and `scattered_project_records` are carried across by their own steps and never removed by it.
- rejected: taking the lock only when something retired is present (still fails the test above, and a retired path has no reader to race).
- blast:    `setup.rs`, `owned.rs`.

## R21 — Scratch by pid falls back to age where the platform cannot answer

- status:   pending
- task:     3.4
- commit:   31c08c22
- trigger:  `uze_platform::process::alive` answers `None` where the platform will not say, and a run directory not named `{pid}-{run}` has no pid.
- decided:  a scratch directory is removed when its pid is not alive; kept while it is; and, with no answer, judged by the old one-hour age. The pid is the UZE process that made the run (the run name's first part). The age-only sweep stays as the test helper `sweep_stale_scratch`, so its test is unchanged.
- rejected: keeping a directory forever when liveness is unknown.
- blast:    `program.rs`.

## R22 — How the home is compared with its schema

- status:   pending
- task:     3.5
- commit:   bf3abad7
- trigger:  D8's table leaves open how a parameterised path matches, where the walk stops, and how the result is reported.
- decided:  `OwnedPath::pattern/matches/lies_under` treat each `{…}` (and the placeholder package's segments) as any text within one segment. The walk skips the generated and remembered tiers and the opaque terminal subtree; a declared directory is entered only when a declared path lies under it (a package's bytes, a build, a memory directory are their owner's content). Set-aside names are found anywhere walked. An undeclared directory is entered when a declared path lies under it; otherwise an empty one in bytes, store or record is removed, and anything else is `unknown`. A retired path that a step still carries is passed over. Results are `RepairReport.home` (`SchemaEntry`: path, tier, finding, size for leftovers, removed), shown under "Home"; they do not change the exit status.
- rejected: reporting each unknown as a `HealthFinding` (it would make the exit status 1 for a newer build's file).
- blast:    `schema.rs`, `owned.rs`, `repair.rs`, `src/cli/repair.rs`, `src/cli/grammar.rs`.

## R23 — The size figure

- status:   pending
- task:     3.6
- commit:   bf3abad7
- trigger:  D8 asks plain `doctor` to show a recorded figure and `--repair` to refresh it.
- decided:  `CollectionRecord { measured_at_unix, tiers }` in `cache/collection.json`, measured per tier by each top-level entry's tier (`Tier::of`, read-only, terminal state counted as a record). `--repair` writes it after its second observation and reports it as `size`; plain `doctor` reads it into `DoctorReport.collection` and shows a `size` row only when one was recorded. Plain `doctor`'s collectors read only what each owner's rule needs (markers, compiled headers, mirror `catalogue.json`, directory listings); `doctor_meets_the_budget` stays green.
- rejected: measuring on plain `doctor` (a walk of every package's bytes).
- blast:    `collection.rs`, `doctor.rs`, `read_models.rs`, `src/cli/status.rs`.

## R24 — Collector moves committed by owner crate

- status:   pending
- task:     3.3
- commit:   fad80101, 3f4ce4a1, c958e63e
- trigger:  the brief asks for one commit per mechanism; the application's registration list and its coverage test only hold once every owner is in, and several owners share one file (`maintenance.rs`, `program.rs`).
- decided:  one commit per crate that owns a rule outside the application (workspace, integrations), then one for the application's registry and owners. Each mechanism keeps its rule and its existing tests unchanged; the only test-visible behaviour change is scratch by pid (R21), with a test of its own.
- rejected: hand-built intermediate commits that never existed as a working tree.
- blast:    history only.

## R25 — The "running session" scenario leaves the spec

- status:   pending
- task:     4.2
- commit:   bf3abad7
- trigger:  R2 left the spec's "A running session" scenario, the requirement's "unless a session … is running" and design D4 promising a guard nothing implements.
- decided:  the requirement now says repair repoints through the harness's own CLI and does not wait for or look for a session; the scenario is removed; design D4, its risk line, the proposal and task 2.3 say the same; the "repoints during a session" negative test is dropped from 4.2.
- rejected: keeping the scenario as a declared limitation (it would be a requirement nothing meets).
- blast:    `specs/machine-repair/spec.md`, `design.md`, `proposal.md`, `tasks.md`.

## R26 — The Lab's repair contract, and the bug it found

- status:   pending
- task:     4.3
- commit:   410cd9b5, feec1e1a, 5f068a92
- trigger:  4.3 asks for the repair proven on every vertical; the vendor-stores contract module the brief points to (`conformance/contract/stores.py`) is on `feat/clean-uninstall` and not in this branch.
- decided:  `conformance/contract/repair.py`, written in the same shape and naming no vendor: after `uze add flow@uze-lab`, `sources/` is deleted, the binding's `misplace_registration` runs (Codex: `[marketplaces.uze-store] source` pointed at `runtime/attachments/codex`, holding a copy of the store, as an earlier release left it), and a `uze terminal serve` is killed with `SIGKILL` so its endpoint stays. Doctor's JSON before must name `package_bytes`, `terminal_endpoint` (and `registrations` where misplaced); `uze doctor --repair` must exit 0; doctor after names none of them; every digest in `packages.json` is unchanged; the turn offers `flow:commit` and carries the `always_on` rule; the deny scene denies; where the binding has a `plugin_listing`, the vendor lists the plugin installed and enabled. `Bindings` gains `misplace_registration`, `plugin_listing`, `plugin_enabled` and `json_documents` (the same names the vendor-stores branch adds). The Codex run found that repair looked a mirror up by the registered name, failing for a plugin a marketplace lists under another (`mcp-plugin` vs `uze-mcp-conformance`): fixed in 5f068a92 with `a_plugin_listed_under_another_name_comes_back`.
- result:   `python3 conformance/lab.py --harness <h> --contract repair`: claude 20/20, codex 20/20 (after the fix), opencode 21/21, antigravity 21/21 (these three on the image before the fix, which only changes the entry lookup).
- rejected: cherry-picking the vendor-stores conformance commits into this branch (they are delivered with that change).
- blast:    `conformance/contract/{repair,bindings,__init__}.py`, `conformance/harnesses/codex/bindings.py`, `repair.rs`.

## R27 — The gate's findings on code an earlier group wrote

- status:   pending
- task:     4.4
- commit:   31c08c22, (the commit recording this)
- trigger:  `make check` failed on two things group 2 left: clippy 1.99's `type_complexity` on a test array in `catalogue.rs`, and `vendor_neutral` scans refusing `PackageSource`/`ResolvedSource` in `store.rs` (a test helper of `restore_bytes`).
- decided:  a type alias in the catalogue test; the store test builds its provenance through `Provenance::of_local_path`, a `#[cfg(test)]` constructor in `acquisition.rs`, so `store.rs` names no source mechanism.
- result:   `make check` (with `UZE_AGENT`, `UZE_AGENT_KEY`, `UZE_PANE` unset): fmt, lint, deny, telemetry tests, python fmt/lint and artifacts pass; `test` fails only on the known environment failures — `command_approval::approving_needs_a_person_at_a_terminal`, `extensions::an_install_of_what_the_project_declares_says_the_same`, `extensions::an_install_off_a_terminal_leaves_a_script_extension_disabled_and_says_where_to_enable_it`, `uze-terminal` `foreground_status_*` (2) — plus one `uze-theme` test (`the_bundled_tables_and_the_themes_directory_agree`) that did not find its themes directory once and passed on rerun. `openspec validate --all --strict`: 101 passed.
- blast:    `catalogue.rs`, `store.rs`, `acquisition.rs`.

## R28 — Restoring a pin is not changing a revision

- status:   pending
- task:     review
- commit:   (the commit recording this)
- trigger:  the review found `repair.rs` running `restore_pins` while the spec said repair SHALL NOT change a revision.
- decided:  the requirement says repair SHALL NOT change a *pinned* revision, and that restoring the commit pinned before a marketplace link is not a change: `agents.lock` never moves, and the restore puts back the revision it records.
- rejected: dropping the restore from repair (a machine left on a link's working tree would need `uze update`, which moves pins).
- blast:    `specs/machine-repair/spec.md`.
