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

- **A dialect table**: per (harness, artifact kind, fact), a version-stamped
  record of discovery roots, fields honoured or dropped, link-following and
  placeholder expansion, each fact naming the Lab check that proves it.
  Delivery reads the table; the nightly fails when a fact stops holding.
- **Canonical `harness:` frontmatter** for skills and agents, keyed by harness
  id (aliases resolved by the registry), rendered through the table and never
  delivered. Identity fields (`name`, `description`) stay at the root and are
  not overridable.
- **The canonical package conforms to Agent Plugins 1.0** (`plugin.json`,
  `skills/`, `mcp.json`), with UZE-only surfaces (agents, hooks, invocation
  policy) under `extensions["sh.uze"]`, and every vendor envelope rendered
  from it.
- **Project `.agents/`**: project-scoped plugins delivered into `./.agents/`
  for the harnesses that read it (skills on Codex, OpenCode and Antigravity;
  agents, MCP and hooks on Antigravity), once the ownership of a UZE-written
  file inside a repository is decided.

## Decisions this waits on

- The `harness:` block's name and shape (validated with a second model on
  2026-09-28: a block keyed by harness id, opaque to the Core, rendered by the
  integration; root `model` reserved).
- Whether files UZE writes into a repository are committed or ignored, and
  who owns them.

## Capabilities

### Modified Capabilities

- `transparent-harness-attachment`: delivery is derived from a measured
  dialect table.

## Impact

- `crates/uze-integrations`: a `dialect` module per harness; the constants
  and branches it replaces.
- `crates/uze-core/src/capability/{skill,agent}.rs`: the `harness:` block.
- `conformance/contract`: one check per table fact.
