## 0. Decisions

- [x] 0.1 The `harness:` block's name and shape, approved 2026-09-28: keyed
  by harness id or alias; `name`, `description` and `invoke` are root-only;
  `model` and `tools` at the root warn; the block is never delivered.
- [x] 0.2 Ownership of files inside a repository. Decided 2026-09-28: UZE
  writes nothing into `./.agents/`; the project authors it and UZE reads and
  mounts it.

## 1. Dialect table

- [x] 1.1 Per harness, a table of measured facts (`HarnessFact` through
  `IntegrationPort::facts`, each vertical's `FACTS`), each fact stating the
  version it was measured on and the Lab function that proves it, and
  published on the harness page (`uze-harness-matrix`). Field acceptance is
  data the delivery renders from (each integration's agent dialect,
  `shared::dialect`); link-following needs no switch since every delivery
  is physical; placeholders are resolved by UZE on every route because no
  harness expands the portable one. Discovery roots stay code in each
  vertical.
- [x] 1.2 Each fact names its Lab function, and
  `tests/integrations/facts.rs` fails a fact whose function the Lab does
  not define (it checks that `def <fn>(` exists, not that it passes). Facts
  proved by `conformance/contract/` are rechecked by the conformance run;
  the six proved by `conformance/experiments/` run only on demand.

## 2. Canonical frontmatter

- [x] 2.1 `harness:` block for skills and agents, keyed by harness id,
  rendered through the table, never delivered
  (`uze_core::capability::harness`, `shared::dialect`; each integration
  declares its agent dialect as data).
- [x] 2.2 `plugin check` validates it and names the harness that would drop
  a field: a common layer in the Core, and one layer per integration through
  `IntegrationPort::check_capability`, the same declaration the delivery
  renders from (`tests/packages/authoring.rs`).

## 3. Standards

- [x] 3.1 Canonical package conformant with Agent Plugins 1.0; UZE-only
  surfaces (agents, hooks, invocation policy) stay as files and fields the
  standard leaves to each client, and `extensions["sh.uze"]` is the
  reserved manifest namespace, holding no setting UZE reads. `plugin check` judges the standard
  as advice, never as a finding (`uze_core::authoring::agent_plugins`);
  the scaffold is valid under both; a `./` MCP `command`/`cwd` resolves to
  the package (`every_scaffold_passes_its_own_check`,
  `check_names_what_keeps_a_package_from_agent_plugins_without_refusing_it`,
  `an_agent_plugins_relative_command_runs_from_the_package`,
  `tests/packages/authoring.rs::check_names_what_keeps_a_plugin_from_agent_plugins_and_still_passes`).

## 4. Project `.agents/`, read and mounted

- [x] 4.1 Read the project's `./.agents/` and mount what a harness does not
  read there natively into its runtime projection (outside the repository);
  a harness that reads it natively is left alone. Codex, OpenCode and
  Antigravity read `./.agents/skills` themselves and get no contribution;
  Claude Code reads neither `.agents/skills` nor `.agents/agents`, and its
  launcher links both into `.claude/` of the runtime projection it adds with
  `--add-dir` (`tests/integrations/runtime_projection.rs::a_project_agents_directory_reaches_every_harness_without_a_write_into_the_checkout`,
  `claude::runtime::runtime_projection_tests`). `./.agents/agents` is
  answered per harness (`IntegrationPort::project_resource_route`, one row
  per kind in the status views): Antigravity reads it itself; Claude Code
  gets it linked into the same `--add-dir` target; Codex, which reads only
  `.codex/agents`, gets each agent as a role in a `-c agents={...}` layer
  its launcher adds, a role file per agent under the runtime projection
  (`codex::runtime::tests`); OpenCode is Unsupported, measured: its only
  outside-the-checkout mechanisms either replace the user's configuration
  (`OPENCODE_CONFIG_DIR`) or reach the shared background service, which
  would offer one project's agents in every other
  (`experiments/opencode/project-agents`). A project agent is labelled by
  its logical name on every harness.
- [x] 4.2 Lab: a project `.agents/skills` skill reaches every harness, with
  nothing written into the checkout (contract `context`:
  `context-project-skill-reaches-model`,
  `context-project-skill-checkout-untouched`, launched through UZE's
  launcher; `context-project-agent-reaches-model` for `.agents/agents` on
  every harness claimed to receive it, declared unsupported on OpenCode).
