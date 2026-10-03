## Context

`deliver-the-whole-plugin` settled how bytes reach a harness. What each
harness accepts was measured by the Lab on 2026-09-28 (image `beta4-final`)
but lived in comments and branches inside each integration. This change
records those measurements next to the code that depends on them, gives
authors a portable per-harness block, and reads the project's `.agents/`
without writing into the checkout.

## Goals / Non-Goals

**Goals:**
- Every measured fact a delivery leans on is stated once, with the version
  it was measured on and the Lab function that proves it.
- What a harness accepts on an agent is one declaration that renders the
  file, answers `plugin check` and reports the loss at install.
- A `harness:` block lets an author say something to one harness only.
- The project's `./.agents/` reaches every harness that can take it,
  without a write into the repository.

**Non-Goals:**
- Turning discovery roots, link-following or placeholder handling into data
  a generic delivery reads. They stay code in each vertical.
- Running every fact's proof on every nightly (experiments run on demand).

## Decisions

- **Facts are data on the integration, not a central table.** Each vertical
  holds its `FACTS: &[HarnessFact]` (`subject`, `fact`, `measured_on`,
  `proven_by`) and returns it from `IntegrationPort::facts`. The Core stays
  vendor-neutral, `uze-harness-matrix` renders the table, and
  `tests/integrations/facts.rs` fails a fact whose `proven_by` names a
  function the Lab does not define. The test checks existence, not outcome:
  the outcome is the Lab's to prove.
- **Field acceptance is an `AgentDialect` per vertical**
  (`shared::dialect`): the known fields with their `Shape`, and whether the
  harness carries an unknown field or refuses it. `agent_block` renders the
  delivered block and the `plugin check` findings from the same value, and
  a root field the harness does not carry turns the route Degraded with the
  field named (`shared::agent::projection_route`).
- **No link-following switch.** Every delivery is physical, so no harness's
  link behaviour is on the path. Placeholders are resolved by UZE on every
  route, because no harness expands the portable `${PLUGIN_ROOT}`.
- **`harness:` block** (approved 2026-09-28): keyed by harness id or alias,
  opaque to the Core (`uze_core::capability::harness`), rendered by the
  integration and never delivered; `name`, `description` and `invoke` are
  root-only; `model` and `tools` at the root warn.
- **`plugin check` is layered**: the Core validates what every harness
  shares, and `IntegrationPort::check_capability` adds each harness's layer
  from the same dialect its delivery renders with.
- **Agent Plugins 1.0 is advice, never a finding**
  (`package::authoring::agent_plugins`). UZE's own surfaces stay as the
  files and fields the standard leaves to each client;
  `extensions["sh.uze"]` is reserved and holds no setting UZE reads.
- **Project `.agents/` is read and mounted, never written.** A harness that
  reads it natively is left alone; Claude Code gets it linked into the
  runtime projection it adds with `--add-dir`; Codex gets project agents as
  roles in a `-c agents={...}` layer; OpenCode's project agents are
  Unsupported, measured (`experiments/opencode/project-agents`).

## Risks / Trade-offs

- A fact proved only by an experiment can go stale between on-demand runs;
  the matrix still shows it with its measured version, which is the signal
  to rerun it.
- `facts.rs` greps for `def <fn>(`; a renamed Lab function is caught, a
  weakened one is not.
