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
| vendor-store builds | build manifest (vendor-stores D3) | `Missing` → automatic; `Drifted` → none |
| package bytes (`sources/`) | registry entry with no directory ("bytes missing") | `--repair` (network, digest-verified) |
| marketplace registration | registered root against its vendor store | `--repair` (repoint) |
| terminal-runtime endpoint | endpoint present, no server answering | `--repair` |
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

### D7. One collector, declared policy, owner-held liveness

Today ten mechanisms collect garbage. Each has its own rule, trigger and log:

| mechanism | rule | trigger |
|---|---|---|
| `harness_runtime::prune_projections` | reachability (a tenant uses it), deliberately not age | the TUI's reads |
| `rules_engine::sweep_rule_sessions` | age | `setup` |
| `extensions::sweep_stale_scratch` | age (1 h) | the first program run in a process |
| `prune_compiled_cache` | reachability (installed extension versions) | maintenance |
| `marketplace_catalogue::prune_materialized` | the current commit plus age | a marketplace fetch |
| `retire_records_of_earlier_releases` | a hard-coded list | `setup`, bootstrap |
| `Store::prune_ghost_registrations` and orphans | registry hygiene | maintenance |
| `delivery/leftovers.rs` | report only | `doctor` |
| MCP idle sweep, terminal `retire` | process lifecycle | timers, the runtime |
| `checkout::prune_integrated_branches` | project Git | the workspace |

The design separates three things that these mechanisms each mix
differently:

1. **Policy is data on `UzeHome`.** Each owned path carries its
   `Retention`: `OnDemand`, `Age(Duration)`, `Reachable(Owner)` or
   `Never`. `uze-core` holds it, next to the tier.
2. **Liveness stays with the owner.** A `Reachable` path names an owner that
   answers `alive(&UzeHome) -> set of paths`, with today's reasoning kept:
   - projections answer by tenant marker, never mtime;
   - compiled caches by installed extension identity;
   - mirrors by each marketplace's current commit.

   Owners live where the knowledge lives: `uze-core` for projections and
   `uze-application` for extensions and mirrors. `uze-application` also
   reaches `uze-workspace` paths through its existing dependency.
   `uze-terminal`, which depends on nothing here by design, is never
   called: its paths are declared with an `Age` or `OnDemand` retention and
   collected by looking at the disk, as `leftovers.rs` already does.
3. **One collector executes.** `uze-application`'s `collect(&UzeHome,
   scope)` deletes only inside declared disposable paths, and only what
   the owner did not report alive. Anything it does not recognise is
   reported and never deleted, because two builds on one machine is this
   repository's daily state, and a newer build's paths are unknown to an
   older one.

**Triggers.** The maintenance pass that `doctor` and the TUI already run
calls `collect` with the stat-only scope, which stays inside `doctor`'s
budget. Owners that collect at a natural moment today keep doing so by
calling `collect(path)` for their own path, so no behaviour is delayed:

- a marketplace fetch;
- the first program run of a process.

`--repair` adds the schema comparison (D8).

**Out of scope, on purpose:**
- **Process lifecycle** (idle MCP servers, the terminal's `retire`): a
  process is alive or not, which is not a retention.
- **Project Git** (`prune_integrated_branches`): it lives outside
  `$UZE_HOME`, in the workspace's domain.
- **Registry hygiene** (`prune_ghost_registrations`): it decides ownership,
  not disposal, and stays in maintenance under the "bytes missing" rule of
  `vendor-stores-and-uninstall`.

### D8. The schema comparison

`--repair` walks `$UZE_HOME` against the enumerable list and sorts each
entry it finds:

| found | action |
|---|---|
| a declared path | handled by its health check and retention |
| a path on the **retired list** (added by the release that stopped using it, replacing `retire_records_of_earlier_releases`) | removed and reported |
| an empty directory in the bytes, store or record tiers that is not a declared path | removed |
| a record the ladder set aside (`leftovers.rs`) | reported with its size; removed only with `--discard-leftovers` |
| anything else | reported as unknown, possibly from a newer build; never deleted |

`doctor` reports, per tier, the size on disk and what the last collection
freed.

The coverage test `every_owned_path_has_a_health_check` also requires a
`Retention` for each path. This is how the dispersion started: a new path
was added and the decision of who cleans it was left for later. The test
closes that gap.

## Risks / Trade-offs

- **[A repoint is visible in the harness]** → it is explicit, never runs
  during a session, and is reported.
- **[An unreadable ledger leaves the machine stuck until the person acts]**
  → this is intended (persisted-state). The report names the file and the
  error, and says that no delivery is touched meanwhile.
- **[A retention declared wrong collects live data]** → only `Reachable`
  and `Age` paths are ever collected, and every `Reachable` owner keeps its
  existing tests. Each mechanism moves behind the collector in its own step,
  with its current tests unchanged and green.
- **[The retired list is forgotten when a release drops a path]** → the
  release-upgrade proof (persisted-state: "A release proves its own upgrade
  against the previous one") runs the previous release, then this one. It
  fails on any path the previous release left that is neither declared nor
  retired.
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
| 10 | One collector: policy on `UzeHome`, liveness kept by owners, execution central | Keeping ten mechanisms leaves no single answer to "what does UZE keep, and why". A fully central collector that judges liveness itself would need every domain's knowledge and would re-introduce the age rule that projections reject. |
| 11 | Process lifecycle, project Git and registry hygiene stay outside the collector | A process is not a path with a retention, branches live outside `$UZE_HOME`, and ghost pruning decides ownership, not disposal. |
| 12 | Unknown entries are reported, never deleted | Two builds on one machine: a newer build's state is unknown to an older one. |
| 13 | Set-aside records leave only with `--discard-leftovers` | Persisted-state keeps them because they may be the operator's lost work. |
| 14 | Owners keep their inline triggers through `collect(path)` | Moving every trigger to maintenance would delay collection that happens today exactly when it matters. |

## Candidate ADRs

- **Every owned thing has a health check with a declared repair lane.** This
  extends ADR-037 (an explicit lane beside the automatic one) and ADR-052
  (`UzeHome` as an enumerable list with tiers).
