## Why

UZE reaches OpenCode through several generated plugins in its global plugin
directory: one `hooks-<package>.ts` per package with hooks, each embedding
the whole hook runtime, plus `uze-rules.ts` and `uze-agent-context.ts`. The
same runtime is copied once per package, the order between two packages'
hooks belongs to OpenCode and was never measured, and every V2 API move
means regenerating every copy. Meanwhile a project's own `.agents/agents`
is declared Unsupported on OpenCode, because no launch can hand them to the
shared background service from outside the checkout.

A measurement on OpenCode 2.1.2 (`experiments/opencode/project-agents-plugin`,
14/14) found the channel that closes that gap: a plugin in the global
directory is set up once per location, is handed that location, and can
register agents into that location's registry and reload it. One plugin,
owned by UZE, can carry every OpenCode capability UZE delivers.

## What Changes

- One generated runtime plugin in OpenCode's global plugin directory
  replaces `hooks-<package>.ts`, `uze-rules.ts` and `uze-agent-context.ts`.
  It is written by setup and by every command that installs, like the
  agent-context plugin today, and never written over a foreign file.
- A package's hooks become a generated document of its groups, receipt-owned
  per package, which the runtime reads; installing, updating or removing a
  package changes a document, not a plugin. A running service picks the
  change up without a restart.
- The runtime runs every package's groups in one pass, so first-deny-wins
  and handler order hold across packages, not only within one.
- New: a project's `.agents/agents` reach OpenCode. The runtime reads them
  from the root of the checkout it serves, offers them as
  subagents in that project alone, and reloads when the directory changes.
  The route moves from Unsupported to Adapted.
- Each capability in the runtime fails on its own: an error in one never
  disables another, and a hook failure still follows its group's effect.
- The earlier per-capability plugins whose content is UZE's are removed:
  `uze-agent-context.ts` and `uze-rules.ts` by setup, each
  `hooks-<package>.ts` by that package's next attach, once its document is
  in place.
  **BREAKING** for anyone relying on the file names `hooks-<package>.ts`,
  `uze-rules.ts` or `uze-agent-context.ts`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `portable-hooks`: OpenCode's managed hook delivery is a per-package
  document read by one runtime plugin, and ordering holds across packages.
- `portable-agents`: a project's own `.agents/agents` reach each harness,
  scoped to that project; OpenCode gains it through the runtime plugin.

Open changes edited in place, since they are not archived: `native-first-hooks`
(its OpenCode scenario and D7), `project-rules` (decision E5) and
`deliver-agent-context` (decision D2) name the files this change replaces.

## Impact

- `crates/uze-integrations/src/opencode/` (`agent_context.rs`, `hooks.rs`,
  `provision.rs`), `src/hooks/bridge.rs`, `src/rules/plugin.rs`: the
  separate templates merge into one runtime; the bridge emits a document.
- `UzeHome`: a generated-tier directory for OpenCode's hook documents.
- `crates/uze-integrations/src/opencode/README.md`, `docs/capabilities/portable-hooks.md`,
  `docs/architecture/agent-memory.mmd`: the delivery they describe.
- Conformance: the `context-project-agent-reaches-model` declaration in
  `harnesses/opencode/bindings.py` is retired for a measurement; a new
  cross-package hook scene; the hooks contract and the goldens run against
  the runtime.
