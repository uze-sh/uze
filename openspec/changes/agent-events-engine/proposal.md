> **Parked (2026-10-28).** Not scheduled: the radar example that needed it
> was removed, and an independent review (design.md, "Review before
> resuming") found the identity, delivery and `waiting` parts need Lab
> measurements first. Resume from that section.

## Why

An agent UZE launches tells the workspace what it does through a reporter
written per harness: Claude Code gets a per-session `--settings` of hooks
running `uze agent report`, Codex gets its `notify` (turn end only), and
OpenCode and Antigravity get nothing, so the workspace guesses their turns
from the pane. The extension contract then offers events only one harness
produces (`tool.*`, `approval.asked`), and `approval.resolved` that none
does: radar showed an agent awaiting approval long after it was answered.
UZE's promise is that what it offers works on every harness it supports;
its own agent events do not.

## What Changes

- UZE delivers **one hooks manifest of its own** through the portable hooks
  compiler (ADR-040), the way it delivers a plugin's: written by `uze
  setup`, receipt-owned, into each harness's native hook configuration.
  Its handlers run `uze agent report` only for an agent the workspace
  launched, from the portable fields every harness already gives.
- The per-harness reporters go: Claude Code's per-session `--settings`
  hooks and Codex's `notify`. **BREAKING** for nobody outside UZE: they
  are UZE's own launch arguments.
- The extension contract offers only **agent events every supported
  harness produces, or that the workspace derives the same way for all**:
  `session.started`, `turn.started`, `turn.finished`, `waiting` and
  `waiting.ended`. **BREAKING** for an extension declaring
  `agent.tool.started`, `agent.tool.finished`, `agent.approval.asked` or
  `agent.approval.resolved`: its manifest no longer compiles, and says
  which event replaces it.
- `waiting` is derived, not reported: a tool started and not finished
  while the agent's pane is quiet. Whether every harness allows it is
  measured in the Conformance Lab before it enters the contract.
- The workspace takes its agent state, the finished-turn chime and who is
  waiting from these events, falling back to the pane only where a report
  did not arrive.
- radar keeps only its bar item; its drawer of events goes, and a click
  on the item puts the agent that is waiting, or the longest-working one,
  in front.

## Capabilities

### New Capabilities

- `agent-events`: how an agent UZE launched reports what it does on every
  supported harness, which agent events the workspace and extensions get,
  and the rule that an event no harness can give stays out of the
  contract.

### Modified Capabilities

- `agent-finished-chime`: a finished turn is known from the agent's
  report on every harness, the pane's quiet only where none arrived.

## Impact

- `crates/uze-integrations`: each vertical's `reporter` and
  `reported_events` go from `IntegrationPort`; the UZE hooks manifest is
  compiled and delivered with the harness's existing hook target. The
  Conformance Lab gains a contract check per harness.
- `crates/uze-workspace/src/reports.rs`: the reportable events shrink to
  what the hooks give; `src/cli/agent.rs` reads the portable `HOOK_*`
  fields instead of each harness's payload.
- `crates/uze-contract`, `crates/uze-extensions`: `AgentEvent` loses the
  non-agnostic events and gains `waiting`/`waiting.ended`; the bridge
  (`src/ui/orchestrator/declared/bridge.rs`) derives them.
- `plugins/radar`, the gallery's bridge extensions, journey 23 and the
  docs that name the removed events.
- Supersedes the reporting half of the unarchived `extension-agent-bridge`
  change (its "reports through a journal" requirement stands; what writes
  the journal changes).
