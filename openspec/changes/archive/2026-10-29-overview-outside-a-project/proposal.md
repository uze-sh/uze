## Why

An agent's overview collapsed to its Agent section whenever its directory
was not inside a project, its project could not be read, or the directory
was gone: the section bar went away, and with it Memory, which needs no
project at all. Settings, Plugins and Worktrees are a repository's: they
read and write `agents.yaml` and its lock and the checkouts Git made, and
an `agents.yaml` written in a plain directory such as `$HOME` would make it
the project of every directory below it that is not a repository.

## What Changes

- **The sections follow the directory.** Inside a Git repository the
  overview has all five sections. Outside every one it has the agent's
  prompts and its memory, and nothing is marked or counted as an issue
  for what is not there.
- **Memory reaches every agent.** Outside a repository, Memory lists the
  machine's facts and what the agent's harnesses keep for its directory.
- **Settings are written inside a repository only.**
  `apply_declaration` refuses a directory outside every Git repository,
  whatever asked.
- **A part that cannot be read empties its own sections.** A project that
  could not be read says so in Settings, Plugins and Worktrees, with Memory
  still listed; a directory that is gone, or a read that failed, says so
  in Memory, under the agent's prompts.
- The CLI is unchanged: `uze status` still answers for a project wherever
  one is declared.

## Impact

- `uze-core`: `project_root::repository_root_of`, read from the markers.
- `uze-application`: `SpaceOverview::Directory(DirectorySpace)` replaces
  `Project` and `NotAProject`; `DirectorySpace` carries the memory and an
  optional `ProjectSpace`, whose `status` is no longer optional.
- `src/ui/orchestrator`: the overview's sections, rendering, health and
  issues.
- Spec: `space-overview`.
