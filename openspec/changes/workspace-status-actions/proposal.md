## Why

The delivery chip in the workspace's tab strip mixes two jobs. It reports where the work
stands, and in some states it is also the button that rebases, runs the gate and pushes. Which
states answer a click is told only by a raised or recessed ground that a terminal barely
shows, so the operator cannot predict a click.

Underneath that sits a blurrier interface.
- Three completion behaviors (`handoff`, `merge`, `pr`) mix where the work ends up with what
  UZE says to the agent afterwards.
- Three messages to the agent are hard-coded (a conflict, a failed gate, "open the request"),
  and every project gets the same words.

## What Changes

- **BREAKING:** `delivery:` says only where UZE puts the work.
  - `merge`: into the local target.
  - `push`: onto the remote, as the task's branch.

  Both rebase onto the target and run the gate first, which is internal to delivery and never
  a mode of its own.
  - `pr` becomes `push`, and `handoff` goes.
  - With no `delivery:` declared, there is no delivery: the deliver key says so.
- **UZE does only what needs its lock or an invariant:** the rebase, the gate, the
  fast-forward of the target that no agent may write, and a push of exactly what passed the
  gate. What comes after (a request, a review, a summary) is the agent's, asked through the
  project's instructions.
- **A request is a state of its own:** `in_review` when the forge has a request for the
  branch, apart from `published`, a branch on the remote with none.
- **The chip is feedback only.** It is read from the checkout and the remote, the same
  whoever acted.
- **A project may declare `workspace.actions` in `agents.yaml`:** one plain-text instruction
  per state the agent acts in: `on_change` (uncommitted), `on_publish`, `on_conflict`,
  `on_fail`. Ready, in review and delivered take none: the next step from each is the
  delivery, a reviewer or nobody.
  - An instruction is sent, ended with Enter, when the operator clicks that state's chip.
    Uncommitted work shows a chip only when `on_change` is declared, beside the change count.
  - It is also sent when a delivery or an automatic rebase leaves the work in that state: a
    push sends `on_publish`, a conflict `on_conflict`, a failed gate `on_fail`.
  - With nothing declared, the state is only reported and nothing reaches the agent, except
    a conflict and a failed gate, which fall back to a line of UZE's own: in both the agent's
    checkout is waiting on the agent.
- **BREAKING:** the fixed `pr` message ("open the request") goes, and the conflict and gate
  messages become fallbacks a project may replace. A project keeps today's `pr` flow by
  declaring `on_publish: open a pull request for this branch…`.
- **One kind of action.** A skill is asked for in the instruction's own words. There is no
  per-harness syntax, no built-in verbs and no placeholders.
- **Nothing interrupts.** An instruction is never sent into a pane where the agent is working
  or the operator has typed (or UZE cannot tell). It is refused with a short reason and never
  queued.
- **Affordance by weight and hue.** A chip with an instruction is bold in its full hue and
  brightens under the pointer, and one without never reacts. No ground.
- A click on ready work still delivers, and only there: ready takes no instruction, so the
  click has one meaning, and its hint names the deliver key (`deliver-task`).

## Capabilities

### New Capabilities
- `status-actions`: the work-state chip as feedback, the states it reads, and the per-state
  instructions a project may declare.

### Modified Capabilities
- `worktree-policy`: `delivery:` becomes `merge | push` or undeclared; delivery stops asking
  for a request; a failed gate and a conflict send the project's instruction, or UZE's fallback.
- `agent-isolation`: delivery is offered for an isolated agent only when the project declares a
  completion behavior.

## Impact

- `uze-workspace`:
  - `CompletionBehavior` becomes `{Merge, Push}`, optional in the policy.
  - `publish` stops producing the "open the request" message.
  - `actions` is read beside the strict `WorktreePolicy`, as `artifacts` is.
  - The three message builders in `landing` go.
- `uze-application`:
  - `WorkStateView` gains `InReview`, derived from the record and the remote, as
    `Published` is.
  - `AgentView` carries the state's instruction.
- `src/ui/orchestrator`:
  - The chip is drawn from the state and the instruction.
  - A click submits the instruction, and delivery outcomes send theirs.
  - The `Deliver` hit leaves the strip.
- `src/ui/widget/chip.rs`: `ChipState::Static` goes, replaced by a status-chip style.
- Elsewhere:
  - The `05-delivery` journeys move to the deliver key, and their `pr`/`handoff` worlds move
    to `push`, `merge` or no delivery.
  - This repository's `agents.yaml` moves to `push` and declares the instructions that
    reproduce today's flow.
  - The projected `AGENTS.md` region changes with it.
