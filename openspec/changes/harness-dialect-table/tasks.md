## 0. Decisions

- [x] 0.1 The `harness:` block's name and shape, approved 2026-09-28: keyed
  by harness id or alias; `name`, `description` and `invoke` are root-only;
  `model` and `tools` at the root warn; the block is never delivered.
- [x] 0.2 Ownership of files inside a repository. Decided 2026-09-28: UZE
  writes nothing into `./.agents/`; the project authors it and UZE reads and
  mounts it.

## 1. Dialect table

- [x] 1.1 Per (harness, artifact kind, fact) table, version-stamped, each
  fact naming its Lab check; delivery derives roots, link-following,
  placeholders and field acceptance from it. `IntegrationPort::facts` and
  each integration's `FACTS`; field acceptance is data the delivery renders
  from (each integration's agent dialect); link-following needs no switch
  since every delivery is physical; placeholders are resolved by UZE on
  every route because no harness expands the portable one. The table is
  published on the harness page (`uze-harness-matrix`).
- [x] 1.2 Lab: one contract check per fact; the nightly fails on a fact that
  stops holding. Each fact names its Lab function, and
  `tests/integrations/facts.rs` fails a fact whose function the Lab does not
  define.

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

- [x] 3.1 Canonical package conformant with Agent Plugins 1.0, UZE-only
  surfaces under `extensions["sh.uze"]`. `plugin check` judges the standard
  as advice, never as a finding (`uze_core::authoring::agent_plugins`);
  the scaffold is valid under both; a `./` MCP `command`/`cwd` resolves to
  the package (`every_scaffold_passes_its_own_check`,
  `check_names_what_keeps_a_package_from_agent_plugins_without_refusing_it`,
  `an_agent_plugins_relative_command_runs_from_the_package`,
  `tests/packages/authoring.rs::check_names_what_keeps_a_plugin_from_agent_plugins_and_still_passes`).

## 4. Project `.agents/`, read and mounted

- [ ] 4.1 Read the project's `./.agents/` and mount what a harness does not
  read there natively into its runtime projection (outside the repository);
  a harness that reads it natively is left alone.
- [ ] 4.2 Lab: a project `.agents/skills` skill reaches every harness, with
  nothing written into the checkout.
