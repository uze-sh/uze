## Why

UZE recognizes package `agents/` content during acquisition but discards it
from the effective environment, leaving reusable subagents unavailable in all
four supported harnesses. The current harnesses now provide documented agent
surfaces, so UZE can make agents portable with an explicit, evidence-backed
projection instead of keeping the matrix cell as roadmap.

## What Changes

- Introduce Markdown definitions below `agents/` as UZE's canonical Agent
  capability, with a `harness:` frontmatter block for what each harness
  spells differently.
- Discover canonical agent definitions from installed packages, preserve their
  bytes in the Store, and route them independently of Skills and MCP.
- Project agents natively to Claude Code, OpenCode, and Antigravity CLI, and
  as a generated native TOML custom-agent file to Codex CLI, each under the
  qualified label `<plugin>:<logical name>`.
- Report each root field a harness does not carry on that agent's route
  (Degraded), and check agent definitions in `uze agent plugin check`.
- Extend receipts, inspection, lifecycle safety, TUI compatibility rows,
  generated harness matrix, deterministic tests, and real-harness conformance
  coverage for the new capability.

## Capabilities

### New Capabilities

- `portable-agents`: Portable discovery, routing, delivery, lifecycle, and
  support reporting for package-defined agent profiles across supported
  harnesses.

### Modified Capabilities

- None.

## Impact

Affected areas include `uze-core` capability discovery and resource identity,
all four modules in `uze-integrations`, the shared exposure/receipt model,
the TUI and harness-matrix generator (`web/content/docs/reference/harnesses.mdx`,
`web/lib/harness-matrix.json`), `plugin check`, fixtures, integration/acceptance tests,
and the four conformance verticals. No new runtime dependency is required.

## Not in this change

- Install-time validation of agent definitions: install delivers what
  `plugin check` would flag (Codex receives a placeholder description for an
  agent without one). Follow-up.
