## Why

GSD keeps more than units of intent. `.planning/STATE.md` holds an open
milestone, `ROADMAP.md` its phases, and each phase its plans in waves, and
the whole layout exists to answer one question: what should the agent run
next. The spec surface reads a GSD checkout as phases and documents, so it
shows the files and loses that answer, and the operator still has to work
out and type the next command by hand. Issue #287 asks for the same loop
for extensions in general; this change closes it for the built-in surface,
where the host already trusts the code.

## What Changes

- A fourth subject, **milestone**, offered only in a GSD checkout whose
  `STATE.md` has a milestone open. It lists an overview and one row per
  phase, each with its plans done out of all and the current phase marked.
- The overview renders the milestone's progress, where it stopped, its
  blockers and pending todos. A phase renders its goal, its success
  criteria and its plans grouped by wave, with done, current and
  stops-for-you marks, and the step GSD would run next.
- Activating a row offers its steps (discuss, plan, execute, verify for a
  phase; the next step and progress for the overview). Picking one types
  the skill into the focused agent's prompt, **unsent**, spelled the way
  that agent calls a skill, and closes the surface.
- The surface opens on the milestone when the agent has no unit of its own.
- A harness integration says how a skill is called at its prompt (`/` by
  default, `$` for the one that differs), so no vendor is named outside
  the integrations.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `spec-surface`: a fourth subject (milestone), its rows and rendering, and
  a step handed to the agent as an unsent prompt.

## Impact

- `crates/uze-extensions/src/spec/milestone.rs`: the milestone reader and
  the step recommendation, GSD's own and not a dialect table.
- `crates/uze-extensions/src/spec.rs`: `Subject::Milestone`, the step menu,
  `SpecOutcome::Prompt { skill, arguments }`.
- `crates/uze-core` (`IntegrationPort::skill_sigil`), `uze-integrations`,
  `uze-application` (`AgentIdentity.skill_sigil`) and the orchestrator,
  which pastes the step bracketed into the focused agent and never sends it.
