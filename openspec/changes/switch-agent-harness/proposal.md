## Why

An agent is bound to the harness it was launched with. When that harness
stops being able to work — the most common case is a usage limit
(Claude's 429 with a reset time, Codex's `rate_limits` window) — the
operator's only way to keep going is to start a different agent beside it,
which begins in a stranger's position: no goal, no decisions, none of the
conversation that produced the commits it is now standing on. The record
UZE keeps already holds one conversation per harness for each agent, so
the agent's identity, checkout and branch could survive a change of
harness; nothing lets the operator ask for one, and nothing tells the
incoming harness what happened.

Showing that a harness hit its usage limit, and offering the switch
because of it, is left for a change of its own: limits are a subject
beyond this one.

## What Changes

- An agent tab's context menu gains **Continue in <harness>**, one item per
  detected harness other than the one the agent runs now. Choosing it
  relaunches the same agent (same identity, checkout, branch and label) on
  that harness, replacing the tab the way **To worktree** already does.
- The agent's recorded harness becomes the harness it runs **now**, not the
  one it was first launched with. The harnesses it has run on are the keys
  of its conversation record, so no history is stored twice.
- The previous harness's conversation is kept. Switching back resumes it.
- Every switch writes a **briefing**: a bounded, owner-only document outside
  every checkout, made from the agent's Git facts and, where the outgoing
  harness's integration can read its own transcript, the goal, the
  vendor's own compaction summary when there is one, the operator's turns,
  the last assistant messages and the last command run. Where the
  transcript cannot be read, the briefing is the Git facts alone.
- The incoming harness is started with one short first turn pointing at the
  briefing. The turn is given **once**: a later relaunch of the same agent,
  including the terminal runtime restoring a workspace, carries no prompt.
  When the incoming harness resumes a conversation it had before, the
  briefing covers only what happened since it left.
- Switching an agent that is mid-turn asks for confirmation instead of
  being refused, because relaunching ends what its process is doing.

## Capabilities

### New Capabilities
- `agent-harness-switch`: relaunching an agent on a different harness
  while keeping its identity, checkout and every harness's conversation;
  the briefing written on each switch, its contents, bounds and placement;
  the one-shot first turn that points at it.

### Modified Capabilities
- `agent-session-continuity`: the record's harness of an agent may change
  across launches; the one place UZE writes conversation text is the
  briefing, never the continuity record; a launch can carry a pending
  briefing turn that the shim gives once and clears.

## Impact

- `crates/uze-core/src/delivery/integration.rs`: new port verbs on the
  workspace half of `IntegrationPort` (read an agent's turns, arguments
  that carry a first turn), each defaulting to "not supported".
- `crates/uze-integrations`: Claude and Codex implement them from their
  transcripts; OpenCode and Antigravity implement the first-turn
  arguments only.
- `crates/uze-workspace`: `continuity`, `conversation` (pending briefing
  marker), `task` (meaning of `Agent.harness`), a briefing module.
- `crates/uze-application`: a `switch_harness` workspace operation
  answering a placement.
- `src/shim.rs`: the launch plan carries trailing first-turn arguments.
- `src/ui/orchestrator`: menu items, a `Switch` placement request, the
  the mid-turn confirmation.
- `UzeHome`: one generated-tier path for briefings.
- `conformance/` and `journeys/`: a contract check per harness and one
  journey.
