## D1 — One action, many entries: the menu carries the harness beside the action

- status:   pending
- task:     3.1
- commit:
- trigger:  `uze_keys::Action` is a fixed enum (a keymap ABI), and the harnesses a machine has are not something it can name.
- decided:  one `Action::SwitchHarness` (no chord, beside `isolate-agent`), inserted once per set-up harness; `ContextMenu` carries a `choices` vector parallel to its items, and activation passes the choice with the action.
- rejected: one variant per harness (names vendors in `uze-keys` and the binary); a submenu or the agent picker in a "switch" mode (one more step, and the spec asks for one entry per harness).
- blast:    menu activation's two call sites and `ContextMenu`'s constructors.

## D2 — Ask before interrupting with a one-entry menu, not a modal

- status:   overturned (operator: a global confirmation dialog; `interrupt.rs`, shaped like `approval.rs`)
- task:     3.3
- commit:
- trigger:  the spec asks for confirmation on a working agent; the workspace has no generic confirm dialog for tab actions.
- decided:  the first activation on a `Working` tab replaces the menu with one entry, "Interrupt it, continue in <harness>"; anything else dismisses it, the rule every menu already follows.
- rejected: an extension-style confirm overlay (a new surface for one question); refusing a busy switch (the spec says ask, not refuse).
- blast:    `ContextMenu::interrupting` and `HarnessChoice::interrupting`.

## D3 — Say "no first turn" as a placement warning, not a client field

- status:   overturned (operator: no notice at all; every built-in harness takes a first turn)
- task:     3.4
- commit:
- trigger:  the client never learns the briefing's path; adding it to `AgentPlacement` touches every placement.
- decided:  `switch_harness` adds a warning naming the path when the incoming integration's `first_turn_args` is empty; `absorb_placement` already shows warnings as toasts.
- rejected: `AgentIdentity::takes_briefing` plus a path on `AgentPlacement` (two new fields for a case no built-in harness has today).
- blast:    one warning string.

## D4 — The limit mark is words in the warning hue, not a new symbol and token

- status:   overturned (operator: usage limits are left for a change of their own; the feature was removed)
- task:     5.5
- commit:
- trigger:  design D8 said "a new `theme` symbol and token"; AGENTS.md says to add one only when none says what yours means.
- decided:  the caption row reads `claude · limit 3h` in `Token::StateWarning`, the same treatment as "checkout removed".
- rejected: `Symbol::MarkLimited` and `Token::StateLimited` (a second warning hue a theme author has to set, meaning the same thing); the full "resets in 3h" (truncated at sidebar width).
- blast:    `limit_caption` in `render/sidebar.rs`.

## D5 — A briefing's directory is marked like a projection's

- status:   accepted
- task:     1.5
- commit:
- trigger:  `prune_projections` removes any `runtime/projects/<id>/` without a `project.json` marker, and a project UZE never projected into has none, so a briefing could be swept before it was read.
- decided:  `harness_runtime::prepare_project_dir` (split out of `prepare_projection`) writes the marker; `briefing::write` calls it.
- rejected: a tenant of its own under `runtime/` (one more sweep rule for the same lifetime: the project's root).
- blast:    `prepare_projection` now delegates; behaviour unchanged.

## D6 — `resume_task` takes the harness it launches

- status:   accepted
- task:     2.1
- commit:
- trigger:  the existing drift (a resume on another harness kept the old one) has no harness to record without a parameter.
- decided:  `resume_task(cwd, id, harness, occupied)`, recorded by `keyed_on` in the same write that issues the launch key; `PlacementRequest::Resume` carries it.
- rejected: an optional harness (every caller has one).
- blast:    five test call sites and the one client call.

## D7 — The Lab checks and the journey wait for hand validation

- status:   accepted
- task:     4.2, 5.6, 6.3
- commit:
- trigger:  the operator's standing rule: journeys (and, by the same reasoning, real-harness Lab runs) come after the feature was validated by hand, not during iteration.
- decided:  implemented and unit-tested every other item; those three stay unchecked until the operator has tried the switch.
- rejected: writing the Lab scenes and journey now and iterating them against a feature that may still change shape.
- blast:    none; they are additive.
