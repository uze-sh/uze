## Context

See proposal.md for why. What the code does today:

- `hooks/bridge.rs` writes `<config root>/plugins/hooks-<package>.ts` per
  package: the runtime (handler spawn, `HOOK_*` environment, order,
  first-deny-wins, deadlines, fail-closed by effect, the alias table) with
  the package's groups as data. A denial renames the call to a tool that
  does not exist, so another package's `execute.before` sees the substituted
  name (read from the bridge, not yet measured).
- `rules/plugin.rs` writes `uze-rules.ts`, which asks the `sh` rules engine on
  `context` and `execute.after`. It imports nothing, because OpenCode 2.0.18
  refused a plugin-directory file importing its own plugin package; `node:`
  built-ins are fine (`uze-agent-context.ts` imports `node:fs`).
- `opencode/agent_context.rs` writes `uze-agent-context.ts`, owned by its
  first line, the directory it reads baked into its source because the
  shared service's environment is not the launch's.

Measured on OpenCode 2.1.2 (`conformance/experiments/opencode/project-agents-plugin.py`):
a global plugin's `setup` runs once per location in the one service process;
`ctx.location` gives the directory and `project.directory`, the repository
root; `ctx.agent.transform` registers agents into that location's registry;
`ctx.agent.reload()` from an `fs.watch` re-runs the transform with no
restart; a dispatched subagent receives the registered `system` as its
system message.

## Goals / Non-Goals

**Goals:** one runtime file for OpenCode; hook data out of code; ordering
across packages; project agents on OpenCode; each feature isolated.

**Non-Goals:** changing the handler contract (`HOOK_*` in, exit code out),
the rules engine, or the agent-context document; any harness other than
OpenCode; a hook `transform` effect.

## Decisions

### D1: One runtime plugin, features as sections

`<config root>/plugins/uze.ts`, written by `install` (setup and every
command that installs) when missing or when the file there carries UZE's
header and differs, never over a file without it (D2 of
`deliver-agent-context`, generalized). Inside, one section per capability:
hooks, rules, agent context, project agents, each registering its own V2
hooks and each wrapped so that a throw is logged and ends only that section.
Paths it needs (hook documents, rules engine, sessions, agent context) are
baked into its source.

- Alternative: keep one file per capability, only dropping the per-package
  hook copies. Rejected: four files and four setups per location for one
  owner, and the same "UZE's header, never a foreign file" logic four times.
- Alternative: regenerate one plugin with every package's groups inlined.
  Rejected: every lifecycle operation rewrites a shared file that no
  package's receipt can own.

### D2: Hook groups are a generated document per package

`UzeHome::generated_attachments_dir("opencode")/hooks/<package>.json`, the
same tier and parent as the other harnesses' `hooks/exec`: generated,
rebuildable from Store and Engine. The document is what `bridge_hooks`
emits today, plus a version field; it names no packager (ADR-040's D8 holds
for the per-package artifact). It is the package's receipt-owned artifact,
inspected by content before removal, as the bridge file is now. The runtime
reads the directory at setup and on an `fs.watch` event, keeping the parsed
groups in memory; a call never reads the disk.

- Alternative: documents in OpenCode's config root. Rejected: a directory
  OpenCode does not define may be claimed by a later release.

### D3: One pass over every package's groups

For an event, the runtime orders groups by package id, then manifest order,
and runs them as the bridge runs one package's today: the first denial
stops the rest and is applied once. The substituted-tool-name trick then
happens once, after every group has seen the real call.

### D4: An unreadable document denies every tool call

A document that does not parse makes the hooks section answer every
`execute.before` with a denial naming the file, until a watch event brings
a readable one. Without the document the runtime cannot tell which calls a
deny group guarded, and the delivery model makes an unreadable ledger block,
not authorize. Other sections are untouched.

### D5: Project agents read from the checkout root, translated in TypeScript

The section reads `<ctx.location.project.directory>/.agents/agents/*.md`.
OpenCode sets `directory` to the checkout's own top level (`repo.worktree`
in `core/src/project.ts`), so a linked checkout under `.worktrees/<id>`
reads its branch's agents; `canonical`, the main checkout, is never used.
Outside a repository `directory` is the location itself.

Each file is parsed with `Bun.YAML.parse` (OpenCode 2.1.2 runs plugins on
Bun 1.4.2; measured, no import needed) and projected with the portable
subset: `name`, `description`, body as `system`, `mode: subagent`, and the
`harness.opencode` block's V2 fields. The transform edits the resolved
`Agent.Info`, not the configuration shape, so the section converts as
OpenCode's own `ConfigAgentPlugin` does: `model` `provider/model` becomes
`{providerID, id, variant?}`, permission rules are expanded the same way
(`~` included), and `disabled` means not registered.

That makes it a second implementation of the Rust dialect, held to it by
one fixture table: for each fixture, the TypeScript projection under Bun
must carry and withhold exactly the fields Rust does, with the same
reasons; values are compared after the shape conversion above.

### D5b: Precedence follows OpenCode's plugin order

External plugins activate after OpenCode's internal `pre` plugins and
before its `post` ones (`plugin/supervisor.ts`, `plugin/internal.ts`). The
built-in agents (`AgentPlugin`, pre) therefore exist when the section's
transform runs, and a project agent with a built-in's id is skipped. The
configured agents (`ConfigAgentPlugin`, post: `opencode.json`,
`.opencode/agents`, the global agents root UZE's package agents use) apply
afterwards, field by field, over any agent already registered. A project
agent sharing a configured agent's id is therefore not skipped but
overlaid by the configuration, and the operator's global permission rules
reach every project agent. Both are OpenCode's precedence, kept as is.

- Alternative: read OpenCode's configured roots to skip those ids too.
  Rejected: it re-implements OpenCode's discovery inside the plugin.

- Alternative: UZE precomputes project agents at install. Rejected: a file
  the operator writes would not appear until UZE runs again, and the plugin
  reaches sessions UZE never launched.

### D5c: Reported as delivered by setup, like the rules

`ProjectResourceRoute` gains a fourth answer, handed over by what setup
delivers into the harness's own configuration, inside the workspace or
outside it; OpenCode gives it for `Agents`. `ContextMechanism`'s
`RulesEngine`/`RulesEngineMissing` generalize to `SetupDelivered`/
`SetupMissing`, used by rules and project agents alike, plus `Blocked`
when a foreign file holds the artifact's path. The workspace renders them
as Delivered, Run uze setup and Blocked; `uze doctor` names the file.

- Alternative: a third pair of variants for the runtime plugin. Rejected:
  on OpenCode rules and agents depend on the same file, and two names for
  one state would let the drawer show one delivered and the other missing.

### D6: Earlier files are converged, not migrated

Setup deletes `hooks-*.ts`, `uze-rules.ts` and `uze-agent-context.ts` in
the plugin directory only when their content carries an earlier generated
header; a package's bridge receipt is replaced by its document's on the
next reconcile, through the receipts' normal detach (inspect, then remove).
No compatibility path reads the old files.

## Candidate ADRs

- OpenCode receives UZE through one runtime plugin reading generated
  documents: it supersedes ADR-040's "the plugin is the wrapper, with the
  package's groups as data inside it", and ties OpenCode's hooks to a
  generated directory under `$UZE_HOME` as the other harnesses' wrappers
  already are.

## Risks / Trade-offs

- [One file carries four features] → each section isolated (D1); the
  Lab's hooks, rules, context and agent contracts all run against it.
- [OpenCode reorders its plugin phases] → D5b rests on the `pre`/`post`
  split read from source; the name-collision scene (task 1.3) measures it,
  and a change of order turns that scene red, not silent.
- [Deleting `$UZE_HOME/runtime/` silences OpenCode's deny groups until the
  next delivery] → the same exposure the other harnesses' `hooks/exec` has;
  `uze doctor` reports a hook receipt whose document is missing.
- [The TypeScript projection drifts from Rust's] → the shared golden table.
- [A watch event lost on some filesystem] → the next setup of a location
  reads again; documents are also re-read when a session is created.

## Migration Plan

Ship in two steps under this change: (1) the runtime plugin with the agent
context and project agents sections, retiring `uze-agent-context.ts`;
(2) hooks and rules move in, retiring `hooks-*.ts` and `uze-rules.ts`.
Rollback of either step is the previous build's setup, which writes its own
files again; the documents are generated and ignored by it.
