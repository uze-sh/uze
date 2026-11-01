## Context

See proposal.md. The current state:

- **Plain `doctor`.** It runs bounded maintenance from
  `add-environment-maintenance-engine`, which is mostly implemented in
  `crates/uze-application/src/application/maintenance.rs`. That maintenance
  repairs only a receipt that inspects as `Missing` and that its integration
  can rebuild, with no network involved. Its outcomes are `Repaired`,
  `OrphanCleaned` and the rest of its vocabulary.
- **The fragile start.** `src/main.rs` builds `UzeApplication::from_env`
  before dispatching any command, and ignores errors from
  `ensure_default_plugins`. If `from_env` fails, nothing runs.
- **Persisted-state's existing guarantees** (ADR-052), which this change
  builds on:
  - the ladder carries records across versions;
  - the floor sets aside a shape with no rung and reconstructs what the
    world still knows;
  - `UzeHome` names every owned path;
  - `every_path_uze_owns_is_named_in_the_map` enforces that.

## Goals / Non-Goals

**Goals:**
- One command, no agent: run deterministically, from the CLI, on a home that
  nothing else can start on.
- Coverage by construction, so that a new owned thing cannot ship without a
  check.

**Non-Goals:**
- Changing the automatic lane, its triggers or its budget.
- Repairing harnesses themselves (vendor binaries, vendor-owned files UZE
  never wrote) or projects.
- A TUI repair button. The TUI may be the broken part, so it names the
  command instead.

## Decisions

### D1. A flag on `doctor`, not a skill, an extension or a new verb

| option | why not |
|---|---|
| A `uze:doctor` skill | Needs a working harness, a model and an agent, and is non-deterministic. |
| A workspace extension | Needs the workspace, which is among the things that break. |
| A `uze repair` verb | `doctor` already owns machine health. Two verbs would mean two reports of the same findings. |

`--repair` turns the diagnosis into action. The flag is the consent, so there
is no extra prompt, and the plan is printed as it executes. `--format json`
serves automation.

### D2. The health catalogue

`uze-application` declares the catalogue once, as a list of `HealthCheck`s.
Each has a subject, a tier, an `observe` that yields findings, and an
optional `repair` with its preconditions (needs the network, needs a vendor
CLI). Its subjects are:

- every `UzeHome` path, with its tier read from a new tier tag on the map
  (`uze-core`);
- every receipt kind, observed through the existing `IntegrationPort`
  inspection;
- every vendor store and vendor registration (marketplace source, plugin
  enabled);
- process state: the mutation lock, the terminal-runtime endpoint, shims.

Plain `doctor`, `doctor --repair` and the TUI all read this catalogue.
Automatic maintenance stays the subset of repairs ADR-037 already allows
(local, deterministic, `Missing`, no network).

`every_owned_path_has_a_health_check` sits beside
`every_path_uze_owns_is_named_in_the_map`: the map and the catalogue must
cover the same set.

### D3. The tier is the repair strategy

| tier | repair | not done automatically, because |
|---|---|---|
| remembered (`cache/`) | observe again | — (automatic) |
| generated (`runtime/`, shims) | produce again | — (automatic) |
| store (`store/<vendor>`) | rebuild builds from `sources/`, re-register or repoint the marketplace, refresh the vendor cache | a repoint is visible in the harness |
| bytes (`sources/`) | acquire again at the receipt's pinned revision | needs the network |
| record (`state/`) | ladder; otherwise floor (set aside) plus a reconstruction report | never invented; the operator re-adopts |
| operator-edited (`Drifted`, `Conflict`) | none: reported with the command | the operator's choice |

Re-acquiring bytes is not an update. It is the exact revision the receipt
pins, and the authorization record must still be present. If that record is
gone, the finding is `NeedsHuman` with the `add` command.

### D4. Recovery before the application

`uze_application::recover_home(&UzeHome) -> RecoveryReport` takes no
`UzeApplication` and only touches what `uze-document` and `uze-platform`
offer. It does three things:

1. **Finish an interrupted layout move**, such as `store/` → `sources/`
   from `vendor-stores-and-uninstall`.
2. **Run every record's ladder and floor**, setting aside what cannot be
   read.
3. **Clear the mutation lock** when its recorded holder is not alive, using
   the platform's process-liveness probe.

`main.rs` runs it ahead of `from_env` for `doctor`, and `--repair` skips
`ensure_default_plugins`. The bundled plugin is re-seeded as an ordinary
store repair, in plan order. Other commands keep their current start: a
failing `from_env` now says `run uze doctor --repair`.

### D5. Order and idempotency

The plan is computed from live inspection, never from a cached `Matched`,
following `doctor`'s existing rule for destructive paths. It executes in
dependency order:

1. home recovery;
2. records;
3. sources;
4. vendor stores;
5. registrations;
6. generated;
7. process state.

Each step re-checks its precondition and writes through the existing
mutation lock. Once the plan has run, the catalogue observes again, and that
observation is the report.

### D6. Exit status

- **0:** healthy after the run.
- **1:** some finding is `NeedsHuman` or `Unrepairable`.
- **2:** some finding is `NeedsNetwork`, and nothing needs a human.

Scripts can tell "try again online" apart from "look at this".

## Risks / Trade-offs

- **[Repair rebuilds a vendor store while a harness session is open]** →
  Claude reads the store live, so the session sees the rebuilt plugin at its
  next start. Repair is explicit, and the report names running harness
  processes.
- **[Reconstruction is incomplete after a record is set aside]** → by design
  (ADR-052's floor). The report lists what the world still shows (vendor
  stores, registrations) and the commands to re-adopt it. Nothing is guessed.
- **[The catalogue grows with every feature]** → the coverage test makes
  that growth explicit, which is the point.

## Candidate ADRs

- **The tier of an owned thing is its repair strategy, and every owned
  thing has a health check.** This extends ADR-052 and ADR-037 and binds
  every future owned path.
