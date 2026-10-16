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
- decided:  drop `AgentView::forge` and its per-listing `landing::forge` read, and the `Forge` re-export from `uze-application`; `uze-workspace` keeps `landing::forge` for its own tests.
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
- decided:  follow the pattern `05-delivery/05` already used for the conflict message: the instruction appearing in the pane (`in: pane`, the strip's hint excluded) is the gate, and the checks read Git and the task store — the click moved nothing and left the work ready (`01`), the push reached the remote (`03`), the gate failure left the target alone (`02`). Covered: the click (`on_ready`, `01`), and three automatic sends (`on_publish` `03`, `on_fail` `02`, `on_conflict` `05`).
- rejected: teaching the runner `AGENT_SCRIPTS` to read an inbox — runner work outside this change's scope, for a claim the pane already gates and `src/ui` tests already assert byte for byte.
- blast:    none; a later runner change can add an inbox check beside the gate.
