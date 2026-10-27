## Why

The extensions a plugin declares can draw beside an agent, but they know
nothing about it:
- whether its turn is running;
- which tool it just called;
- whether it waits for an approval.

They also cannot hand it anything: the next step of a plan, a review, a
fresh start. The gallery needs exactly that:
- `agents-hud` and `flightdeck` show what every agent is doing;
- `chimes` sounds when a turn ends;
- `jump` focuses an agent from a picker;
- `gsd-band` and `plan-review` give the agent its next prompt.

This is the second half of change 6 of `extension-platform`'s split
(D16), after `extension-slots-and-feeds`. A pane may now *report about
itself* through a verified, bounded journal. The workspace turns those
reports into `agent.*` events and an `agents` source. An extension may then
prompt an agent, only through the path UZE already uses to type into a
pane, refused rather than queued, and rate-limited by the host.

## What Changes

- `uze agent report <event> …`, verified by the agent's launch key, appends
  to a bounded per-agent journal under `UzeHome`, which the client's
  one-second watch reads.
- Each integration delivers the reporter with the launch, never as a
  portable hook. Each integration also says which events its harness can
  report; the paint heuristic covers turns where it cannot.
- Normalized events, with ids, times and outcomes, and with tool names and
  checkout-relative paths scrubbed:
  - `agent.session.started`
  - `agent.turn.started` and `agent.turn.finished`
  - `agent.tool.started` and `agent.tool.finished`
  - `agent.approval.asked` and `agent.approval.resolved`
- `refresh-on "agent.turn.finished"`, and the other agent events, reach
  sources like checkout events do.
- The `agents` source is answered by the workspace from its activity model,
  across open spaces. Reading it, or subscribing to agent events, needs
  `agent:observe`.
- The actions `agent.prompt` (a text, or a skill the integration spells),
  `agent.restart` (always confirmed) and `focus-agent`. Each names its
  `target`: `self` or an agent id from the `agents` source.
- The grants:
  - `agent:prompt`: from a gesture, after the exact text was shown;
  - `agent:prompt:auto`: per project and agent, for 24 hours, fixed text or
    a closed set only, revoked by any digest change;
  - `agent:restart`.
- A write goes through `submit_notice` behind `may_submit`:
  - it is refused unless the agent is ready, in the foreground and with no
    approval pending, and it is never queued (ADR-059);
  - it is limited per extension and agent, and per agent;
  - it is attributed;
  - the host's pause key stops it.
- A global key may open an overlay, for `jump`.
- ADR-056, ADR-040 and ADR-059 are amended, each in the commit that creates
  the fact it describes.

## Capabilities

### New Capabilities
- `extension-agent-bridge`: how an agent reports, what the workspace makes
  of it, and the `agents` source.
- `extension-agent-actions`: prompting, restarting and focusing an agent
  from an extension, and the limits on it.

### Modified Capabilities
- None in `openspec/specs/`. The integrations' delivery grows per launch,
  and the consent screen names the agent grants.

## Impact

- `uze-core`:
  - `UzeHome` names the journal;
  - `capability::hook` gains the turn-start and approval events;
  - `IntegrationPort` gains a method that spells a skill invocation.
- `uze-integrations`: the four integrations' `runtime_contribution`.
- `uze-contract`:
  - the agent events;
  - `SourceKind::Agents`;
  - `Does::{AgentPrompt, AgentRestart, FocusAgent}`;
  - `Grant::{AgentObserve, AgentPrompt, AgentPromptAuto, AgentRestart}`.
- `uze-workspace`: the journal reader and the `agent:prompt:auto` record.
- `uze-extensions`: the engine answers `agents` and routes agent events.
- `src/`:
  - `uze agent report`;
  - the client's watch of journals;
  - `submit_notice` from an extension, with its rates;
  - the pause key;
  - global keys.
- Docs: ADR-056, ADR-040 and ADR-059 amended; the extensions rule; the
  invariants; the `uze:extension` skill.
- The gallery's `agents-hud`, `flightdeck`, `chimes`, `jump` and
  `plan-review`.
