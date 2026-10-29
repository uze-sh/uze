## Why

An agent's overview collapsed to its Agent section whenever its directory
was not inside a project, its project could not be read, or the directory
was gone: the section bar went away, and with it Memory and Settings,
which need no project at all. Memory reaches every agent from the machine
and from its harness for any directory, and the `workspace:` settings are
already read as defaults when `agents.yaml` is absent and create it on the
first change. Refusing both because `uze status` has nothing to say was a
gate on the whole screen for what only some of its parts need.

## What Changes

- **Every section stands, always.** The section bar is drawn whatever the
  read answered; a section that cannot be read says why in its own empty
  state, and its tab is marked, instead of the overview losing all but
  the Agent section.
- **Outside a project is not a warning.** A directory with no project in or
  above it is overviewed as itself: Memory lists what the machine and its
  harnesses keep for it, Settings shows the defaults and the first change
  creates `agents.yaml` there, Plugins and Worktrees show their empty
  states. Nothing is marked and nothing is counted as an issue.
- **What needs a project waits for one.** `uze status`'s parts (the
  context delivery, commands waiting for approval, steps not spelled for
  this platform) are absent outside a project rather than failing it.
- A directory that is gone, or a read that failed, still says so, now in
  each section rather than in place of them.

## Impact

- `uze-application`: `SpaceOverview` loses `NotAProject`; `ProjectSpace`
  carries `root` and an optional `status`.
- `src/ui/orchestrator`: the overview's rendering, health and issues.
- Spec: `space-overview`.
