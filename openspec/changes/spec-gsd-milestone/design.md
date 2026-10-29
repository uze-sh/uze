## Context

The spec surface reads every tool through `dialect::Dialect` tables. GSD's
table already lists phases (`.planning/phases/<NN-name>/`) and counts a
plan done by its `SUMMARY.md`. Extensions describe and never draw, read
only through `Host`, and answer an action with a `SpecOutcome` the host
carries out. The host knows the focused agent's harness only through its
integration; `uze-extensions` and the UI must not name a vendor
(`cli_and_tui_never_name_a_vendor_harness`).

## Goals / Non-Goals

**Goals:**
- Show GSD's open milestone, its phases and waves, and the next step.
- Hand a step to the agent in one key, without sending it.

**Non-Goals:**
- Running a step, or submitting one: the operator presses Enter.
- Live execution (which plan an executor is on, its last tool, elapsed
  time), a checkpoint band, a status-line segment. The host has no event
  at tool-call granularity, and host chrome is not drawable (#287).
- Workspaces spanning sibling repositories.
- Writing `.planning/`.

## Decisions

**The milestone is GSD's own reader, not a table.** A dialect is data so a
new tool needs no new screen. A milestone with phases, waves and a
recommended next step exists only in GSD; abstracting it into roles would
invent a concept no other tool has. It lives in `spec/milestone.rs`, read
only when the GSD dialect is detected, and the rest of the surface keeps
speaking in roles.

**A fourth subject, outside `Subject::ALL`.** The three subjects are what
every tool may keep. The milestone is offered in front of them, only while
one is open, and a finished milestone (`complete`, `completed`,
`milestone_complete`, `shipped`, `archived`) is not shown.

**The step is pasted, never sent.** `SpecOutcome::Prompt { skill,
arguments }` asks the host to type into the focused agent through
bracketed paste, with control characters stripped and no Enter. The
operator sees it and decides. Closing the surface puts the prompt in view.

**The sigil is the integration's.** A harness calls a skill as `/name` or
`$name`. The extension returns the skill without a sigil; the host asks
the focused agent's integration (`IntegrationPort::skill_sigil`, `/` by
default) and spells it. Adding a harness adds nothing here.

**The next step follows GSD's own routing.** No plans and no context →
discuss; context and no plans → plan; plans not all summarised → execute;
all summarised and not verified → verify; verified → `gsd-next`; every
phase done → audit the milestone.

## Risks / Trade-offs

- GSD's file formats are not versioned. The reader takes what it finds and
  falls back (roadmap checklist, joined by the `### Phase` headings it has
  no line for; `Phase:` in the state body, then the first unfinished phase;
  the percent counted from the plans when the state has none), and shows
  less rather than failing.
- A step pasted while the agent is mid-turn is queued by the harness the
  way typed text would be; the surface does not check.
