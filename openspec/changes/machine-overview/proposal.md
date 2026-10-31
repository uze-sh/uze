## Why

An operator's harnesses carry skills, agents, MCP servers, hooks and
instruction files from many places — uze, the harness's own plugins, files
placed by hand — and nothing shows them together, says which are broken or
duplicated, or what each one costs in context before a word is typed. The
management modal's Overview screen exists behind a feature flag whose own
comment says what it should be is undecided; this change decides it: the
machine's health, across every harness.

## What Changes

- The manage Overview becomes the machine overview: everything every
  detected harness carries at machine level, whoever installed it, grouped
  by kind (skills, agents, mcp servers, hooks, rules) in one tree, with
  uze's own health findings as a group above them when there are any.
- For each resource it shows its origin, the tokens it puts in every
  session and the tokens it loads when used (measured on its files, offline,
  as estimates), and a status: `ok`, `broken` (a fact on disk proves it
  cannot work) or `duplicate` (the same name with different content).
- Sessions are counted per harness and period, from file metadata or the
  harness's published statistics, never from what a session says.
- Only what is consistent across all four harnesses is reported. Per-resource
  uses, per-session tokens of MCP servers and hooks, and MCP sign-in state
  are not in this version: only some harnesses can answer them.
- Read only: nothing outside uze's own cache is written. Removing what uze
  installed points to the Marketplace; removing what it did not shows the
  harness's own command, which the operator runs.
- The Overview route leaves its feature flag once the screen is complete.

## Capabilities

### New Capabilities
- `machine-overview`: what the manage Overview reports about the machine's
  harnesses — inventory, origin, tokens, sessions, static health — and the
  performance, privacy and read-only bounds of collecting it.

### Modified Capabilities
None. The `overview` capability describes the workspace summary read model
(project and marketplace at the cwd), which this change does not touch.

## Impact

- `uze-core`: a harness-neutral model of a found resource and a session
  count, a token estimator, static health checks, and two optional
  `IntegrationPort` methods whose default answer is "not known".
- `uze-integrations`: inventory and session counting for Claude Code,
  Codex, OpenCode and Antigravity.
- `uze-application`: one read model, a background collection and a
  snapshot in the `cache/` tier.
- `src/ui`: the Overview screen; the feature flag is retired.
- No new dependency, no network, no session transcript read, no write
  outside `$UZE_HOME/cache`, and no added work on any CLI command, the
  launch path or the runtime shim.
