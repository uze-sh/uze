## ADDED Requirements

### Requirement: Only a person outside every sandbox answers for the operator
Approving a project's commands and consenting to an extension SHALL be
refused to a process carrying an agent's launch stamp, and also to a process
the kernel reports as confined (`no_new_privs` on Linux), which every sealed
agent pane and every common harness sandbox is and which no process can
leave. A terminal emulator or a stripped environment SHALL NOT turn such a
process into the person.

#### Scenario: An agent strips its stamp and fakes a terminal
- **WHEN** a process in a sealed agent pane runs `uze workspace allow` under `script` with `UZE_AGENT` and `UZE_AGENT_KEY` removed
- **THEN** it is refused, and nothing is approved

#### Scenario: The person approves from their own terminal
- **WHEN** the person runs `uze workspace allow` in an unconfined terminal and answers yes
- **THEN** the commands are approved

### Requirement: A harness sandbox is never given UZE's records to write
The directories UZE names to a harness as writable for an agent SHALL lie
outside `$UZE_HOME/state`, so an agent whose harness sandbox confines writes
cannot forge an approval or a task record.

#### Scenario: The writable roots of an agent
- **WHEN** UZE prepares the directories a harness is told an agent may write
- **THEN** none of them is under `$UZE_HOME/state`
