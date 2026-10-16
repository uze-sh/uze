## D1 — Leave the policy popup alone: there is none that edits `delivery`

- status:   pending
- task:     1.5
- commit:
- trigger:  1.5 asked to "update the policy popup's choices", but no surface in `src/` calls `set_completion`; the client only reads `delivery_policy` for the target.
- decided:  `set_completion` takes `Option<CompletionBehavior>` and unsets the key through a new `manifest::unset_scalar`, so a future popup can offer "no delivery"; nothing in the UI changed.
- rejected: adding a popup in this change — out of scope, and no requirement asks for one.
- blast:    none; the API is exercised by `declaration` tests only.

## D2 — Drop the status's pressed inversion

- status:   pending
- task:     5.1
- commit:
- trigger:  design D10 had an action invert for the frame a press lasts, but the spec says no status in the strip's zones is set apart by a ground of its own.
- decided:  `StatusKind::Action { hovered }` only; a press paints nothing, and its answer is the toast ("sent to agent", or the refusal) and the state the work moves to.
- rejected: a one-frame inverted ground — it contradicts the requirement it was meant to illustrate.
- blast:    trivial; one match arm.

## D3 — Ask the remote about every task, so a branch the remote no longer holds keeps no request number

- status:   pending
- task:     2.2
- commit:
- trigger:  running the request lookup in every project (not only `pr`) means `adopt_request` now also drops the number of a task whose branch is gone from the remote — typically a request merged and its branch deleted.
- decided:  accept it: `in_review` needs the branch on the remote anyway, and `delivered` reads `→ <target>`, which is true. The test `work_after_a_delivery_forgets_the_delivered_request` no longer asserts the number survives the first pass.
- rejected: keeping a number for a branch that is gone — it would name a request for work the remote no longer carries.
- blast:    a delivered task's chip shows `→ main` instead of `#41`.

## D4 — Remove `Forge` from the read model

- status:   pending
- task:     2.3
- commit:
- trigger:  with the chip's subject `#<n>` or `→ <target>` whatever the behavior, nothing in the client used `AgentView::forge` any more, and the only toast that named the forge's word ("agent opening the PR") went with the fixed messages.
- decided:  drop `AgentView::forge` and its per-listing `landing::forge` read, the `Forge` re-export from `uze-application`, and the `landing::forge` module itself, which no production code reads any more.
- rejected: keeping it unused — a field nobody reads is a Git read per listing for nothing.
- blast:    a later surface wanting "PR"/"MR" re-adds one field and one read.

## D5 — Lift `state.published` in the two themes where its muted form failed contrast

- status:   pending
- task:     5.1
- commit:
- trigger:  the new muted tokens derive as `@state.<x>/c0`, and in `dracula` (`#6272a4`, 3.0:1 at full strength) and `tokyo-night-light` (`#707280`) the published hue was already at the 3:1 floor, so its muted form failed the palette check.
- decided:  lift `state.published` to `#8b9bd6` in `dracula` (5.3:1, muted 3.7:1) and `#5a5d70` in `tokyo-night-light` (5.3:1, muted 3.2:1); `tokyo-night-light` keeps `state.in-flight-muted` at `/e0`, the one hue whose `/c0` falls short there.
- rejected: muted equal to the full hue in those themes — it would leave weight alone to tell an action from a report.
- blast:    the published mark in the sidebar and recent is brighter in those two themes.

## D6 — Gate the journeys' instruction claims on the agent's pane, and check the machine for what the gesture did not do

- status:   pending
- task:     6.2
- commit:
- trigger:  6.2 asked to check the instruction "against the pane process's input", but the journeys' harness stand-in only echoes what it is sent; the scripted agent that writes an `.inbox` needs `AGENT_SCRIPTS`, which the journey runner does not provide.
- decided:  follow the pattern `05-delivery/05` already used for the conflict message: the instruction appearing in the pane (`in: pane`, the strip's hint excluded) is the gate, and the checks read Git and the task store — the click moved nothing and left the work uncommitted (`01`), the push reached the remote (`03`), the gate failure left the target alone (`02`). Covered: the click (`on_change`, `01`), and three automatic sends (`on_publish` `03`, `on_fail` `02`, `on_conflict` `05`).
- rejected: teaching the runner `AGENT_SCRIPTS` to read an inbox — runner work outside this change's scope, for a claim the pane already gates and `src/ui` tests already assert byte for byte.
- blast:    none; a later runner change can add an inbox check beside the gate.

## D7 — Four keys, and fallbacks for a conflict and a failed gate

- status:   accepted
- task:     7.1
- commit:
- trigger:  used by hand, the uncommitted state (the commonest moment to ask the agent something) had no key, while `on_ready`, `on_review` and `on_deliver` had little to ask: the next step from each is UZE's delivery, a reviewer or nobody. With the fixed messages gone, a project declaring no `on_conflict`/`on_fail` left the agent unaware of a paused rebase or a refused branch.
- decided:  keys `on_change`, `on_publish`, `on_conflict`, `on_fail`; an `uncommitted` status is drawn beside the change count only when `on_change` is declared, since the count already reports the changes and opens them on a click; `on_conflict` and `on_fail` fall back to UZE's own static line when undeclared.
- rejected: a dropdown of several actions per state (commit / open PR) — a second interaction model, and an "open PR" offered over uncommitted work would push without it; making the change count the action — it already opens the changes.
- blast:    `on_ready`, `on_review` and `on_deliver` are reported as unknown keys; adding one back later is additive.

## D8 — Cut the status's latency in this change

- status:   accepted
- task:     8.1
- commit:
- trigger:  by hand the status lagged its work. The journal showed three causes: states read only when a pane went quiet or every 20 s; every pass waiting ~2.2 s on a `git ls-remote` once a minute, made a per-project cost by D3; and an evaluation holding the tasks document while waiting 60 s for the write lock a delivery held through a 90 s `pre-push`, which held the delivery's own outcome behind it (`time.busy=94.0s`).
- decided:  the follow-the-target rebase never waits for the write lock; the remote's question is answered after the states are drawn; a `stat`-only stamp per checkout, polled each second, evaluates a repository the moment Git records something in it.
- rejected: a file-system watcher crate (`notify`) — a new dependency for what a few `stat`s a second already answer; a shorter refresh clock — a full Git pass every few seconds for every repository, changed or not.
- blast:    an edit not yet added to the index is still seen when the pane goes quiet or on the clock, as before.

## D9 — A click on ready work delivers it

- status:   accepted
- task:     7.6
- commit:
- trigger:  by hand, the operator clicked `→ main ↑4` under its `deliver alt+i` hint and nothing happened. The click was taken from delivery so that ready could carry an instruction without the click meaning two things; D7 took `on_ready` away, and with it the ambiguity.
- decided:  ready work is always an action; a click runs the deliver key's action; the hint stays `deliver <chord>`. Every bold status now does something on a click, and every muted one nothing.
- rejected: keeping the click inert — a status drawn under a hint naming an action, that does not take it, is the unpredictable click this change set out to remove.
- blast:    a stray click delivers; `push` writes only the agent's branch, `merge` only the local target, and "deliver all" stays on its key.

## D10 — The target's pull count pulls

- status:   accepted
- task:     8.4
- commit:
- trigger:  by hand, the operator expected the `↓9` beside the change count to pull, as an editor's sync count does; it only reported.
- decided:  a click runs `landing::pull_target`, the clock's own fast-forward without its one restraint (never moving a target the operator stands on), since the operator asked. Diverged, unpublished or blocked by uncommitted changes, it is refused with the reason in a toast. The push count stays a report: a push from the operator's checkout is theirs, outside what UZE delivers.
- rejected: a merge or rebase pull — a click that can rewrite or merge the operator's own commits is not a click.
- blast:    none beyond the operator's own target moving forward, which is what they asked for.

## D10 — The target's pull count pulls

- status:   accepted
- task:     8.4
- commit:
- trigger:  by hand, the operator expected the `↓9` beside the change count to pull, as an editor's sync count does; it only reported.
- decided:  a click runs `landing::pull_target`, the clock's own fast-forward without its one restraint (never moving a target the operator stands on), since the operator asked. Diverged, unpublished or blocked by uncommitted changes, it is refused with the reason in a toast. The push count stays a report.
- rejected: a merge or rebase pull — a click that can merge or rewrite the operator's own commits is not a click.
- blast:    the operator's own target moves forward when they ask.
