## Context

See [proposal.md](proposal.md). Before this change (on `254daf6c`):

- `render_add_report` (then in `src/main.rs`, now `src/cli/report.rs`)
  collected locations into a `BTreeMap<harness, location>` and skipped loose
  attachments of a harness with a plan; explicit vs generated envelopes only
  differed in evidence strings shown with `--verbose`/JSON.
- `install_authorized` (`uze-application`, `lifecycle/install.rs`) ingested
  into the Store before delivery and returned the first delivery error with
  no undo; receipts were recorded per resource as it attached
  (`lifecycle/attach.rs`).
- `update_machine` (`project_environment.rs`) compared
  `provenance.resolved.display()`, a checkout path for a linked source.
- `doctor` counted receipts by state; `uze inspect` (`read_models.rs`,
  `Plugins::inspect`) already showed plan route and per-capability route,
  so the effective view extends it rather than adding a verb.

## Goals / Non-Goals

**Goals:** one delivery model, rendered by install, update, inspect and
doctor alike; failure never reads as success.

**Non-Goals:** launching a harness to observe it (that is the Lab's job);
changing routes (that is `deliver-the-whole-plugin`).

## Decisions

### One delivery report model

Integrations already produce `ExposurePlan`s (`route`, `mechanism`,
`evidence`), with the route one of `CompatibilityRoute::{Native, Adaptable,
Degraded, Unsupported}`; that stays the status vocabulary, and the evidence
of a degraded agent leads with the fields not carried. No `Shadowed` status
was added: a capability whose name something UZE does not own already holds
is reported as blocked, with the reason. The exposed name is not on the plan:
the effective view (`lifecycle/effective.rs`, `CapabilityDelivery`) asks the
integration for it (`packaged_exposure_name` for a capability the package
envelope carries, the resolved exposure name or the plan's own artifact
otherwise). `inspect` and `doctor` share one planning pass
(`plan_delivery_to`), computed without attaching. The install report
(`HarnessDeliveryReport`: route, attachments, blocked, shortfalls) is built
from the delivery itself, and its agreement with `inspect` on route and
shortfalls is held by a test
(`tests/cli/machine.rs::inspect_and_install_report_agree_on_every_harness`)
rather than by construction.

### Rollback by the receipts the attempt wrote

Install collects the receipts it recorded; if no harness attached, it
detaches them (inspect-before-detach still applies) and removes the Store
entry only if this install created it (`ingest` returns an existing package
for the same origin — that one stays). Partial delivery is a record under
`state/` (`UndeliveredRegistry`, `Shaped`), cleared per harness when a
later delivery reaches it.

### Content identity for update

Compare the Store package digest before and after (the lock already carries a
digest per package); provenance is shown, not compared.

### Readability checks belong to integrations

`IntegrationPort::unreadable(package, receipt, served)` answers, for a
receipt that still inspects as matched, what the harness would not load of
it; the default answers nothing. Each integration adds the vendor-side facts
it alone knows: Claude Code, that the plugin its marketplace points at still
exists and that the cached copy its record names carries every skill and
agent, and that a user agent's frontmatter `name` is the label it is filed
under; OpenCode, that an agent file has `mode: subagent`, a `provider/model`
`model` and a map for `tools`. `doctor` compares the plan `inspect` computes
with the receipts and their inspection, then asks this. Core stays
vendor-free.

## Risks / Trade-offs

- [Install output gets longer] → one header per harness with its route and
  why, its attachments and only the capabilities short of native or
  blocked; the full route evidence waits for `--verbose`. The full
  per-capability list with exposed names is `inspect`'s.
- [Rollback removing a package the user had before] → only a package this
  install created is removed; covered by a test with a pre-existing install.
