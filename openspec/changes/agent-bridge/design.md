## Context

This change carries out D16 of `add-extension-platform/design.md` (the agent
bridge), with the rates decided in its *Product decisions owed* (0.2.4) and
the amendments of 0.9 and R-A1. It is the second half of change 6, after
`extension-slots-and-feeds` (whose D1 records the split).

What exists:
- **The launch key.** Every agent UZE launches carries `UZE_AGENT` and
  `UZE_AGENT_KEY`, and the workspace verifies a claim by them (`uze agent
  work name`, `Claim { id, key, cwd }`).
- **The activity model.** The client's activity model
  (`src/ui/orchestrator/agents.rs`) knows each pane's state from its paint,
  and `may_submit` says whether UZE may type into a pane (Ready, Busy,
  Typed, Unknown).
- **The hook vocabulary.** The portable hook vocabulary
  (`capability/hook.rs`) has `PreToolUse`, `PostToolUse`, `Stop` and
  `SessionStart`.
- **The launch contribution.** `IntegrationPort::runtime_contribution`
  adds args and environment to one launch.

## Goals / Non-Goals

**Goals:**
- `agents-hud`: a status row of every agent's state and turn time.
- `flightdeck`: a view of tool calls delivered `each`.
- `chimes`: a sound when a turn finishes.
- `jump`: an overlay picker on a global key that focuses an agent.
- `plan-review`: a review prompt sent on a gesture.
- `gsd-band`'s next-step button.

**Non-Goals:**
- Answering an approval. The bridge reports; it never decides.
- `agent.interrupt`: no integration has the concept.
- Pipes.

## Decisions

### B1. The journal is generated state, one file per agent, bounded

`runtime/agents/<id>/reports.jsonl` holds at most 256 lines or 64 KiB, and
the oldest half is dropped on overflow. It lives in the generated tier
because losing it costs nothing: a report is a claim about now. The client
reads only what was appended since its last read, by offset.

### B2. Reports are claims, never permissions

Any process in the pane inherits the key, so a report can only move what
is drawn and which sources are asked again. No write, copy or path opening
is released by one, and `agent:prompt:auto` counts its turns by the
bridge's report or the paint heuristic's, never by an event alone.

### B3. Each integration delivers what its harness can say

- **Claude Code**: a `--settings` JSON of hooks running `uze agent report`.
- **Codex**: its `notify` program for the turn's end.
- **OpenCode and Antigravity**: what their plugin and sidecar surfaces
  allow.

Each lists what it cannot report, and the paint heuristic covers turns.
The reporter fails open and exits 0 whatever happens.

### B4. The `agents` source and agent events reach the engine through the contract

`contract::Extension` gains `agents(&mut self, agents: &Value)`, called by
the host each pass with the activity model's agents for an extension that
holds `agent:observe`. The engine answers `agents` sources itself, as it
answers `events`. Agent events join `EventName` and arrive through
`Pass::happened`.

### B5. Writing to an agent is the host's, through `submit_notice`

An `agent.prompt` action resolves to text the host shows first:
- the text itself;
- the agent;
- the extension.

It is written through `submit_notice` only when `may_submit` is Ready, the
harness is the pane's foreground process and no approval is pending.
Otherwise it is refused with the reason and never queued.

The rates, per (extension, agent):
- one write per turn;
- one write per 60 s.

The rates, per agent:
- 3 writes per minute;
- 20 writes per hour;
- 10 s between unattended writes.

The excess is dropped with an attributed notice. The pause key resets every
counter and holds every write until it is pressed again.

## Risks / Trade-offs

- [A harness reports nothing] → the paint heuristic gives turns, and the
  Extensions screen says which events that agent cannot report.
- [A prompt reaches a harness's command syntax] → the strict filter of D16
  applies: no control characters, and no leading `!`, `/`, `#` or `@`
  unless it is the integration's own spelling of a skill.

## Migration Plan

Nothing persisted changes shape. The journal is new, generated state, and
the `agent:prompt:auto` acceptances are a new record beside the consents.

## Open Questions

None. This change records what it decides in `decisions.md`.
