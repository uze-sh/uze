## Why

Every agent's row in the sidebar names its project by the directory's
own name, and nothing lets the operator say it otherwise. Two clones of
one repository both read `uze`, and a directory named for a machine's
layout (`app`, `repo`, `work`) says nothing about the project in it. The
name the operator wants is their own choice about their own workspace,
not something the project declares for everyone who clones it.

## What Changes

- The operator can give a project a label of their own. It is kept on
  this machine, in `config.toml`, under the project's root, and never
  written into the project.
- Every workspace surface that names a project names it by that label,
  falling back to the directory's name when none is set: the agent's row,
  recent's selected row and filter, and the approval of a project's
  commands and its toasts. Surfaces that show a path keep the path.
- The agent's context menu keeps one `Rename` entry that opens a
  submenu, `Agent` and `Project`, the way `Continue in` opens its
  harnesses. `Project` renames the project for every agent working in it.
- An entry that opens a submenu is marked by an arrow at the menu's right
  edge instead of an ellipsis, `Continue in` included; the ellipsis is
  kept for entries that ask before acting.
- A double click on the row's project renames the project inline; a
  double click on the agent's name keeps renaming the agent.
- Confirming an empty label takes the label away, and the directory's
  name is shown again.

## Capabilities

### New Capabilities
- `project-labels`: the operator's own name for a project, where it is
  kept, which surfaces read it, and the gestures that set it.

### Modified Capabilities
<!-- None: `space-groups` and `recent-work` require that a row names its
project, and a label still does. Recent offers no rename, as it offers
no action but its row's own. -->

## Impact

- `uze-workspace`: a concern owning the `projects` section of
  `config.toml`, the way `notifications` owns its own.
- `uze-core::config`: reading and writing a key one table deeper than a
  section, keeping the operator's comments and ordering.
- `uze-application`: a service for the label, so `src/` never reads the
  file itself.
- `src/ui/orchestrator`: one function naming a project for every surface
  (two copies of `project_name` today), the `Rename` submenu and
  its arrow, the double click on the caption's project, and the inline edit committing
  to the label.
