## MODIFIED Requirements

### Requirement: Isolated checkouts are reusable slots
The system SHALL keep isolated checkouts under the primary checkout's fixed
isolation directory, each at the path its agent owns, so an agent finds
its checkout at the same path every time it is placed. An agent's own path
SHALL be the directory it last held, and its own identifier when it never
held one. The system SHALL reuse a free checkout before creating another,
preferring one whose last agent will not come back to it, and SHALL rename
it to the agent's path with Git's own worktree move so the checkout keeps
its ignored files. A checkout Git refuses to move, or one any process is
working in — with or without a terminal — SHALL NOT be renamed, and SHALL
NOT stand in the way of creating a checkout at the agent's path instead;
nothing SHALL be copied. When another checkout already stands at the
agent's path, the agent SHALL be placed at a generated name and the
operator told that its conversation could not be carried. Reuse SHALL put the working tree at the task's base with no tracked
or untracked file from the previous task, and SHALL preserve ignored files.
A checkout holding uncommitted changes, or commits absent from its base,
SHALL never be reused; uncommitted changes confined to content the system
derives are not work (`checkout-accounting`). A checkout in which any
process is working SHALL never be reused. The number of checkouts SHALL be bounded by peak
concurrency, and a project MAY declare a cap.

#### Scenario: A free checkout is reused
- **WHEN** a task ends with its checkout clean and a new agent is created
- **THEN** the new agent starts in that checkout, on a new branch from the base
- **AND** ignored files such as build caches remain in place

#### Scenario: A previous task's edits never reach the next
- **WHEN** a checkout whose last task was delivered is reused
- **THEN** the working tree matches the base and carries none of the previous branch's edits

#### Scenario: A checkout holding work is never reused
- **WHEN** a task's agent is gone and its checkout has uncommitted changes or commits absent from its base
- **THEN** the checkout is parked, listed to the operator, and not offered to a new agent

#### Scenario: A slot whose work was squash-merged is free again
- **WHEN** a task's work reached the target as a squash or a rebase merge, so none of the branch's own commits is reachable from it, and the agent is gone
- **THEN** the checkout is free and the next agent is placed in it rather than in a new one

#### Scenario: A reused slot answers for its new task alone
- **WHEN** a new agent is placed in a checkout that earlier tasks ran in, and the record of one of them still reads as live
- **THEN** only the new task holds that checkout
- **AND** each earlier task ends by what its own branch holds, and keeps that branch

#### Scenario: A worktree the system did not create is not a slot
- **WHEN** a harness, an agent or an operator creates a worktree without the system's record, anywhere — inside the isolation directory, or a harness's own isolation started from inside an agent's checkout
- **THEN** it is never adopted as a slot, offered to an agent, swept as idle or removed, and its branch is never pruned
- **AND** nothing the system projects forbids it: work on another branch is the agent's to bring back to its own

#### Scenario: A new checkout is created only when none is free
- **WHEN** every existing checkout is occupied or parked
- **THEN** a new checkout is created

#### Scenario: A declared cap holds
- **WHEN** a project declares a maximum number of checkouts and all are occupied
- **THEN** no new agent is started until one is free, and the operator is told why

#### Scenario: A reused slot takes the agent's name
- **WHEN** a new agent is placed in a free checkout an earlier agent left
- **THEN** the checkout is renamed to the new agent's name, keeps its ignored files, and is recorded as the system's under that name

#### Scenario: A slot that cannot move is not taken
- **WHEN** the only free checkout holds an initialized submodule, is locked, or has a process working in it
- **THEN** a checkout is created at the agent's path, the free one is left as it was, and it does not count against the declared cap for that creation

#### Scenario: An agent comes back to the directory it last held
- **WHEN** an agent placed under an earlier build in a slot of a generated name is resumed and that slot is still free
- **THEN** it is placed in that slot, at that path, and nothing is moved

#### Scenario: The agent's path is taken
- **WHEN** an agent is resumed and another agent's checkout stands at its path
- **THEN** it is placed at a generated name and the operator is told its conversation could not be carried

#### Scenario: A slot whose agent may come back is the last to go
- **WHEN** a new agent is placed and the free slots include one left by a shelved agent and one left by an agent whose work was delivered
- **THEN** the slot left by the delivered agent is taken
