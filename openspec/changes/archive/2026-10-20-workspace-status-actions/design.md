## Context

See proposal.md (Why). The current state:

- **Delivery.** `landing::deliver` takes the write lock, rebases the slot onto the target and
  runs the gate. Then:
  - `merge` fast-forwards the target;
  - `pr` calls `publish`, which pushes under a derived name and either finds a request or
    returns the "open the request" message;
  - `handoff` stops.

  A conflict or a failed gate returns a fixed message (`conflict_message`,
  `gate_failure_message`).
- **Typing into the pane.** `submit_notice` + `notice_bytes` sanitize the text, paste it
  bracketed when asked, and end it with Enter. They check neither the agent's activity nor
  what the operator typed.
- **Reading the state.** `WorkStateView::Published` is derived, not recorded: `Ready` with
  `unsynced == Some(0)`, computed only when the completion is `pr`. The request number is found
  by `ls-remote refs/pull/*/head refs/merge-requests/*/head` against the published tip, at most
  every 60 s, also only in `pr`.
- **The chip.** It is `delivery_notification` + `task_mark`, drawn only for the selected tab's
  isolated agent. Its click is `WorkspaceHit::Deliver`. `ChipState` drives every control in
  the strip, and only `Static` is the chip's own.
- **What the client knows about a pane.** `agent_is_working(pane)`; `remembered.prompt_buffers`
  (one per pane, fed while focused, untrusted after Esc, Up/Down or a ctrl/alt chord, reset by
  Enter or Ctrl-C); and `note_agent_prompt_submission`, which marks a pane working at once.
- **The manifest.** The `workspace:` section is strict (`deny_unknown_fields`, and a bad key
  makes the evaluation `unreadable`). `artifacts` is read beside it.

## The split this change makes

UZE does what needs its write lock or protects an invariant:
- the rebase and the gate;
- the fast-forward of the target, which no agent may write;
- a push of exactly what passed the gate.

The agent does what comes after: the request, answering a review, summarizing. `delivery:`
says where UZE puts the work (`merge`: the local target; `push`: the remote). Rebasing and
gating are how UZE gets it there, never a mode. `actions:` says what the agent is asked at each
state, and no word reaches the agent from anywhere else.

## Goals / Non-Goals

**Goals:**
- One line between UZE's work and the agent's, the same in every project.
- One rule the operator can learn: a chip that answers a click looks different at rest, and
  what it answers with is the project's instruction.

**Non-Goals:**
- Actions on the upstream sync (`↓N ↑N`), which describes the operator's checkout.
- Placeholders (`{branch}`, the gate log path), per-harness skill invocation and built-in
  verbs. The agent is in the checkout and asks Git itself. The gate log path is the first
  placeholder worth adding if one ever is.
- Reading the forge through its API (checks, approvals, a request closed unmerged).
- Delivery progress steps (`rebase → gate`).

## Decisions

**D1. `delivery:` names where the work is put: `merge | push`, or undeclared.**
`CompletionBehavior` becomes `{Merge, Push}`, and the policy holds `Option<CompletionBehavior>`.
- `merge` puts the work into the local target, by fast-forward.
- `push` puts it on the remote as the task's branch, under its readable name, via today's
  `publish` minus the "open the request" message.
- Both rebase and gate first. That is how UZE gets the work there, so it is never a mode an
  operator chooses.
- Undeclared, there is no delivery. `deliver_task` and `deliver_ready` in `uze-application`
  refuse, so the key, recent and anything else that asks are refused alike.
  Not configured means nothing happens, the same as for `actions`.
- `pr` becomes `push`: the name promised a request that UZE no longer opens.
- `handoff` goes: "validate and stop" was the internal step exposed as a mode.
- Who pushes in `push`: UZE, right after the gate. The remote then only ever receives what the
  gate passed. With the agent pushing, a commit made after the gate would reach the request
  unchecked.
- `refresh` today passes `CompletionBehavior::Handoff` to `target_tip` to mean "the local
  target". That becomes an explicit `TargetSource::{Local, Remote}`, so no behavior value
  stands in for a target.
- `Delivered::Handoff` and `DeliveryOutcome::Handoff` go.
- `DeliveryPolicyView.completion` becomes optional.
- `set_completion` gains the ability to unset the key, so the policy popup can choose "no
  delivery".
- The spellings carry no aliases. The project is alpha and keeps no scattered compatibility,
  so `pr` and `handoff` fail with the strict section's error naming `merge | push`.

**D2. The states, and their precedence.**
- `in_review` holds only while nothing is left to push, and `delivered` only while the target
  holds all of the branch. New commits therefore always read `ready`.
- `WorkStateView` gains `InReview`. `Integrated` and `GateFailed` are shown as `delivered` and
  `failed`.
- When more than one holds, the furthest along wins:
  `delivered > in_review > published > ready`.
- `conflicted` and `failed` come from the record. `Running`, `Uncommitted`, `Shelved` and
  `Closed` draw no chip.
- Publication and the request lookup run whenever the branch is on the remote, whatever the
  behavior. A push is a fact whoever made it, including the agent in a `merge` project. The 60 s bound and
  "stop asking once answered" stay.
- The chip's subject no longer depends on the behavior: `#41` in review, otherwise
  `→ <target>`.

**D3. One kind of action: an instruction string.**
- `workspace.actions` maps a state to text, and its keys are the states of D2.
- Rejected: typed actions (`skill:`, `prompt:`, `open:`, `uze:`). Each kind is a branch in
  the schema, the parser, the renderer and the spec.
- Rejected: `skill:` in particular. It would carry vendor invocation syntax onto a click path
  in `src/`. "Use the git:pr skill to …" reaches every harness as written.

**D4. The keys name the moment, as `on_<event>`, one per state the agent acts in.**

| State | Key |
|---|---|
| uncommitted | `on_change` |
| published | `on_publish` |
| conflicted | `on_conflict` |
| failed | `on_fail` |

- Four, not one per chip state. From `ready` the next step is UZE's delivery (the key), from
  `in_review` a reviewer's, and from `delivered` nobody's, so a key there would mostly go
  undeclared. The file grows by additive keys, so adding one later breaks nothing, while
  shipping one and removing it would break every file that declared it.
- `uncommitted` gets a key although it is not a delivery state: it is the state an agent's
  work sits in most, and "commit it" is the request an operator otherwise types by hand. Its
  chip is drawn only when `on_change` is declared, beside the change count, which keeps
  reporting the changes and opening them on a click.

- The key says *when* and the text says *what*.
- The form is the familiar event-handler one: `on` plus the event in its base form, as in
  `onClick` or GitHub Actions' `on: push`.
- The handler name fits both ways in:
  - UZE's operation is the event;
  - a click on the chip asks for that moment's instruction again.
- Rejected: deed names such as `open_pr`. They bind a key to one deed and undo D3.
- Rejected: a bare `on:` block. YAML 1.1 reads `on` as `true`, the same trap GitHub Actions
  is known for.
- `ActionState` is its own enum in `uze-workspace`, so a state added to the view is never
  accepted by the manifest silently.

**D5. `actions` is read beside the policy, not inside it.**
- Like `artifacts`, it is removed before the strict parse. `declaration::actions(root)` then
  reads it leniently, returning the map and the entries it left out.
- The strict section protects policy: an unknown `delivery` read as a default is a policy
  silently not honoured.
- An unknown action key makes nothing happen and is said, so failing on it would empty the
  sidebar over a typo in a click target.
- The left-out entries reach the client as `Evaluation::ignored_actions`, said once per
  primary.

**D6. The view carries the instruction; the client never reads the manifest.**
- `AgentView` gains `action: Option<String>`.
- `from_agent` takes the actions map at its four call sites, and the instruction is resolved
  from the final drawn state.
- The client's local `Integrating` override drops it. A root agent gets `None`.

**D7. An operation of UZE's sends the instruction of the state it leaves.**

| UZE's operation | Resulting state | Instruction sent |
|---|---|---|
| `push` delivery, branch pushed | published | `on_publish` |
| delivery or automatic refresh, rebase stops | conflicted | `on_conflict` |
| delivery, gate fails | failed | `on_fail` |

- Every other way into a state sends nothing on its own: the agent's edits (`uncommitted`)
  and a push the agent made (`published`). Those are observations, and an agent typed at on
  each of its own edits would be interrupted mid-turn. Their instructions go by click.
- `on_conflict` and `on_fail` fall back to a line of UZE's own when undeclared
  (`ActionState::fallback`): in both UZE left the agent's checkout waiting on the agent, a
  rebase paused or a branch the gate refused, and an agent told nothing sits in front of it.
  The fallback is static text, so the target and the gate's output are not quoted; the
  checkout and the gate's log say both.
- `DeliveryOutcome::{ReturnedToAgent, AwaitingRequest}` and `Evaluation::notices` lose their
  `AgentNotice` and carry the resulting state. The outcome toasts become state words.

**D8. Sending: one predicate, refuse rather than queue, submitted with Enter.**
- The target is `pane_for_agent(task.id)`, found by launch stamp.
- `may_submit(pane)` answers:
  - `Busy` when the agent is working;
  - `Typed` when the pane's `PromptBuffer` is non-empty;
  - `Unknown` when the buffer is untrusted, which clears on the next Enter or Ctrl-C;
  - otherwise `Ok`.
- A click that is refused gets a toast. An automatic send that is refused gets a "held back"
  toast, and the chip's click is how to send it later.
- Nothing is queued. A queued instruction would land at a moment nobody chose, perhaps after
  its state changed, and the chip is the queue the operator can see.
- After a send, `note_agent_prompt_submission` marks the pane working at once, so a second
  click refuses as `Busy`.
- Submitted, not staged. The text was reviewed once, when it was declared, and a staged one
  would wait on a second gesture and collide with what the operator types next.
- The text goes through `notice_bytes` (control characters become spaces, bounded length,
  bracketed paste) and is not recorded in prompt history.
- Known limit: the guard reads only this client's buffer.

**D9. The region projected into `AGENTS.md` says the new split.**
The clause is rendered from `Option<CompletionBehavior>` in `WorktreePolicy::instructions`, so
the bullet's "delivery is UZE's" half agrees in every case:
- `push`: UZE rebases your branch onto the target, runs the project's checks and pushes it.
  Open a request only when the project's instruction asks you to.
- `merge`: as today.
- No delivery: the bullet reads "Never commit to, merge into, rebase, or reset the target
  branch: your branch is left for a person to integrate; commit on it and stop", with no
  claim that delivery is UZE's.

The bullet "if UZE tells you a rebase is paused" stays, since it is already conditional.

**D10. Affordance moves from ground to weight.**
- `ChipState` loses only `Static`. The status chip gets `StatusChip::{Report,
  Action { hovered, pressed }}`.
- Report: muted hue, regular weight, unchanged under the pointer.
- Action: full hue and bold. It brightens when hovered, and when pressed it inverts for a
  single frame.
- `StateWarningMuted` exists. Muted Published, Landed and InFlight are
  added to `token.rs`, `theme.schema.json` and the bundled themes.
- The extension group keeps its ground.

**D11. The hint and the toasts are bounded.**
- The hint lives in the message zone, as that zone already works: over the tab side's tail,
  never over a control. It names the key a click sends (`send on_publish`), and is dropped
  without room.
- Rejected after trying it: the start of the instruction cut at 30 columns, which named
  nothing since an instruction is a sentence; and a marquee, which takes ~20 s to show one
  and is the only thing in the strip that would move without work in flight.
- A `ready` chip, which never has an instruction, shows the deliver key, from
  `chord_for(Action::DeliverTask)`.
- Toasts: title at most 24 columns, detail at most 32.

| Outcome | Title | Detail |
|---|---|---|
| sent | `sent to agent` | start of the instruction |
| held back | `held back` | the reason |
| `Busy` | `agent is busy` | agent's label |
| `Typed` | `you have text typed` | agent's label |
| `Unknown` | `can't tell what's typed` | `press enter or ctrl-c` |

**D12. No consent step for instructions.** Setup and gate lines ask consent because UZE runs
them as shell. An instruction is text an agent reads, in the trust class of `AGENTS.md`, and it
is sanitized (D8).

## Candidate ADRs

- **`delivery:` names a destination, and UZE's work ends at writing it.** The request and
  everything after it belong to the agent, through declared instructions. This moves the
  boundary that the delivery model, the region text and every project's `agents.yaml` are
  built on, and it is costly to move back.

## Risks / Trade-offs

- [A `push` project with no instructions publishes branches nobody opens a request for] →
  intended: the strip says `published`, and the project opts in. The docs and this
  repository's `agents.yaml` show the `on_publish` instruction that reproduces today's `pr`
  flow.
- [A project that relied on `handoff` now has no delivery] → it loses only "rebase and gate
  without writing anything". The automatic rebase still follows the target, and the
  project's own checks remain runnable by the agent.
- [A stray click on ready work starts a delivery] → the chip is drawn as an action and its
  hint names the deliver key before the click; `push` writes only the agent's branch and
  `merge` only the local target.
- [`PromptBuffer` models the line and may be wrong] → untrusted refuses (`Unknown`). A false
  refusal costs a key, and a false send mixes with the operator's words.
- [Two binaries on one machine] → the installed release rejects `push` in this repository's
  `agents.yaml`. The release reads the section as unreadable, so the `agents.yaml` edit is the
  last task, and the release that ships this change is the one to adopt it.
- [Wider reach] → five journeys and the delivery tests click the chip or assume `pr`, and five
  doc comments call the chip a button. The tasks list each.
