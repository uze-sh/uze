## Why

`deliver-the-whole-plugin` settled *how* bytes reach a harness. What each
harness *accepts* is still spread through the integrations as constants,
comments and branches: which frontmatter fields it honours or drops (OpenCode
silently drops an agent with `model: haiku` or a string `tools`; Codex refuses
unknown keys in an agent TOML), where it discovers each artifact kind, whether
it follows a link, which placeholders it expands. Every one of those was
measured by the Lab (study runs of 2026-09-28, image `beta4-final`), and none
of them is recorded as data a delivery is derived from. A harness that moves
breaks a delivery silently, and a new harness means rediscovering all of it.

Authors also have no portable way to say something per harness (a model for
Claude, a sandbox for Codex) without writing a vendor field the other
harnesses drop or refuse.

## What Changes

- **A table of measured facts**: each harness states what it was measured
  to do (`IntegrationPort::facts`), each fact naming the harness version it
  was measured on and the Lab function that proves it. A test fails a fact
  whose function the Lab does not define, and the table is rendered into
  the harness matrix. The fields a harness accepts on an agent are declared
  as data in its own vertical (its agent dialect), and that one declaration
  renders the delivered file, answers `plugin check` and states the loss at
  install.
- **Canonical `harness:` frontmatter** for skills and agents, keyed by harness
  id (aliases resolved by the registry), rendered through the table and never
  delivered. Identity fields (`name`, `description`) stay at the root and are
  not overridable.
- **The canonical package conforms to Agent Plugins 1.0** (`plugin.json`,
  `skills/`, `mcp.json`). What UZE adds beyond the standard (`agents/`,
  `hooks.json`, `AGENTS.md`, a skill's `invoke:` and `harness:` blocks)
  stays in the files and fields the standard leaves to each client, so it
  never counts against conformance; `extensions["sh.uze"]` is reserved as
  UZE's manifest namespace and holds no setting UZE reads today.
- **Project `.agents/` is read, never written.** The project authors
  `./.agents/`, as it does `AGENTS.md`; UZE writes nothing into the
  repository. A harness that reads `./.agents/` natively (skills on Codex,
  OpenCode and Antigravity; agents, MCP and hooks on Antigravity) needs
  nothing. For one that does not (Claude Code reads `.claude/`), UZE mounts
  the project's `.agents/` content into that harness's runtime projection,
  outside the repository, the way it already projects `AGENTS.md`. The table
  says which harness reads which kind there.

## Decisions

- The `harness:` block (approved 2026-09-28): keyed by harness id or alias,
  opaque to the Core, rendered by the integration; `name`, `description` and
  `invoke` root-only; `model` and `tools` at the root warn.
- `plugin check` is layered: the Core validates what every harness shares,
  and each integration adds its own layer from the same dialect its delivery
  renders with, so the check says what the install would do.
- Project `.agents/` is read and mounted, never written.

## Capabilities

### Modified Capabilities

- `transparent-harness-attachment`: delivery is derived from a measured
  dialect table.

## Impact

- `crates/uze-integrations`: `shared::dialect` (the agent dialect type and
  renderer), and in each vertical its `FACTS` and its agent dialect.
- `crates/uze-core`: `HarnessFact` and `IntegrationPort::facts`;
  `package::authoring::agent_plugins` for the standard.
- `crates/uze-core/src/capability/{skill,agent}.rs`: the `harness:` block.
- `tests/integrations/facts.rs`, `src/bin/uze-harness-matrix.rs`.

## Not in this change

- Discovery roots, link-following and placeholder handling as table data
  that delivery reads: they remain code in each vertical, and the table
  states them as prose. Follow-up.
- Rechecking the facts proved only by `conformance/experiments/` on every
  nightly: today they run only on demand (`lab.py --experiment`). Follow-up.
