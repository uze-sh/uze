---
name: execute-phase
description: "Executes a planned phase of the milestone in .planning/: carries out the plan's tasks, summarizes what it did and moves the roadmap on. Use when asked to execute phase N of the milestone."
invoke:
  model: true
  user: true
---
Execute the phase whose number came as the argument.

1. Read that phase's `.planning/phases/NN-*/NN-PLAN.md`. With no plan, stop
   and say the phase must be planned first.
2. Carry out the plan's tasks in this repository, one at a time.
3. Write `.planning/phases/NN-*/NN-SUMMARY.md` with what was done, a few
   lines per task.
4. In `.planning/ROADMAP.md`, mark the phase done (`- [x]`). In
   `.planning/STATE.md`, set `Phase:` to the next phase not done and `Step:`
   to `pending`.

Answer in two lines what you executed.
