## Why

What an agent learns about a project lives in its harness and nowhere
else, and every harness keeps it differently:

- Claude: Markdown under `~/.claude/projects/`.
- Codex: consolidated Markdown summaries under `~/.codex/memories/`, off by default.
- Antigravity: opaque protobufs.
- OpenCode: nothing at all.

None of them reads another's. An agent the workspace moves to another
harness loses what it learned, and the operator re-teaches the same facts
once per harness.

The decision that left memory out of the capability model
(`validate-universal-agent-environment`,
*scope-capability-model-to-standards-gap*) called it "deferred, not
abandoned", because there was no standard and no common native support.
Both are still true. Two things changed:

- UZE now launches the agents.
- `deliver-agent-context` gives every launched agent a context, delivered
  invisibly through a channel proven per harness.

Memory no longer needs four stores translated into each other. It needs
one store and the channel that already exists.

**Why UZE keeps it, rather than a plugin with an MCP memory server.** A
memory server reaches all four harnesses, but only when the model decides
to ask it. The index, the part that has to be there *before* the model
knows what to ask, needs the launch-time channel, and that is UZE's core.
Provenance comes from the launch identity, which only UZE has. The
workspace is the one place an operator can be asked before an agent's fact
reaches every project. Keeping the store beside them is cheaper than an
API between a core channel and a plugin. A server can still be added later
as one more way to reach the same verbs.

## What Changes

- **A memory UZE owns.**
  - One Markdown file per fact, with front matter, kept outside every
    checkout.
  - Two scopes: the project and the machine.
  - The operator reads and edits the files as text.
- **`uze agent memory remember|forget|list|show`, with `-m`/`--machine`.**
  - The agent audience's verbs.
  - They work in any harness that can run a command, so no MCP server is
    involved.
  - Documented in the context of each non-Claude agent, never in
    `AGENTS.md`, so no model is told two ways to remember.
- **Claude's own memory is read live as a source.** Claude keeps writing
  where it always does, inside the workspace or outside it. UZE reads
  those facts into the shared index at each launch, never copying or
  redirecting them. Codex's consolidated summary, when it exists, is
  offered to other harnesses' agents as one entry they read on demand.
- **Machine-scope facts from agents wait for the operator.**
  - A launched agent's `--machine` fact is a *proposal* the workspace asks
    the operator about.
  - A fact that would reach every project must not be one prompt injection
    away.
- **The memory index is a section of the agent context**, ranked so
  standing guidance (`feedback`, `user`) survives the bound. On a degraded
  harness it is not sent, and status says so.
- **Recall without a service.** Everything stays deterministic: files,
  the launch's context and the verbs, with no LLM and no index on disk.
  - Memory takes the context room a briefing leaves, and every fact
    earns a line in rank order once every name is placed.
  - A project fact citing a path the repository no longer has is marked,
    so an agent reads it as history.
  - Words match without regard to accents, a match in a name outranks
    one in a body, and `show` follows `[[name]]` links both ways.
  - `remember` refuses credential shapes and names a near duplicate.
- **`uze status` lists memory**: where it lives, each fact with its type
  and provenance, proposals, and unreadable files.
- **A harness's own memory, kept or turned off per machine, unless it is how its agents write.**
  - Covers Codex `memories` today. Claude's memory is a source and is
    never turned off.
  - Applied only at UZE's launches.
  - The default is to keep it.

## Capabilities

### New Capabilities

- `agent-memory`: the memory UZE keeps. Covers its scopes, the fact
  document, the agent verbs, the operator's acceptance of machine facts,
  provenance, concurrent writers, the index section, and what status shows.

### Modified Capabilities

None. The index plugs into `agent-context-channel`, introduced by
`deliver-agent-context`, through that capability's own notion of a
section, so no requirement of it changes.

## Impact

- **Depends on** `deliver-agent-context` having landed. No memory reaches
  a model without it.
- **Persisted state:**
  - `state/memory/` and `state/projects/<id>/memory/`, both named in
    `UzeHome`, as records.
  - `memory.harness` in `config.toml`, authored.
- **Code:**
  - A `memory` module in `uze-core` (project concern). Front matter is
    read with `noyalib`'s serde, which already reads capability front
    matter in `uze-core`, and written by rendering the fixed fields; the
    `noyalib::cst` containment is untouched.
  - `uze-application`: the memory service and status read model.
  - `uze-application`: renders the index section and hands it to
    `continuity::plan` as data, so `uze-workspace` knows nothing of memory.
  - `src/cli/`: the grammar, the help region, `command_performance`.
  - `src/ui/`: the toast asking about a proposed machine fact, read on the
    orchestrator's read thread.
- **Dependencies:** none new.
