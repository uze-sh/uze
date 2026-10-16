## MODIFIED Requirements

### Requirement: A checkout carries a record that UZE made it
The system SHALL record, when it makes a checkout, that it made it, the
checkout's path, and, for a checkout made for a subagent, the agent it was
split from and the commit it was split at. Which agent currently holds a
checkout SHALL remain the task store's answer, never the record's. The
record SHALL live with the checkout itself: it SHALL survive the loss of
UZE's own state, and it SHALL cease to exist when the worktree is removed,
by UZE or by Git. A record naming a path other than the checkout it is
found in SHALL be ignored, except where the record says the system was
moving the checkout to exactly that path: a move SHALL be recorded as
intended before Git performs it, and SHALL be completed or undone on the
next reconciliation, with Git's own repair run before anything is pruned,
so a crash during a move never disowns a checkout the system made. A record this build cannot read — unreadable,
or written by a newer build — SHALL make the checkout neither reused nor
removed, and SHALL NOT be written over.

#### Scenario: A made checkout is recorded
- **WHEN** the system makes a checkout
- **THEN** the checkout carries a record saying UZE made it, at that path

#### Scenario: The record outlives lost state
- **WHEN** UZE's state for the project is deleted and the project's space starts again
- **THEN** every checkout UZE made is still known as UZE's, and nothing it did not make is taken for one
- **AND** each is accounted for from its facts: parked when it holds work, free otherwise

#### Scenario: The record goes with the worktree
- **WHEN** a recorded checkout's worktree is removed, by the system or with Git directly
- **THEN** no record of it remains anywhere

#### Scenario: A copied checkout does not inherit the record
- **WHEN** a recorded checkout's directory is copied and the copy is registered with Git
- **THEN** the copy is not taken for UZE's

#### Scenario: A record from a newer build
- **WHEN** a checkout carries a record this build cannot read
- **THEN** it is neither offered to an agent nor removed, and the record is left as it was

#### Scenario: A move interrupted by a crash
- **WHEN** the system crashes after a checkout was moved to an agent's path but before its record names that path
- **THEN** the next reconciliation finds the record's intended path, takes the checkout as the system's at its new path, and prunes nothing of it

### Requirement: The pool keeps a few spare slots and no more
The system SHALL remove the directory of a free slot unused for longer
than a declared age, three days when the project declares none, and SHALL
keep every other free slot as a spare for the next agent, so the slots a
project has are the most it had in use at once within that age. The rule
SHALL apply to free slots only, SHALL keep each removed slot's branch, and
SHALL be decided from the slots' present state alone, with no record of
past use or count of spares. A free slot SHALL keep the name of the agent
that last held it until another agent takes it, and SHALL count as a
spare whatever its name. A project MAY declare the age under its worktree
policy.

#### Scenario: Closing agents leaves spares for the next ones
- **WHEN** five agents are working and two of them end with nothing left in their checkouts
- **THEN** both checkouts stay, and the next two agents created are placed in them rather than in new directories

#### Scenario: Spares beyond the declared number are removed
- **WHEN** five agents end with nothing left in their checkouts and the project declares no age
- **THEN** all five checkouts stay as spares, since no number of them is declared, and each directory is removed once it has gone unused for three days

#### Scenario: An idle project gives its disk back
- **WHEN** a free slot has not been used for more than three days and the project declares no age
- **THEN** its directory is removed and its branch is kept

#### Scenario: Work is never trimmed
- **WHEN** the pool holds free slots idle past the declared age beside parked or occupied checkouts
- **THEN** the idle free slots are removed and none of the parked or occupied checkouts is

#### Scenario: A slot keeps its last agent's name while it waits
- **WHEN** an agent ends with nothing left in its checkout and no other agent takes the slot
- **THEN** the slot stays at that agent's path, and resuming the agent places it there without moving anything
