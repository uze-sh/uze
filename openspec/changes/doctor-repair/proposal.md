## Why

`uze doctor` already heals the narrowest kind of damage without asking.
Bounded maintenance (ADR-037, `add-environment-maintenance-engine`)
recreates any receipt-proven `Missing` artifact it can rebuild locally.
After `vendor-stores-and-uninstall`, that includes a deleted vendor-store
build. Damage that needs more than a local rebuild is only reported, and
the fix is left to the person:

- package bytes deleted from `sources/`, which `vendor-stores-and-uninstall`
  keeps delivering as "bytes missing" but can no longer rebuild;
- a vendor marketplace registered at a root other than its vendor store;
- a stale terminal-runtime endpoint;
- missing shims.

`vendor-stores-and-uninstall` makes `uze uninstall` the supported way out
and declares removing parts of `~/.uze` by hand unsupported. That stance
needs a supported way back: one deterministic command that knows the layout
of `~/.uze` and puts back what can be put back. It cannot depend on an
agent, a harness or the workspace, since any of those may be the broken
part.

## What Changes

- **`uze doctor --repair`**: an explicit, deterministic repair of UZE's own
  machine state.
  - It plans from live inspection, executes under the mutation lock, then
    observes again.
  - It reports each finding as `Repaired`, `NeedsHuman` (with the reason and
    the command), `NeedsNetwork` or `Unrepairable`.
  - It is idempotent, with exit status 0, 1 or 2.
  - It runs no bootstrap.
- **One health catalogue, declared once.** Every path in `UzeHome`, every
  receipt kind, every vendor store and every registration UZE asks a vendor
  for has a check. Each check names its repair lane:
  - **automatic**: ADR-037's lane, unchanged;
  - **`--repair`**;
  - **none**, with the reason.

  A coverage test fails when an owned path has no check.
- **What `--repair` adds over the automatic lane:**
  - re-acquire missing package bytes from the registry's provenance,
    verified against the cached digest;
  - repoint a marketplace whose registered root differs from its vendor
    store, never while that harness runs;
  - clear a stale terminal-runtime endpoint;
  - recreate shims.
- **What it never does**, and reports instead with the command that
  resolves it:
  - overwrite an artifact the operator edited;
  - change a revision;
  - grant trust;
  - set aside or rewrite an irreplaceable record (the receipt ledger,
    authorizations);
  - touch a project or unlanded work.
- **Doctor never aborts on one bad record.** Each check isolates its own
  failure. `doctor` already does this for the ledger; it becomes a
  requirement, tested with every record corrupted in turn.
- **Findings name their lane.** Wherever `doctor` or the TUI shows a finding
  that only `--repair` fixes, the action it shows is `uze doctor --repair`.

## Capabilities

### New Capabilities
- `machine-repair`: the explicit repair of UZE's own machine state. It covers
  the health catalogue and its coverage, the lanes, what repair may and may
  not do, and how it reports.

### Modified Capabilities
<!-- none: `doctor`'s caching and requirement reporting and
`environment-maintenance`'s automatic lane are unchanged; this change adds
an explicit lane beside them. -->

## Impact

- `crates/uze-core`: `UzeHome` becomes enumerable (each owned path with
  its tier), so the catalogue and the coverage test read one list.
- `crates/uze-application`:
  - the health catalogue, shared by `doctor`, `doctor --repair` and the TUI;
  - the repair service (plan, execute, observe again);
  - re-acquisition from provenance.
- `src/main.rs` and `src/cli/`: `doctor --repair` skips bootstrap, renders
  the report and is classified `JustifiedSlow`. Plain `doctor` stays
  Budgeted, and its budget test covers the catalogue.
- Tests:
  - `tests/acceptance/repair.rs`, the damage matrix;
  - `every_owned_path_has_a_health_check`;
  - `doctor` with each record corrupted.
- Conformance: on real harnesses, delete `sources/` and corrupt a
  marketplace root, run `doctor --repair`, then check that plugins load.
- Depends on `vendor-stores-and-uninstall` for the store tier, the cached
  digest and "bytes missing". It extends `add-environment-maintenance-engine`
  and does not change its lane.
