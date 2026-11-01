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

The bytes are kept only when their digest equals the cached one.

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
and the registry, and never spawns a vendor CLI just to check a
registration; it reads the vendor's registration file through the
integration's existing read-only parser. `doctor_meets_the_budget` covers
the catalogue. `--repair` is `JustifiedSlow`.

## Risks / Trade-offs

- **[A repoint is visible in the harness]** → it is explicit, never runs
  during a session, and is reported.
- **[An unreadable ledger leaves the machine stuck until the person acts]**
  → this is intended (persisted-state). The report names the file and the
  error, and says that no delivery is touched meanwhile.
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

## Candidate ADRs

- **Every owned thing has a health check with a declared repair lane.** This
  extends ADR-037 (an explicit lane beside the automatic one) and ADR-052
  (`UzeHome` as an enumerable list with tiers).
