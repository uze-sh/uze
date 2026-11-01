## Why

`uze doctor` already heals the narrowest kind of damage. Bounded maintenance
(ADR-037, `add-environment-maintenance-engine`) recreates a receipt-proven
`Missing` artifact without asking. Everything else is reported and left to
the person:

- a deleted `sources/` or vendor store;
- a vendor marketplace pointing at a directory that is gone;
- a stale lock or terminal-runtime endpoint;
- an unreadable record.

Some damage is worse still. When `state/` cannot be read,
`UzeApplication::from_env` fails before any command runs, `doctor` included,
so the tool meant to diagnose the home cannot start.

`vendor-stores-and-uninstall` makes `uze uninstall` the supported way out
and declares removing parts of `~/.uze` by hand unsupported. That only holds
up if there is a supported way back. A person who deleted or corrupted
something, or an upgrade that went wrong, needs one deterministic command that
knows the layout of `~/.uze` and puts it back. That command cannot depend on
an agent, a harness or the workspace, since any of those may be the broken
part.

## What Changes

- **`uze doctor --repair`**: an explicit, deterministic repair of everything
  UZE owns on the machine.
  - It plans first and executes under the mutation lock.
  - It re-inspects afterwards and reports each finding as `Repaired`,
    `NeedsHuman` (with the reason and the command), `NeedsNetwork` or
    `Unrepairable`.
  - It is idempotent.
- **One health model, declared once.** Every path in `UzeHome`, every receipt
  kind, every vendor store and every registration that UZE asks a vendor
  for has a check. Each check carries a repair, or a stated reason why it
  has none. The repair follows from the tier:
  - **generated and remembered:** produced again;
  - **store:** rebuilt from `sources/` and re-registered;
  - **bytes:** re-acquired at the pinned revision;
  - **records:** carried by the ladder or set aside by its floor, never
    invented.

  A test fails when an owned path has no check.
- **Recovery before the application.** `doctor` (with or without `--repair`)
  first runs a home-level recovery that needs only `uze-document` and
  `uze-platform`:
  - finish the layout migration;
  - set aside a record that cannot be read;
  - clear a lock whose holder is dead.

  Only then does it build the application, so `doctor` starts on a home
  that `from_env` would refuse. `doctor --repair` runs no bootstrap.
- **The boundaries stay exactly where ADR-037 drew them.** `--repair` never
  does any of the following:
  - overwrite an artifact the operator edited (`Drifted`, `Conflict`);
  - change a pinned revision;
  - grant trust;
  - delete a record;
  - touch a project, a checkout or unlanded work.

  Plain `doctor` keeps its automatic, network-free lane unchanged.
- **The TUI and the CLI name the command.** A finding beyond the automatic
  lane says `uze doctor --repair` instead of describing a manual fix.

## Capabilities

### New Capabilities
- `machine-repair`: the explicit repair of UZE's own machine state. Covers the
  health model and its coverage, the recovery that runs before the
  application, what repair may and may not do, and how it reports.

### Modified Capabilities
<!-- none: `doctor`'s existing requirements (caching, package requirements)
and `environment-maintenance`'s automatic lane are unchanged; this change
adds an explicit mode beside them. -->

## Impact

- `crates/uze-application`:
  - a health catalogue (checks plus repairs) shared by `doctor`,
    `doctor --repair` and the TUI;
  - a repair service (plan, execute, re-inspect);
  - a `recover_home` entry point that does not need a constructed
    application.
- `crates/uze-core`: tier metadata on `UzeHome` entries, so that the health
  catalogue and the coverage test read one map.
- `src/main.rs` and `src/cli/`:
  - `doctor --repair` runs the recovery, skips bootstrap and renders the
    report;
  - `doctor` runs the recovery before `from_env`;
  - `--repair` is classified `JustifiedSlow`.
- Tests:
  - `tests/acceptance/repair.rs` is the damage matrix: delete or corrupt each
    owned thing, repair, then healthy;
  - the coverage test `every_owned_path_has_a_health_check`.
- Conformance: on real harnesses, delete `sources/` and a vendor store and
  corrupt `attachments.json`, then `doctor --repair`, then plugins load.
- Depends on `vendor-stores-and-uninstall`, for the store tier and
  `sources/`, and builds on `add-environment-maintenance-engine`, which
  provides the automatic lane and the outcome vocabulary.
