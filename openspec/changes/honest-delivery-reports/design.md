## Context

See [proposal.md](proposal.md). On `254daf6c`:

- `render_add_report` (`src/main.rs:3021`) collects locations into a
  `BTreeMap<harness, location>` and skips loose attachments of a harness
  with a plan (`:3044`); explicit vs generated only differ in evidence
  strings shown with `--verbose`/JSON (`claude/plugin.rs:45`).
- `install_authorized` (`uze-application/.../lifecycle/install.rs:78`)
  ingests into the Store (`:110`) before delivery (`:170`) and returns the
  first delivery error (`:198`) with no undo; receipts are recorded per
  resource as it attaches (`attach.rs:194,234`).
- `update_machine` (`project_environment.rs:572`) compares
  `provenance.resolved.display()`, a checkout path for a linked source.
- `doctor` counts receipts by state (`read_models.rs:1034`); `uze inspect`
  (`read_models.rs:28`) already shows plan route and per-capability route —
  the effective view extends it rather than adding a verb.

## Goals / Non-Goals

**Goals:** one delivery model, rendered by install, update, inspect and
doctor alike; failure never reads as success.

**Non-Goals:** launching a harness to observe it (that is the Lab's job);
changing routes (that is `deliver-the-whole-plugin`).

## Decisions

### One delivery report model

Integrations already produce `ExposurePlan`s with route and evidence. Add to
each planned capability the exposed name and a status with its reason
(`Native`, `Adapted { not_carried }`, `Shadowed { by }`, `Unsupported
{ why }`), produced by the integration, rendered generically. `inspect
--harness` computes the same plans without attaching (plans are pure over the
Store and machine state), so the effective view and the install report
cannot disagree.

### Rollback by the receipts the attempt wrote

Install collects the receipts it recorded; if no harness attached, it
detaches them (inspect-before-detach still applies) and removes the Store
entry only if this install created it (`ingest` returns an existing package
for the same origin — that one stays). Partial delivery is a record under
`state/`, shaped, cleared by the next full delivery.

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

- [Install output gets longer] → one line per harness by default with counts
  and non-native items expanded; full list under `--verbose` and JSON.
- [Rollback removing a package the user had before] → only a package this
  install created is removed; covered by a test with a pre-existing install.
