---
name: plan-phase
description: "Plans a phase of the milestone in .planning/: reads the roadmap, writes the phase's plan and records the state. Use when asked to plan phase N of the milestone."
invoke:
  model: true
  user: true
---
Plan the phase whose number came as the argument.

1. Read `.planning/ROADMAP.md` and find that phase's line, `- [ ] **Phase N: name**`.
2. Create `.planning/phases/NN-slug/NN-PLAN.md` (NN in two digits, the slug
   the name in lowercase with hyphens) with 3 to 5 numbered tasks, short and
   checkable, that complete the phase in this repository.
3. In `.planning/STATE.md`, set the `Phase:` line to `Phase: N` and the
   `Step:` line to `Step: planned`.

Do not carry out any task: only the plan. Answer in two lines what you planned.
