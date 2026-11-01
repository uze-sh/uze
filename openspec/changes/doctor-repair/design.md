## Context

See proposal.md. Revision 2 folds in two independent reviews of revision 1.
The "Decision log" lists every choice made without the operator.

The current state:

- **Plain `doctor` runs bounded maintenance** (ADR-037,
  `add-environment-maintenance-engine`, mostly in
  `crates/uze-application/src/application/maintenance.rs`). It repairs a
  receipt that inspects `Missing` when its integration can rebuild it
  locally. After `vendor-stores-and-uninstall`, that covers a deleted
  vendor-store build.
- **`doctor` already isolates an unreadable ledger.** `doctor.rs` keeps
  `ledger_error` and goes on. The package registry quarantines corrupt
  entries (`store.rs`).
- **`UzeApplication::from_env` reads no state.** It fails only without a
  home directory. Nothing in today's layout stops every command from
  starting.
- **`MutationLock` is a kernel `flock`** on a file that is never unlinked.
  It cannot go stale.
- **Persisted-state (ADR-052):** irreplaceable state (the ledger,
  authorizations) blocks and names its remedy, and is never set aside.
  `UzeHome` names every owned path.

## Goals / Non-Goals

**Goals:**
- One command with no agent involved: it is deterministic, works from the
  CLI, and never needs the workspace.
- Coverage by construction, so that a new owned thing cannot ship without a
  check.

**Non-Goals:**
- Changing the automatic lane, its triggers or its budget.
- Repairing harnesses themselves (vendor binaries, vendor files UZE never
  wrote) or projects.
- A TUI repair button. The TUI names the command.
- Recovering from an unreadable receipt ledger automatically. That record
  is irreplaceable by persisted-state's own rule.

## Decisions

### D1. A flag on `doctor`

| option | why not |
|---|---|
| a `uze:doctor` skill | needs a working harness, a model and an agent; it is non-deterministic |
| a workspace extension | needs the workspace, which is among the things that break |
| a `uze repair` verb | `doctor` already owns machine health; two verbs would give two reports of the same findings |

`--repair` turns the diagnosis into action. The flag is the consent, so
there is no extra prompt. The plan is printed as it executes, and
`--format json` is available for automation.

### D2. The health catalogue and its lanes

`UzeHome` becomes an enumerable list, where each owned path carries its
tier. `uze-application` declares `HealthCheck`s once. Each has a subject,
an `observe` that yields findings, and a lane:

| subject | observe | lane |
|---|---|---|
| receipts (all kinds) | existing `IntegrationPort` inspection, through the inspection cache on plain `doctor` | `Missing` → automatic (ADR-037); `Drifted`/`Conflict` → none (operator) |
| vendor-store builds | build receipt (vendor-stores D3) | `Missing` → automatic; `Drifted` → none |
| package bytes (`sources/`) | registry entry with no directory ("bytes missing") | `--repair` (network, digest-verified) |
| marketplace registration | registered root against its vendor store | `--repair` (repoint) |
| terminal-runtime endpoint | `uze-terminal`'s own probe, run from `src/` | `--repair`, through `uze-terminal`'s clear under its workspace lock |
| shims | each expected link present and pointing at this binary | `--repair` |
| records (`state/`) | each record's load through its ladder | rebuildable classes → automatic (existing floor); ledger and authorizations → none |
| generated and remembered | — | automatic (produced or observed again) |

`every_owned_path_has_a_health_check` sits beside
`every_path_uze_owns_is_named_in_the_map`, and both read the same list.

### D3. Re-acquisition is a restore, not an update

The registry records each package's provenance, and
`vendor-stores-and-uninstall` caches the digest taken at ingest.
Re-acquisition per source:

| source | how |
|---|---|
| Git marketplace | fetch at the recorded commit |
| local path | read the path again |
| linked marketplace | read the link target again |
| embedded (the bundled `uze` plugin) | the snapshot inside the binary, offline |

The bytes are kept only when their digest equals the cached one. The one
exception is an embedded package. Its snapshot *is* this binary's own
version of it, which after an upgrade legitimately differs from the cached
digest. It is ingested again, recording the new digest, as bootstrap would
do.

- **A mismatch** → `NeedsHuman` with `uze update <id>`. The builds keep
  delivering what they have.
- **No network** → `NeedsNetwork`.

Authorization is never re-asked. Bytes that match the digest are the bytes
that were authorized.

### D4. Repoint carefully

A repoint happens only when the registered root differs from the vendor
store, and never while a session of that harness is running. Sessions are
found through the platform's process listing, filtered by the harness
executable.

- **Claude:** a same-name `marketplace add`.
- **Codex:** `remove`, then `add`. If the `add` fails, the old root is
  registered again.

These are the same rules as the vendor-stores migration (its D5) and share
its code.

### D5. Order, idempotency, report

The plan is computed from live inspection, never from a cached `Matched`.
It executes in dependency order:

1. bytes;
2. builds, through the automatic lane, which now finds bytes to build from;
3. registrations;
4. the endpoint;
5. shims.

Each step re-checks its precondition. Afterwards the catalogue observes
again, and that observation is the report.

**Exit status:**
- `0` when healthy;
- `1` when any finding is `NeedsHuman` or `Unrepairable`;
- `2` when some finding is `NeedsNetwork` and none needs a human.

### D6. Budget

Plain `doctor` stays `Budgeted`. It observes through the inspection cache
and the registry, and never spawns a vendor CLI just to check a registration.
It reads each vendor's registration file read-only instead:

- **Claude:** `known_marketplaces.json`, through the existing
  `recorded_marketplaces`.
- **Codex:** `[marketplaces.*]` in `config.toml`. This is a new read
  alongside the existing `[plugins.*]` read in `codex/inventory.rs`, which
  today spawns `codex plugin marketplace list` instead.

`doctor_meets_the_budget` covers the catalogue. `--repair` is
`JustifiedSlow`.

### D7. One collector: owners decide, one executor runs

There are fourteen collection mechanisms today, each with its own rule,
trigger, lock and log:

| mechanism | rule | trigger, lock |
|---|---|---|
| `harness_runtime::prune_projections` | any `runtime/` child that is not a tenant, and any project without a readable marker; deliberately not by age | the client's occupancy pass (`Health::prune_runtime_projections`); no lock |
| `rules_engine::sweep_rule_sessions` | age (7 days) | `setup` |
| `extensions::sweep_stale_scratch` | age (1 h); run dirs are `{pid}-…` | the first program run per process |
| `extensions::keep_pictures` | count bound | the program run |
| `prune_compiled_cache` | the newest file per installed identity, plus unparseable files | maintenance, `MutationLock` |
| `marketplace_catalogue::prune_materialized` | the current commit OR touched within `MAX_AGE` | a fetch, `MirrorHold` |
| `codex/runtime.rs` role-file sweep | the project's live agents | a Codex launch |
| `conversation::forget` (`runtime/sessions/<integration>`) | the session ended; a killed harness leaks it | `uze-workspace` |
| `telemetry::prune_to_size` (`logs/`) | size budget | `src/` |
| `retire_records_of_earlier_releases` | a hard-coded list | every `setup` and bootstrap, `MutationLock` |
| `store/superseded/` | nothing collects it | — |
| `delivery/leftovers.rs` | report only | `doctor` |

Out of scope, on purpose:
- **Process lifecycle** (idle MCP servers, the terminal's `retire`): a
  process is alive or not, which is not a retention.
- **Project Git** (`prune_integrated_branches`): outside `$UZE_HOME`.
- **Registry hygiene** (`prune_ghost_registrations`): it decides ownership,
  not disposal, and stays in maintenance under vendor-stores' "bytes
  missing" rule.

**The shape.** One policy enum on `UzeHome` was the first draft. A reviewer
showed it is too small for the rules the owners actually use: "current OR
recently touched", "newest per identity", size and count budgets, process
liveness. The shape adopted instead:

1. **`UzeHome` marks disposable roots.** Each owned path carries its tier,
   and the generated and remembered paths that may be swept are flagged
   *disposable*. `uze-core` holds no policy and names no owner.
2. **Each owner implements `Collectable`** in the crate that knows its
   rule:
   - `roots()` names the disposable roots it owns;
   - `collect(&UzeHome) -> Collected` decides and deletes under the
     owner's **own** lock (`MirrorHold`, `MutationLock`, none for
     projections), with today's rule kept exactly.

   Owners in `uze-application` cover extensions, mirrors, rule sessions and
   the retired list. Through `uze-application`, `uze-core` covers
   projections, `uze-workspace` covers conversation sessions and
   `uze-integrations` covers Codex role files. The binary (`src/`)
   registers telemetry logs.
3. **One registry runs them.** `uze-application` holds the registry. The
   maintenance pass that `doctor` and the TUI already trigger runs every
   collector, and owners keep their inline triggers by calling their own
   `collect`. Every owner reports what it freed to one report.
4. **Coverage.** `every_disposable_root_has_one_collector` requires exactly
   one registered collector per disposable root. A disposable path added
   with no collector fails the build. That gap is how the dispersion
   started.

**What changes for a collector:** only how it is called and how it reports.
Its rule and its tests stay the same. The only behaviour that changes on
purpose is extension scratch, which becomes **alive while its pid is
alive** (through `uze-platform`'s process probe) instead of "younger than
1 h". It now runs on every maintenance pass, not just the first program
run, and an age rule there would delete the scratch of a long-running
program.

**What is never collected:**
- **The terminal runtime's paths** (`state/terminal/…`: workspace record,
  lock, operator key, endpoint, socket). That subtree is the one sanctioned
  exception to `UzeHome`, and it holds live state: a never-unlinked
  `flock`, the TERM-1 operator key, and the socket `spawn_endpoint_watch`
  guards because a cleaner once removed it under a live server. The
  subtree is declared opaque. Neither the collector nor the schema
  comparison descends into it, except for the existing leftovers report.
- **Unknown entries in the bytes, store, record and authored tiers.** In the
  generated and remembered tiers, unknown entries remain the owner's to
  sweep. Shipped releases already sweep unknown `runtime/` children, so no
  future build may add a top-level `runtime/` entry and expect an older
  build to leave it alone.

**Retired paths keep their trigger.** `collect(retired)` runs on every
`setup` and bootstrap under the mutation lock, as today. A path goes on the
retired list only once deleting it costs every supported build nothing:

- a generated or remembered path; or
- a record no supported build still reads, in the same way a ladder step
  becomes deletable once no machine is below it.

A retired name is never reused.

**`sources/superseded/`** (a failed restore's only copy) is reported as a
leftover, with its size. It leaves only with `--discard-leftovers`.

### D8. The schema comparison and the budget

`--repair` walks `$UZE_HOME` against the enumerable list, skipping the
opaque terminal subtree:

| found | action |
|---|---|
| a declared path | its health check, and its collector if it is disposable |
| a retired path still present | removed (normally bootstrap already did it) |
| an empty directory in the bytes, store or record tiers, not declared | removed |
| a set-aside record or `sources/superseded/` | reported with its size; removed only with `--discard-leftovers` |
| unknown, in the bytes, store, record or authored tiers | reported as unknown, possibly from a newer build; never deleted |

**Budget.**
- **Plain `doctor`.** It runs the collectors with *bounded reads*: the
  projection markers, the compiled views' headers and each mirror's
  `meta.json`. These are named per owner and covered by
  `doctor_meets_the_budget`. Sizes are never walked on plain `doctor`. It
  shows the last recorded figure, kept in a declared remembered file
  (`cache/collection.json`).
- **`--repair`.** It measures the size per tier and refreshes that file.

The release-upgrade proof (the nightly `journeys/07-upgrade` suite) only
catches retirements that the previous release's scenarios exercise. A
forgotten retirement costs disk, not data, because unknown entries are only
reported.

## Risks / Trade-offs

- **[A repoint is visible in the harness]** → it is explicit, never runs
  during a session, and is reported.
- **[An unreadable ledger leaves the machine stuck until the person acts]**
  → this is intended (persisted-state). The report names the file and the
  error, and says that no delivery is touched meanwhile.
- **[A collector moved wrong collects live data]** → each one moves in its
  own step, keeps its rule and its lock, and keeps its current tests green.
  The single deliberate change (scratch by pid) gets a test of its own with
  a long-running program.
- **[The retired list is forgotten when a release drops a path]** → the
  upgrade suite catches what N-1's scenarios exercise. The rest costs disk,
  not data, because unknown entries are only reported.
- **[The catalogue grows with every feature]** → the coverage test makes that
  growth explicit, which is the point.

## Decision log (revision 2, taken without the operator)

| # | chosen | rejected, and why |
|---|---|---|
| 1 | No "recover before the application" stage | Its premise was false: `from_env` reads no state, and `doctor` already survives an unreadable ledger. |
| 2 | No lock clearing | The lock is a kernel `flock` that cannot go stale; clearing it would allow two holders. |
| 3 | The ledger and authorizations are never set aside; report and stop | Setting them aside breaks persisted-state's irreplaceable-state rule. |
| 4 | Missing builds stay in the automatic lane; `--repair` owns bytes, repoints, endpoint and shims | Making store rebuilds `--repair`-only contradicts ADR-037's lane, which already rebuilds a `Missing` receipt. |
| 5 | Re-acquire from registry provenance, kept only on a digest match | "The revision the receipt pins" does not exist; receipts carry no revision. |
| 6 | No "re-adopt" command | None exists, and inventing one is out of scope. Findings point to existing commands. |
| 7 | A flag on `doctor`, not a skill, an extension or a new verb | See D1. |
| 8 | Embedded packages are re-ingested from the binary's snapshot, with no digest match required | A digest match would turn every post-upgrade restore of the bundled plugin into `NeedsHuman`. |
| 9 | A read-only `config.toml` reader for Codex marketplaces | Spawning `codex plugin marketplace list` on plain `doctor` breaks its budget. |
| 10 | A `Collectable` per owner, one registry, and coverage of every disposable root. `uze-core` only marks roots disposable. | A `Retention` enum on `UzeHome` cannot express "current OR recent", "newest per identity", budgets or pid liveness. A central collector that judges liveness itself would need every domain's knowledge. |
| 15 | The terminal runtime's subtree is opaque and never collected; endpoint repair goes through `uze-terminal`'s own probe, called from `src/` | Collecting it by looking at the disk would delete a live socket, lock and operator key. `uze-application` does not depend on `uze-terminal`. |
| 16 | "Never delete the unknown" applies to the bytes, store, record and authored tiers only | Applying it to generated and remembered would break the projection sweep, which shipped releases already rely on. |
| 17 | Extension scratch lives while its pid lives | A 1 h age would delete a long-running program's scratch once the collector runs on every maintenance pass. |
| 18 | Retired paths stay on the bootstrap trigger; only paths no supported build reads may be retired | Moving retirement to `--repair` alone would stop it from happening. Retiring a record an older build still reads breaks two builds on one machine. |
| 11 | Process lifecycle, project Git and registry hygiene stay outside the collector | A process is not a path with a retention, branches live outside `$UZE_HOME`, and ghost pruning decides ownership, not disposal. |
| 12 | Unknown entries are reported, never deleted | Two builds on one machine: a newer build's state is unknown to an older one. |
| 13 | Set-aside records leave only with `--discard-leftovers` | Persisted-state keeps them because they may be the operator's lost work. |
| 14 | Owners keep their inline triggers through `collect(path)` | Moving every trigger to maintenance would delay collection that happens today exactly when it matters. |

## Candidate ADRs

- **Every owned thing has a health check with a declared repair lane.** This
  extends ADR-037 (an explicit lane beside the automatic one) and ADR-052
  (`UzeHome` as an enumerable list with tiers).
