# checkout-accounting Specification

## Purpose
Lets UZE know which checkouts of a project it made and which belong to
someone else, so that no automatic path ever resets, reuses or removes a
checkout it did not make, keeps the slot pool from growing on UZE's own
writes, and gives an agent a verb for its subagents' checkouts instead of
Git written by hand.

## Requirements

### Requirement: A checkout carries a record that UZE made it
The system SHALL record, when it makes a checkout, that it made it, the
checkout's path, and, for a checkout made for a subagent, the agent it was
split from and the commit it was split at. Which agent currently holds a
checkout SHALL remain the task store's answer, never the record's. The
record SHALL live with the checkout itself: it SHALL survive the loss of
UZE's own state, and cease to exist when the worktree is removed, by UZE or
by Git.

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

### Requirement: A record names the checkout it is found in
A record naming a path other than the checkout it is found in SHALL be
ignored, except where it says the system was moving the checkout to exactly
that path. A move SHALL be recorded as intended before Git performs it, and
SHALL be completed or undone on the next reconciliation, with Git's own
repair run before anything is pruned, so a crash during a move never
disowns a checkout the system made.

#### Scenario: A copied checkout does not inherit the record
- **WHEN** a recorded checkout's directory is copied and the copy is registered with Git
- **THEN** the copy is not taken for UZE's

#### Scenario: A move interrupted by a crash
- **WHEN** the system crashes after a checkout was moved to an agent's path but before its record names that path
- **THEN** the next reconciliation finds the record's intended path, takes the checkout as the system's at its new path, and prunes nothing of it

### Requirement: A record this build cannot read is left alone
A record this build cannot read, because it is unreadable or was written by
a newer build, SHALL make the checkout neither reused nor removed, and SHALL
NOT be written over.

#### Scenario: A record from a newer build
- **WHEN** a checkout carries a record this build cannot read
- **THEN** it is neither offered to an agent nor removed, and the record is left as it was

### Requirement: Only a recorded checkout is ever touched automatically
The system SHALL reset, reuse, collect or remove a checkout, and prune its
branch, on an automatic path only when the checkout carries UZE's record
and sits in the isolation directory. A checkout without one is foreign
wherever it sits and whatever it is called, and SHALL NOT be adopted as an
agent's by inference. A recorded checkout moved out of the isolation
directory SHALL be foreign.

#### Scenario: A checkout added by hand inside the isolation directory
- **WHEN** a person or an agent adds a worktree under the isolation directory with Git, and it is clean and level with the target
- **THEN** it is never offered to a new agent, reset, collected as idle or removed, and its branch is never pruned

#### Scenario: A foreign checkout named like a slot
- **WHEN** a worktree nobody recorded has a name of the shape the system generates
- **THEN** it is foreign

### Requirement: A checkout a launched agent names is recorded on sight
Whenever it accounts for a repository's checkouts, the system SHALL record
as its own every checkout under the isolation directory that the task
store names as the checkout of an agent UZE launched, and every checkout
named `agent-<n>` by the builds before slots. A checkout the task store
names only because an earlier build adopted it by inference SHALL NOT be
recorded, and SHALL be listed to the operator for adoption.

#### Scenario: An upgrade keeps every slot
- **WHEN** a build with this change first starts in a repository whose slots an earlier build made for agents it launched, and whose task store still records those agents
- **THEN** every one of those slots is recorded as UZE's and keeps its agent, its branch and its state
- **AND** a slot whose agent's record was lost before this build ran is listed to adopt rather than kept

#### Scenario: A slot an older build makes later
- **WHEN** an older build on the same machine makes a slot for an agent it launched, after this build has run
- **THEN** this build records it on sight and treats it as any slot

#### Scenario: An earlier build's inference is not inherited
- **WHEN** the task store names a checkout that an earlier build adopted without having made it
- **THEN** it is not recorded, and it is listed to the operator as a checkout to adopt or remove

### Requirement: Content UZE derives never parks a checkout
Judging a checkout free or parked, the system SHALL NOT count as work an
uncommitted change confined to content UZE derives: a lock file change
moving no pin (each plugin in both locks keeps its revision and digest), or
one inside UZE's managed regions of the shared instruction file. A checkout
with only such changes SHALL be free if nothing else holds it, and reuse
SHALL discard them. A moved pin is work. Rebase, join and delivery SHALL
still require a working tree with no uncommitted change.

#### Scenario: A lock rewritten inside a slot
- **WHEN** an agent's checkout ends with the lock file changed only by entries added or dropped, and its branch holds nothing the target lacks
- **THEN** the checkout is free, and the next agent placed there starts from the base's lock file

#### Scenario: A pin moved on purpose
- **WHEN** an agent's checkout ends with the lock file moving a plugin's pin
- **THEN** the checkout holds work and is parked

#### Scenario: A managed region reprojected inside a slot
- **WHEN** the only change in a checkout is inside a region UZE manages in the instruction file
- **THEN** the checkout is free

#### Scenario: A hand edit beside a managed region
- **WHEN** the instruction file also has a change outside every managed region
- **THEN** the checkout holds work and is parked

### Requirement: The pool keeps a few spare slots and no more
The system SHALL remove the directory of a free slot unused for longer than
a declared age, three days by default, and SHALL keep every other free slot
as a spare, so a project has the most slots it had in use at once within
that age. The rule SHALL touch free slots only, keep each removed slot's
branch, and be decided from the slots' present state alone. A free slot
SHALL keep its last agent's name until another agent takes it, and SHALL
count as a spare. A project MAY declare the age.

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

### Requirement: Every worktree of a project is accounted for
The system SHALL account for every worktree the repository registers,
wherever it is on disk, and SHALL classify each by owner: an agent's
checkout, a subagent's checkout (with the agent it belongs to), a harness's
own isolation, or, when no integration claims it and UZE did not record it,
the operator's. For each, the system SHALL know its branch, whether it holds
uncommitted changes, whether its work is in the target, when it was last
changed, and whether it is in use.

#### Scenario: A worktree outside every known place
- **WHEN** the operator adds a worktree anywhere with Git
- **THEN** it is accounted for as the operator's, with its facts

### Requirement: A harness's isolation is recognised where its integration keeps it
A harness's own isolation SHALL be recognised from where the integration
for that harness declares it keeps worktrees, either relative to any
checkout of the project or under a directory of that harness's own outside
the project, including worktrees a harness creates when the model calls a
worktree tool the harness provides.

#### Scenario: A harness's own isolation is named as such
- **WHEN** a harness creates a worktree where its integration declares that harness keeps them, from the primary or from inside a slot
- **THEN** it is accounted for as that harness's, not as the operator's and never as UZE's

#### Scenario: A worktree a harness keeps outside the project
- **WHEN** an agent's model calls the harness's own worktree tool and the harness creates the worktree in a directory of its own outside the project
- **THEN** it is accounted for as that harness's, shown, never driven and never removed

### Requirement: A checkout in use is occupied
A checkout SHALL be occupied while any live process of the operator's has
its working directory in it, for every automatic decision to reset, reuse,
collect or remove a checkout, UZE's own included, and for the operator's
remove action. A process gone mid-read, or whose working directory is
withheld, SHALL be skipped; failing to enumerate processes at all SHALL make
every checkout in use for that decision. UZE's own long-lived processes
SHALL NOT keep a working directory in any checkout.

#### Scenario: A process inside a free-looking slot
- **WHEN** a recorded slot is clean, its work is in the target and its agent has ended, but a process is still working inside it
- **THEN** it is not offered to a new agent and not collected

#### Scenario: The process leaves
- **WHEN** the last process working inside that slot exits
- **THEN** the slot is free again on the next evaluation

#### Scenario: The workspace server was started from inside a slot
- **WHEN** the workspace server is first started by a `uze` running inside an agent's checkout
- **THEN** that checkout is not held in use by the server once the agent ends

#### Scenario: Processes cannot be enumerated
- **WHEN** the system cannot enumerate processes at all while deciding whether to reuse or collect a slot
- **THEN** every checkout is taken as in use for that decision, and none is reused or removed

### Requirement: An agent gives a subagent a checkout by asking for one
The system SHALL offer the agent audience a verb giving the calling agent a
subagent checkout for a topic: from an agent's slot pool and cap, prepared
alike, on a new branch from the caller's current commit, recorded as its
child with that commit, labelled by the topic. It SHALL answer with the
absolute path, identify the caller by the identity UZE stamped into its
environment at launch, and answer a topic it already has a live child for
with that child's checkout.

#### Scenario: A subagent gets a warm checkout
- **WHEN** an isolated agent asks for a subagent checkout for topic `parser` while a free slot exists
- **THEN** it is answered with that slot's path, now on a new branch cut from the agent's current commit and recorded as the agent's child

#### Scenario: Asked twice for one topic
- **WHEN** an agent asks again for a topic whose child is still live
- **THEN** it is answered with the same checkout

### Requirement: Asking for a subagent checkout is refused where none can be given
The verb SHALL refuse, stating which of these holds: the caller is not an
agent UZE launched; the caller is not isolated, so its branch is the
operator's; the caller is itself working in one of its children's
checkouts; the caller's checkout has a rebase or merge in progress; the
project's declared cap is reached.

#### Scenario: Asked from outside an agent
- **WHEN** the verb is run by a process UZE did not launch as an agent
- **THEN** it is refused with that reason, and no checkout is made

#### Scenario: Asked by an agent on the operator's branch
- **WHEN** an agent working in the project's primary checkout asks for a subagent checkout
- **THEN** it is refused, saying its branch is the operator's

#### Scenario: Asked from inside a child's checkout
- **WHEN** the verb is run from a directory inside one of the caller's children
- **THEN** it is refused, saying a subagent's checkout cannot be split again

#### Scenario: The cap is reached
- **WHEN** an agent asks for a subagent checkout and the project's declared cap is reached
- **THEN** no checkout is made and the agent is told why

#### Scenario: Asked mid-rebase
- **WHEN** an agent whose checkout has a rebase or merge in progress asks for a subagent checkout
- **THEN** it is refused, saying so, and no checkout is made

### Requirement: A subagent's checkout keeps its agent
When the task store's current holder of a recorded subagent checkout has
lost its parent — an older build rewrote the store without it — and still
starts at the record's split point, the system SHALL restore the parent
from the record. A checkout reused for someone else SHALL carry a record
rewritten for its new holder.

#### Scenario: An older build dropped the parent
- **WHEN** an older build saved the task store without a child's parent, and this build accounts for the checkouts again
- **THEN** the child is the agent's child again, held and ended with it

### Requirement: A subagent's checkout is held until it is joined or its agent ends
The system SHALL keep a child's checkout held for as long as its agent is
live, whether or not any process is working in it: a child SHALL NOT be
released because no pane carries it, SHALL NOT be revived, followed onto
the target, named from a commit, evaluated for readiness or offered
delivery. It SHALL be released only by being joined, or by its agent's
end.

#### Scenario: A child nobody is working in right now
- **WHEN** an agent has split a child and no process is working inside it
- **THEN** the child's checkout is not offered to a new agent and is not collected

### Requirement: An agent brings a subagent's work back by joining it
The agent audience SHALL have a verb that replays a child's commits since
its split point onto the caller's current commit and moves the caller's
branch to them. That branch SHALL gain them with no merge commit and no
commit of its own twice, and the child's checkout SHALL then be released.
Where the caller's commit already is an ancestor of the child's branch, the
join SHALL only fast-forward the caller's branch. A completed replay SHALL
make the commit it went onto the child's split point.

#### Scenario: A child is joined
- **WHEN** an agent joins a child whose branch has commits its own branch lacks
- **THEN** those commits are on the agent's branch with no merge commit, and the child's checkout is free for the next agent or subagent

#### Scenario: The agent moved on after the split
- **WHEN** the agent's branch was rebased onto its target after the child was split, and the child is joined
- **THEN** only the child's own commits are added, and none of the agent's pre-rebase commits comes back

### Requirement: A join is refused while either checkout is not ready
The join verb SHALL refuse, stating which of these holds: either checkout
holds uncommitted changes; a replay is paused in the child's checkout, to be
resolved and continued there; the child's checkout is not on the branch
recorded for it; a process other than the caller is working in the child's
checkout; the topic belongs to another agent's child.

#### Scenario: Joining while the subagent still works
- **WHEN** a process other than the calling agent is working in the child's checkout
- **THEN** the join is refused with that reason, and nothing moves

#### Scenario: A child that left its branch
- **WHEN** the subagent switched its checkout to another branch or detached it
- **THEN** the join is refused, naming the branch it found, and nothing moves

#### Scenario: Another agent's child
- **WHEN** an agent names a topic that belongs to a different agent's child
- **THEN** it is refused and nothing moves

#### Scenario: Joining over uncommitted changes
- **WHEN** the agent's checkout or the child's holds uncommitted changes and the agent joins the child
- **THEN** the join is refused, naming the checkout, and nothing moves

### Requirement: A conflicting join stays paused in the child's checkout
A replay that conflicts SHALL be left paused in the child's checkout and
reported with the conflicting paths, and the child SHALL NOT be released
until its commits are on the caller's branch. Asking again after the
conflict is resolved and the replay continued SHALL complete the join.

#### Scenario: A join that conflicts
- **WHEN** replaying a child's commits conflicts with the agent's own
- **THEN** the replay is left paused in the child's checkout, the conflicting paths are reported, and the child's checkout is kept
- **AND** asking again once the conflict is resolved completes the join

### Requirement: An agent lists its subagents' checkouts
The system SHALL offer the agent audience a verb that lists the calling
agent's children: topic, path, branch, whether it holds uncommitted
changes, and how many commits it holds that the caller's branch lacks.

#### Scenario: Listing children
- **WHEN** an agent with two children lists them
- **THEN** both are listed with their topic, path, branch and state

### Requirement: An agent with unjoined children is neither delivered nor freed
The system SHALL refuse to deliver an agent while a child holds commits its
branch lacks or uncommitted changes, stating which children. When an agent
ends, the system SHALL release each child holding nothing the agent's branch
lacks and park each child holding uncommitted changes or commits it lacks;
an agent with a parked child SHALL itself be parked, its checkout kept,
whatever its own branch holds. The operator SHALL be able to join a parked
child into its parked agent, or discard it.

#### Scenario: Delivery waits for the children
- **WHEN** delivery is asked for an agent whose child holds commits the agent's branch lacks, or uncommitted changes
- **THEN** it is refused, naming the child

#### Scenario: Clean children go back to the pool
- **WHEN** an agent ends and its children were all joined or never used
- **THEN** their checkouts are free

#### Scenario: A child holding work keeps its agent
- **WHEN** an agent ends with nothing of its own while a child holds commits
- **THEN** the child and the agent are both parked, both checkouts are kept, and the operator can join the child into the agent

### Requirement: Removing a checkout from the view keeps its work
Removal SHALL keep the branch, and SHALL be refused while the checkout holds
uncommitted changes, holds commits no branch reaches, or is in use, with the
reason stated. A harness's own isolation SHALL be listed as left to its
harness and SHALL NOT be removed by the clean-up action. The primary
checkout SHALL never be adopted or removed.

#### Scenario: Removal is refused over work
- **WHEN** the operator removes a checkout that holds uncommitted changes, or in which a process is working
- **THEN** nothing is removed and the reason is stated

#### Scenario: The primary checkout is never offered for removal
- **WHEN** the operator opens the checkouts view or asks to clean up
- **THEN** the primary checkout can be neither adopted nor removed

### Requirement: The projected policy names the verbs
The projected worktree-policy region SHALL tell an agent to give its
subagents checkouts through the split verb, to bring their work back
through the join verb, and to list them through the list verb, and SHALL
NOT carry a Git command for making a worktree. It SHALL NOT prescribe a
directory for subagent checkouts.

#### Scenario: The projected text
- **WHEN** a project's context is reconciled
- **THEN** its worktree-policy region names the three verbs and contains no `git worktree add`

### Requirement: The client shows a checkout only through the agent holding it
The workspace client SHALL show a subagent's checkout under the agent it
belongs to, and the checkout an agent holds only as an attribute of that
agent, as `recent-work` describes. It SHALL NOT list checkouts no agent
holds, and SHALL NOT offer to open a space in a checkout, to adopt one, to
remove one, or to clean up the operator's checkouts. The only destructive
action it offers on an agent's checkout is discarding that agent's work, on
the operator's explicit confirmation.

#### Scenario: A foreign checkout is left alone
- **WHEN** a project holds a harness's own worktree and one the operator added by hand
- **THEN** the client lists neither and changes neither

#### Scenario: An agent's checkout is an attribute of the agent
- **WHEN** an agent with no live pane holds a checkout
- **THEN** the client shows that checkout only as the disk figure on the agent's selected row

#### Scenario: Discarding is the only removal offered
- **WHEN** the operator acts on an agent's kept work from the client
- **THEN** discarding it is the only action that removes its checkout, and it happens only on confirmation

### Requirement: Reading a checkout's facts never holds up drawing
The workspace client SHALL read a checkout's facts off the drawing path, so
reading them SHALL never hold up drawing.

#### Scenario: A slow checkout does not stall the client
- **WHEN** reading the facts of an agent's checkout takes a long time
- **THEN** the client keeps drawing and shows those facts once they arrive
