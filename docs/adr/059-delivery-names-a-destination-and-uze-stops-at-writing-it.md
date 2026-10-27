# Delivery names a destination, and UZE stops at writing it

Status: Accepted

## Context

An isolated agent's work reached its target through one of three
completion behaviors, `handoff`, `merge` and `pr`, and each mixed two
questions. One was where the work ends up. The other was what UZE tells the
agent afterwards. `pr` both pushed the branch and sent a fixed "open the
request" message. A conflict and a failed gate each sent a fixed message
too, the same words in every project. Whatever came after a push (a
request, a review, a summary) was UZE's script, not the project's.

## Decision

**`delivery:` names only where UZE puts the work, and UZE's part ends once
it has written it there.**

- `merge` fast-forwards the local target, and `push` publishes the task's
  branch. Both rebase onto the target and run the gate first, as part of
  delivery and never as a mode of their own. With no `delivery:` declared,
  there is no delivery.
- UZE does only what needs its lock or an invariant: the rebase, the gate,
  the fast-forward of a target no agent may write, and a push of exactly
  what passed the gate.
- Everything after that is the agent's, asked for through the project's
  own instructions. `workspace.actions` in `agents.yaml` declares one
  plain-text instruction per state the agent acts in: `on_change`,
  `on_publish`, `on_conflict` and `on_fail`. It is sent when the operator
  clicks that state, or when a delivery leaves the work in it. A conflict
  and a failed gate fall back to one line of UZE's own when undeclared.
- An instruction is never sent into a pane where the agent is working or
  the operator has typed, and it is never queued.

Amended (agent-bridge): refuse, never queue, holds for every writer into a
pane, extensions included. An extension's `agent-prompt` is shown to the
operator first, then goes through `submit_notice` behind `may_submit`; it
is refused with the reason while the pane is busy, typed into, held by
another program or awaiting an approval, and past the bridge's rates, and
it is never queued: the extension may try again on a later event
(`add-extension-platform` D16). The session's two writers, the project's
instructions and the bridge, are the only callers of `submit_notice`.

The rejected alternative was keeping `pr` as a behavior with a configurable
message. That still makes UZE the author of a step that belongs to the
project's own review flow, and every forge or habit would need a new
behavior.

## Consequences

The boundary between UZE and the agent is one line: UZE writes the work to
a branch or a target, and the project says in its own words what happens
next. A project that relied on `pr` keeps the same flow by declaring
`on_publish`. One that relied on `handoff` loses only "rebase and gate
without writing anything".

It is breaking for `agents.yaml`: `pr` and `handoff` are no longer read,
and a release older than this change reads `push` as an unreadable
section. A request is now a state of its own, `in_review`, read from the
forge like `published`, so the status shows what the remote holds whoever
opened the request.

Source change: openspec/changes/archive/2026-10-20-workspace-status-actions/
