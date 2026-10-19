## MODIFIED Requirements

### Requirement: The agent grammar speaks of work

`uze agent task` SHALL be spelled `uze agent work` — the thing an agent
names is the work, whose checkout lives in a worktree and whose branch is
`agent/<id>`. The `name` verb and its argument grammar are unchanged
(`uze agent work name <type>/<subject>`), and no alias SHALL be kept. The
agent's audience reads the launch-instructions section, which SHALL carry
the new spelling, and the naming journey SHALL follow it.

#### Scenario: An agent names its work with the new spelling
- **WHEN** an agent runs `uze agent work name feat/plugin-authoring`
- **THEN** the work is named, the branch renamed, and the answer is
  identical to what `task name` returned — only the noun changed

#### Scenario: The old spelling is gone without an alias
- **WHEN** `uze agent task name …` is invoked
- **THEN** it is not recognized, and the message names `uze agent work`

### Requirement: A directory is a project only on evidence, and may be none

The project root SHALL be resolved by walking up from the working directory
and taking, in this order: the nearest ancestor holding `agents.yaml`; else
the repository root; else the nearest ancestor holding `AGENTS.md`. When
none of the three is found, there SHALL be **no project**, and no command
SHALL treat the working directory as one.

`AGENTS.md` SHALL NOT take precedence over a repository root: a nested
`AGENTS.md` scopes instructions to its directory, not a project.

#### Scenario: A nested AGENTS.md does not become the project root
- **WHEN** a repository holds `AGENTS.md` in a subdirectory and no
  `agents.yaml` anywhere
- **THEN** the project root is the repository root

#### Scenario: A plain directory is not a project
- **WHEN** a command runs in a directory that has no `agents.yaml`, is not
  inside a repository, and has no `AGENTS.md`
- **THEN** there is no project, and no project file is created anywhere

#### Scenario: A manifest wins over the repository it sits in
- **WHEN** a repository holds `agents.yaml` in a subdirectory
- **THEN** that subdirectory is the project root

#### Scenario: A worktree is a repository
- **WHEN** a command runs inside a Git worktree, where the repository marker
  is a file rather than a directory
- **THEN** the worktree's root is found as a repository root
