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
