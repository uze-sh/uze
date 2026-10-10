## MODIFIED Requirements

### Requirement: A checkout carries a record that UZE made it
The system SHALL record, when it makes a checkout, that it made it, the
checkout's path, and, for a checkout made for a subagent, the agent it was
split from and the commit it was split at. Which agent currently holds a
checkout SHALL remain the task store's answer, never the record's. The
record SHALL live with the checkout itself: it SHALL survive the loss of
UZE's own state, and it SHALL cease to exist when the worktree is removed,
by UZE or by Git. A record naming a path other than the checkout it is
found in SHALL be ignored. A record this build cannot read — unreadable,
or written by a newer build — SHALL make the checkout neither reused nor
removed, and SHALL NOT be written over.

#### Scenario: A made checkout is recorded
- **WHEN** the system makes a checkout
- **THEN** the checkout carries a record saying UZE made it, at that path

#### Scenario: The record outlives lost state
- **WHEN** UZE's state for the project is deleted and the project's space starts again
- **THEN** every checkout UZE made is still known as UZE's, and nothing it did not make is taken for one
- **AND** each is accounted for from its facts: its work shelved and the checkout freed when it holds work, free otherwise, pinned when an operation is paused or a process works in it

#### Scenario: The record goes with the worktree
- **WHEN** a recorded checkout's worktree is removed, by the system or with Git directly
- **THEN** no record of it remains anywhere

#### Scenario: A copied checkout does not inherit the record
- **WHEN** a recorded checkout's directory is copied and the copy is registered with Git
- **THEN** the copy is not taken for UZE's

#### Scenario: A record from a newer build
- **WHEN** a checkout carries a record this build cannot read
- **THEN** it is neither offered to an agent nor removed, and the record is left as it was

### Requirement: Content UZE derives never parks a checkout
When deciding whether a checkout holds work, the system SHALL NOT count as
work an uncommitted change confined to content UZE derives and can produce
again: a change to the project's lock file that moves no plugin's pin —
every plugin present both in the committed lock and in the working one
keeps the same revision and digest, so only entries were added or dropped —
and a change to the shared instruction file confined to the regions UZE
manages in it. Such a change SHALL NOT be shelved. A checkout whose only
uncommitted changes are of that kind SHALL be free when nothing else holds
it, and reuse SHALL discard those changes with the rest of the working
tree. A moved pin is work. Rebasing, joining and delivering SHALL still
require a working tree with no uncommitted change at all.

#### Scenario: A lock rewritten inside a slot
- **WHEN** an agent's checkout ends with the lock file changed only by entries added or dropped, and its branch holds nothing the target lacks
- **THEN** nothing is shelved, the checkout is free, and the next agent placed there starts from the base's lock file

#### Scenario: A pin moved on purpose
- **WHEN** an agent's checkout ends with the lock file moving a plugin's pin
- **THEN** the checkout holds work, and the change is shelved

#### Scenario: A managed region reprojected inside a slot
- **WHEN** the only change in a checkout is inside a region UZE manages in the instruction file
- **THEN** nothing is shelved and the checkout is free

#### Scenario: A hand edit beside a managed region
- **WHEN** the instruction file also has a change outside every managed region
- **THEN** the checkout holds work, and the change is shelved

### Requirement: An agent with unjoined children is neither delivered nor freed
The system SHALL refuse to deliver an agent while it has a child holding
commits its branch lacks or uncommitted changes, stating which children.
When an agent ends, the system SHALL release each child that holds nothing
the agent's branch lacks; each child holding uncommitted changes SHALL have
them shelved, each child's commits SHALL stay on the child's branch, and
every child's checkout SHALL be freed. An agent with a child holding work
SHALL itself be shelved, whatever its own branch holds. Resuming that agent
SHALL place each such child back in a checkout of its own, as that agent's
child under the same topic, with its shelf restored, so that it can be
joined; that, then joining, SHALL be how the operator brings a shelved
child's work into its shelved agent. Discarding the agent SHALL discard its
children.

#### Scenario: Delivery waits for the children
- **WHEN** delivery is asked for an agent whose child holds commits the agent's branch lacks, or uncommitted changes
- **THEN** it is refused, naming the child

#### Scenario: Clean children go back to the pool
- **WHEN** an agent ends and its children were all joined or never used
- **THEN** their checkouts are free

#### Scenario: A child holding work keeps its agent
- **WHEN** an agent ends with nothing of its own while a child holds commits
- **THEN** the agent and the child are both shelved, neither checkout is kept, and the child's commits stay on its branch

#### Scenario: Resuming an agent brings its children back
- **WHEN** the operator resumes an agent shelved because a child held work
- **THEN** the child is in a checkout of its own again, under the same topic, and the agent can join it

## REMOVED Requirements

### Requirement: The pool keeps a few spare slots and no more
**Reason**: Trimming free checkouts down to a fixed count removed the
directories a project with more concurrent agents than the count needs
again, so every burst paid for a new worktree and its `setup`, and lost
its build caches. The idle age alone bounds the pool by peak concurrency.
**Migration**: Remove `spare` from `workspace:` in `agents.yaml`; a
manifest that still declares it is reported as declaring an unknown key.
`idle_days` keeps its meaning.

## ADDED Requirements

### Requirement: The pool keeps free checkouts until they go idle
The system SHALL keep the directory of every free checkout until it has
gone unused for longer than a declared age, measured from when it was last
placed or released — three days when the project
declares none — and SHALL then remove the directory on the next
collection, keeping its branch. The rule SHALL apply to free checkouts
only, and SHALL be decided from the checkouts' present state alone, with
no record of past use. A project MAY declare the age as `idle_days` under
its worktree policy; it SHALL NOT be able to declare a number of checkouts
to keep.

#### Scenario: Four agents, four checkouts
- **WHEN** four agents end with nothing left in their checkouts and four new agents are created the same day
- **THEN** the four new agents are placed in the four existing checkouts and no directory is created

#### Scenario: An idle project gives its disk back
- **WHEN** a free checkout has not been used for more than three days and the project declares no age
- **THEN** its directory is removed and its branch is kept

#### Scenario: In use and pinned checkouts are never trimmed
- **WHEN** a checkout held by a live agent or pinned has gone unchanged for longer than the idle age
- **THEN** it is not removed

#### Scenario: A checkout used for days is not idle when released
- **WHEN** an agent worked in a checkout for days and its tab closes
- **THEN** the checkout is kept for the idle age from that moment

#### Scenario: A task that left nothing is forgotten
- **WHEN** a task ended more than the idle age ago, closed or integrated, holding no checkout, with no branch, no shelf and no subagent naming it
- **THEN** its record is removed from the project's task store on the next collection

#### Scenario: A task that left work is never forgotten
- **WHEN** a task ended more than the idle age ago and its branch or its shelf still holds work
- **THEN** its record is kept

#### Scenario: A manifest still declaring a spare count
- **WHEN** a project's `agents.yaml` declares `spare` under `workspace:`
- **THEN** no agent is placed, and the operator is told the policy declares an unknown key named `spare`, as for any other unknown key
