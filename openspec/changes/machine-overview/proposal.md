## Why

An operator's harnesses carry skills, agents, MCP servers, hooks and
instruction files from many places — uze, the harness's own plugins, an
account's connectors, files placed by hand — and nothing says which of them
are used, which are broken, or what each one costs in context before a word
is typed. The management modal's Overview screen exists behind a feature
flag whose own comment says what it should be is undecided; this change
decides it: the machine's health, across every harness.

## What Changes

- The manage Overview becomes the machine overview: everything every
  detected harness carries at machine level, whoever installed it, grouped
  by kind (skills, agents, mcp servers, hooks, rules) in one tree, with
  uze's own health findings as a group above them when there are any.
- For each resource it shows where it came from, how often it was used in a
  period (7, 30 or 90 days), the tokens it puts in every session, and a
  status: in use, unused, broken, unknown.
- Sessions are counted per harness; a harness can narrow the whole screen.
- Tokens are measured on the artifact itself, locally and offline, and shown
  as estimates.
- Uses are read from each harness's own session records, incrementally and
  off every interactive path; a harness or a metric that cannot be read is
  reported as unknown, never guessed.
- Read only: the screen changes nothing outside uze's own cache. Removing
  what uze installed points to the Marketplace; removing what it did not
  shows the harness's own command for it, which the operator runs.
- The Overview route leaves its feature flag once the screen is complete.

Not in this change: adopting a resource into uze, running any harness's
removal command, a CLI report, per-project resources.

## Capabilities

### New Capabilities
- `machine-overview`: what the manage Overview reports about the machine's
  harnesses — inventory, origin, per-session tokens, sessions, uses, status
  — and the performance, privacy and read-only bounds of collecting it.

### Modified Capabilities
None. The `overview` capability describes the workspace summary read model
(project and marketplace at the cwd), which this change does not touch.

## Impact

- `uze-core`: a harness-neutral model of a found resource, a usage event and
  a session count, a token estimator, and optional `IntegrationPort` methods
  whose default answer is "not known".
- `uze-integrations`: inventory, session counting and usage reading for
  Claude Code and Codex; inventory and session counting for OpenCode and
  Antigravity, whose uses stay unknown in this change.
- `uze-application`: one read model and a background scan with an
  incremental index in the `cache/` tier.
- `src/ui`: the Overview screen; the feature flag is retired.
- No new dependency, no network, no write outside `$UZE_HOME/cache`, and no
  added work on any CLI command, the launch path or the runtime shim.
