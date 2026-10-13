## MODIFIED Requirements

### Requirement: An agent is placed on the target as the last sync left it
The system SHALL bring the local target of a project that isolates its
agents in line with the remote's on the workspace's clock, by fast-forward
and by nothing else, and SHALL NOT ask the remote while placing an agent.
A new agent's branch SHALL be cut from the most advanced tip of the target
the last sync left: the remote's tracking ref when it descends from the
local target, and the local target when the local target carries commits the
remote lacks, when the repository has no remote, or when the remote does not
have the target. The agent's base SHALL name the declared target and its
base commit SHALL be the commit the branch was cut from. An agent moved into
a checkout of its own from the primary checkout SHALL be cut from the
primary checkout's `HEAD`, whatever the target's tips say. A target carrying
commits the remote lacks, one Git refuses to move, and one checked out in the
primary checkout SHALL be left exactly as they stand. The system SHALL say
once, until the target catches up, only when a new agent would start behind
the remote's tip — a local target carrying commits the remote lacks — and
SHALL say nothing when the placement is cut from the remote's tip anyway. A
delivery SHALL fetch the target for itself.

#### Scenario: A placement does not wait on the remote
- **WHEN** the remote's target has moved since the last sync and an agent is created
- **THEN** the agent's branch is cut from the tips the last sync left without contacting the remote

#### Scenario: A new agent starts from what the remote has
- **WHEN** a sync ran after the remote's target moved and the target is not checked out in the primary checkout
- **THEN** the local target is fast-forwarded onto the remote's tip and the next agent's branch is cut from there

#### Scenario: A new agent starts from the remote while the operator stands on the target
- **WHEN** the operator has the target checked out in the primary checkout, the remote has moved, and a sync ran
- **THEN** the sync leaves the target and the working tree as they stand, says nothing, and the next agent's branch is cut from the remote's tip

#### Scenario: An agent moved into a checkout of its own continues the operator's work
- **WHEN** an agent in the primary checkout is moved into a checkout of its own while the remote's tip is ahead of the local target
- **THEN** its branch is cut from the primary checkout's `HEAD`, not from the remote's tip

#### Scenario: The operator's own commits are never rewritten to sync
- **WHEN** the local target carries commits the remote does not and the remote has moved
- **THEN** the local target is left where it stands, the next agent's branch is cut from it, and the system says once how far behind the remote the local target is

#### Scenario: A repository with no remote is placed from its own target
- **WHEN** an agent is created in a repository with no remote
- **THEN** the agent is placed from the local target and nothing is reported

## ADDED Requirements

### Requirement: An agent's work is judged against the target it was cut from
Every judgement the system makes about an isolated agent's work — whether
its branch is in the target, whether its checkout, branch or shelf still
holds work the target lacks, how many commits are its own, and what a
refresh rebases it onto — SHALL be asked against the most advanced of the
local target and the remote's tracking ref for it that this machine already
has; when the two have diverged, against the one the agent's branch
descends from, and the local target when it descends from neither. The same
rule SHALL hold for every agent, however it was placed, and asking it SHALL
NOT contact the remote.

#### Scenario: The remote's commits are not counted as the agent's
- **WHEN** an agent was cut from the remote's tip, which is ahead of the local target, and has committed nothing
- **THEN** its branch counts no commits of its own, holds no work, and its checkout is free to reuse

#### Scenario: The agent's own commits are counted once
- **WHEN** an agent cut from the remote's tip commits once
- **THEN** it is ready with exactly one commit ahead, and refreshing it does not move its base back to the local target

#### Scenario: Work merged on the remote is integrated before the operator pulls
- **WHEN** an agent's work reached the remote's target and a sync fetched it, while the local target is still behind
- **THEN** the agent's branch is judged integrated

#### Scenario: An agent cut from the operator's HEAD is judged by its own commits
- **WHEN** an agent moved into a checkout of its own from a local target behind the remote commits once
- **THEN** exactly that commit is counted as its own
