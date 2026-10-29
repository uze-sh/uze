## Why

The harness switch (`switch-agent-harness`, #258) tells the incoming
harness what happened, but in plain sight. The operator watches a prompt
they never typed appear in the pane ("Read
`~/.uze/runtime/.../briefings/<agent>.md` first, then check `git log`…").
Then they watch the agent open that file and reason about it before it
does anything they asked for. The project's own context (`AGENTS.md`)
already reaches every harness without anyone seeing it handed over. What
UZE has to tell one agent should arrive the same way.

That design (D2) rejected system-prompt flags for good reasons, and those
reasons still hold:

- `--append-system-prompt-file` and `developer_instructions` are frozen
  into the conversation on resume.
- The Codex key overrides the operator's own.
- Argv is visible in `/proc`.

What changed since then is that the Lab has measured launch-scoped
channels that avoid all three. `track-touched-repositories` measured
per-launch hooks on all four harnesses, and each harness has a context
path that is read again on every launch.

This change is also the delivery path `portable-agent-memory` needs. It
ships first, on its own merit, so memory lands on a channel already
proven.

## What Changes

- **An agent context, composed for each launch UZE owns.**
  - A generated, byte-bounded document per agent, made of sections.
  - The briefing is the first section. Memory's index will be the second,
    in its own change.
  - Nothing is composed when no section applies.
- **One channel per harness, declared by its integration.** Each puts the
  context before the model the way `AGENTS.md` is, never shown as a turn,
  with only a path in argv:
  - Claude: a per-agent `--add-dir` directory holding a generated
    `CLAUDE.md`.
  - Codex: a launch-scoped `SessionStart` hook given with `-c`, trusted
    for that process only.
  - OpenCode: a UZE-owned plugin's `context` hook, which finds the agent
    by the session id UZE assigned.
  - Antigravity: a launch-scoped `PreInvocation` hook in a per-agent
    `--add-dir`.

  Each channel is measured in the Lab before it is declared. One that
  cannot be proven invisible is declared *degraded*, and status says so.
- **The switch briefs through the context.**
  - The visible first turn goes, and the incoming harness opens waiting
    for the operator's next turn with the briefing already in hand.
  - A degraded harness keeps today's first turn as its fallback.
- **The briefing becomes a record.**
  - It is given on every launch on its harness until the agent commits,
    and one channel (OpenCode) keeps no copy in the conversation.
  - It is retired at the agent's first commit after the switch, or by the
    next switch.
  - Once nothing else holds it, it moves from `runtime/` into the agent's
    continuity record.

## Capabilities

### New Capabilities

- `agent-context-channel`: covers
  - the per-launch agent context and its sections and byte bound;
  - the invisible, resume-holding channel each integration declares;
  - that only UZE's launches get one;
  - degraded reporting;
  - the four-outcome Lab contract.

### Modified Capabilities

- `agent-harness-switch`: the incoming harness is briefed through the
  agent context, and the pointed-at first turn becomes the degraded
  fallback. This capability is introduced by the not-yet-archived
  `switch-agent-harness`, which is archived first.
- `agent-session-continuity`: the briefing is kept in the record and given
  until the work moves on, no longer given once from a generated file.

## Impact

- **Code:**
  - `uze-core`: an `IntegrationPort` channel declaration, and `UzeHome`
    paths for the agent context directory and the OpenCode session index.
  - `uze-workspace`: composing the context in `continuity::plan`, the
    briefing moving into the record and retiring on the first commit, and
    `LaunchPlan` growing environment.
  - `uze-integrations`: four channels, and the OpenCode context plugin
    delivered by setup.
  - `src/shim.rs`: carrying the plan's environment and clearing the
    context path on a launch it does not own.
- **Shared with `track-touched-repositories`:** the launch-scoped hook
  contribution (`-c hooks.*` and Codex self-trust, Antigravity
  `--add-dir/.agents/hooks.json`). Whichever change lands first builds it,
  and the other uses it.
- **Conformance:**
  - Synthetic providers record *where* a marker lands (system, developer,
    or user part) as evidence.
  - A new `agent_context` contract replaces the briefing first-turn
    checks.
- **Persisted state:** the continuity record changes shape, with one
  ladder step that drops a pending path entry.
- **Dependencies:** none new.
