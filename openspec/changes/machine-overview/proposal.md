## Why

An operator's harnesses carry skills, agents, MCP servers, hooks and
instruction files from many places — uze, the harness's own plugins, files
placed by hand — and nothing shows them together, says which are broken or
duplicated, what each one costs in context before a word is typed, or
whether anyone still uses it. The
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
- Sessions are counted per harness, every one the harness still keeps a
  record of, from file metadata or the harness's published statistics, with
  how far back those records reach.
- Each skill, agent and MCP server shows how often it was used and when last,
  whether the person typed it or the model chose it, read from the records
  every harness already keeps of its sessions, so the count covers the past
  from the first opening; one with no use reads `unused`. Only the names of
  what was used leave a record, and a record already read is not read again.
- Only what is consistent across all four harnesses is reported. Per-session
  tokens of MCP servers and hooks, uses of hooks, and MCP sign-in state are
  not in this version.
- Read only: nothing outside uze's own cache is written. Removing what uze
  installed points to the Marketplace; removing what it did not shows the
  harness's own command, which the operator runs. No hook is added to any
  harness to count uses.
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

- `uze-core`: a harness-neutral model of a found resource, a session count
  and a use, a token estimator, static health checks, the incremental reading
  of usage records, and four optional `IntegrationPort` methods whose default
  answer is "not known".
- `uze-integrations`: inventory, session counting and usage reading for
  Claude Code, Codex, OpenCode and Antigravity.
- `uze-application`: one read model, a background collection, a snapshot and
  the usage records' cursors in the `cache/` tier.
- `src/ui`: the Overview screen; the feature flag is retired.
- No new dependency, no network, no hook delivered, no write outside
  `$UZE_HOME/cache`, and no added work on any CLI command, the launch path,
  the runtime shim or an agent's turn.
