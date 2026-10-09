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
When deciding whether a checkout is free or parked, the system SHALL NOT
count as work an uncommitted change confined to content UZE derives and
can produce again: a change to the project's lock file that moves no
plugin's pin — every plugin present both in the committed lock and in the
working one keeps the same revision and digest, so only entries were added
or dropped — and a change to the shared instruction file confined to the
regions UZE manages in it. A checkout whose only uncommitted changes are
of that kind SHALL be free when nothing else holds it, and reuse SHALL
discard those changes with the rest of the working tree. A moved pin is
work. Rebasing, joining and delivering SHALL still require a working tree
with no uncommitted change at all.

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
The system SHALL keep at most a declared number of free slots — two when
the project declares none — choosing the most recently used, and SHALL
remove the directory of every other free slot on the next collection. It
SHALL also remove the directory of a free slot unused for longer than a
declared age — three days when the project declares none. Both rules
SHALL apply to free slots only, SHALL keep each removed slot's branch, and
SHALL be decided from the slots' present state alone, with no record of
past use. A project MAY declare either number under its worktree policy.

#### Scenario: Closing agents leaves spares for the next ones
- **WHEN** five agents are working and two of them end with nothing left in their checkouts
- **THEN** both checkouts stay, and the next two agents created are placed in them rather than in new directories

#### Scenario: Spares beyond the declared number are removed
- **WHEN** five agents end with nothing left in their checkouts and the project declares no number
- **THEN** the two most recently used checkouts stay and the other three directories are removed on the next collection

#### Scenario: An idle project gives its disk back
- **WHEN** a free slot has not been used for more than three days and the project declares no age
- **THEN** its directory is removed and its branch is kept

#### Scenario: Work is never trimmed
- **WHEN** the pool holds more checkouts than the declared number and some of them are parked or occupied
- **THEN** none of the parked or occupied checkouts is removed

### Requirement: Every worktree of a project is accounted for
The system SHALL account for every worktree the repository registers,
wherever it is on disk, and SHALL classify each by owner: an agent's
checkout, a subagent's checkout (with the agent it belongs to), a
harness's own isolation, or the operator's. A harness's own isolation
SHALL be recognised from where the integration for that harness declares
it keeps worktrees, relative to any checkout of the project, and a
worktree no integration claims and UZE did not record SHALL be the
operator's. For each, the system SHALL know its branch, whether it holds
uncommitted changes, whether its work is in the target, when it was last
changed, and whether it is in use.

#### Scenario: A harness's own isolation is named as such
- **WHEN** a harness creates a worktree where its integration declares that harness keeps them, from the primary or from inside a slot
- **THEN** it is accounted for as that harness's, not as the operator's and never as UZE's

#### Scenario: A worktree outside every known place
- **WHEN** the operator adds a worktree anywhere with Git
- **THEN** it is accounted for as the operator's, with its facts

### Requirement: A checkout in use is occupied
The system SHALL treat a checkout as occupied while any live process of
the operator's has its working directory inside it, for every automatic
decision that resets, reuses, collects or removes a checkout — its own
recorded checkouts included — and for the operator's remove action. A
process that exits during the reading, or whose working directory the
platform withholds, SHALL be skipped. A failure to enumerate processes at
all SHALL make every checkout in use for that decision. The system's own
long-lived processes SHALL NOT keep a working directory inside any
checkout.

#### Scenario: A process inside a free-looking slot
- **WHEN** a recorded slot is clean, its work is in the target and its agent has ended, but a process is still working inside it
- **THEN** it is not offered to a new agent and not collected

#### Scenario: The process leaves
- **WHEN** the last process working inside that slot exits
- **THEN** the slot is free again on the next evaluation

#### Scenario: The workspace server was started from inside a slot
- **WHEN** the workspace server is first started by a `uze` running inside an agent's checkout
- **THEN** that checkout is not held in use by the server once the agent ends

### Requirement: An agent gives a subagent a checkout by asking for one
The system SHALL offer the agent audience a verb that, given a topic, gives
the calling agent a checkout for a subagent: taken from the same slot pool
an agent's checkout comes from and counted against the same cap, prepared
the way an agent's checkout is prepared, on a new branch cut from the
caller's current commit, recorded as the caller's child with the commit it
was split at, and labelled by the topic. It SHALL answer with the
checkout's absolute path. The caller SHALL be identified by the identity
UZE stamped into its environment at launch. A topic the caller already has
a live child for SHALL answer with that child's checkout.

The verb SHALL refuse, stating which of these holds: the caller is not an
agent UZE launched; the caller is not isolated, so its branch is the
operator's; the caller is itself working in one of its children's
checkouts; the caller's checkout has a rebase or merge in progress; the
project's declared cap is reached.

#### Scenario: A subagent gets a warm checkout
- **WHEN** an isolated agent asks for a subagent checkout for topic `parser` while a free slot exists
- **THEN** it is answered with that slot's path, now on a new branch cut from the agent's current commit and recorded as the agent's child

#### Scenario: Asked from outside an agent
- **WHEN** the verb is run by a process UZE did not launch as an agent
- **THEN** it is refused with that reason, and no checkout is made

#### Scenario: Asked by an agent on the operator's branch
- **WHEN** an agent working in the project's primary checkout asks for a subagent checkout
- **THEN** it is refused, saying its branch is the operator's

#### Scenario: Asked from inside a child's checkout
- **WHEN** the verb is run from a directory inside one of the caller's children
- **THEN** it is refused, saying a subagent's checkout cannot be split again

#### Scenario: Asked twice for one topic
- **WHEN** an agent asks again for a topic whose child is still live
- **THEN** it is answered with the same checkout

#### Scenario: The cap is reached
- **WHEN** an agent asks for a subagent checkout and the project's declared cap is reached
- **THEN** no checkout is made and the agent is told why

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
The system SHALL offer the agent audience a verb that brings a child's
commits onto the calling agent's own branch: the commits the child made
after its split point are replayed onto the caller's current commit, and
the caller's branch is then moved forward to them. The caller's branch
SHALL gain the child's commits and no merge commit, and SHALL NOT gain any
commit of its own a second time. The child's checkout SHALL then be
released.

Where the caller's current commit is already an ancestor of the child's
branch, the join SHALL only move the caller's branch forward. Once a
replay completes, the child's split point SHALL become the caller's commit
it was replayed onto.

The verb SHALL refuse, stating which of these holds: either checkout holds
uncommitted changes; a replay is paused in the child's checkout, to be
resolved and continued there; the child's checkout is not on the branch
recorded for it; a process other than the caller is working in the
child's checkout; the topic belongs to another agent's child. A replay
that conflicts SHALL be left paused in the child's checkout, reported with
the conflicting paths, and the child SHALL NOT be released until its
commits are on the caller's branch; asking again after the conflict is
resolved and the replay continued SHALL complete the join.

#### Scenario: A child is joined
- **WHEN** an agent joins a child whose branch has commits its own branch lacks
- **THEN** those commits are on the agent's branch with no merge commit, and the child's checkout is free for the next agent or subagent

#### Scenario: The agent moved on after the split
- **WHEN** the agent's branch was rebased onto its target after the child was split, and the child is joined
- **THEN** only the child's own commits are added, and none of the agent's pre-rebase commits comes back

#### Scenario: A join that conflicts
- **WHEN** replaying a child's commits conflicts with the agent's own
- **THEN** the replay is left paused in the child's checkout, the conflicting paths are reported, and the child's checkout is kept
- **AND** asking again once the conflict is resolved completes the join

#### Scenario: Joining while the subagent still works
- **WHEN** a process other than the calling agent is working in the child's checkout
- **THEN** the join is refused with that reason, and nothing moves

#### Scenario: A child that left its branch
- **WHEN** the subagent switched its checkout to another branch or detached it
- **THEN** the join is refused, naming the branch it found, and nothing moves

#### Scenario: Another agent's child
- **WHEN** an agent names a topic that belongs to a different agent's child
- **THEN** it is refused and nothing moves

### Requirement: An agent lists its subagents' checkouts
The system SHALL offer the agent audience a verb that lists the calling
agent's children: topic, path, branch, whether it holds uncommitted
changes, and how many commits it holds that the caller's branch lacks.

#### Scenario: Listing children
- **WHEN** an agent with two children lists them
- **THEN** both are listed with their topic, path, branch and state

### Requirement: An agent with unjoined children is neither delivered nor freed
The system SHALL refuse to deliver an agent while it has a child holding
commits its branch lacks or uncommitted changes, stating which children. When an agent ends, the
system SHALL release each child that holds nothing the agent's branch
lacks, and SHALL park each child that holds uncommitted changes or commits
the agent's branch lacks; an agent with a parked child SHALL itself be
parked, its checkout kept, whatever its own branch holds. The operator
SHALL be able to join a parked child into its parked agent, or discard it.

#### Scenario: Delivery waits for the children
- **WHEN** delivery is asked for an agent whose child holds commits the agent's branch lacks, or uncommitted changes
- **THEN** it is refused, naming the child

#### Scenario: Clean children go back to the pool
- **WHEN** an agent ends and its children were all joined or never used
- **THEN** their checkouts are free

#### Scenario: A child holding work keeps its agent
- **WHEN** an agent ends with nothing of its own while a child holds commits
- **THEN** the child and the agent are both parked, both checkouts are kept, and the operator can join the child into the agent

### Requirement: The operator sees every checkout and acts on it explicitly
The workspace client SHALL show a subagent's checkout under the agent it
belongs to, and SHALL offer a view listing every worktree of the project
grouped by owner, with the facts the system accounts for. From that view
the operator SHALL be able to open a space in a checkout; adopt a foreign
checkout under the isolation directory as UZE's, being told that a clean
one becomes free at once; remove a checkout; and, in one action, remove
every checkout of the operator's that is clean, not in use, and whose work
is in the target. A harness's own isolation SHALL be listed as left to its
harness and SHALL NOT be removed by that action. Removal SHALL keep the branch, and SHALL be refused while the
checkout holds uncommitted changes, holds commits no branch reaches, or is
in use, with the reason stated. The primary checkout SHALL never be
adopted or removed. None of these SHALL happen without the operator
choosing it, and reading the facts SHALL never hold up drawing.

#### Scenario: A foreign checkout is listed and left alone
- **WHEN** the operator opens the checkouts view in a project with a harness's own worktree and one added by hand
- **THEN** both are listed under their owners with their facts, and neither is changed by opening the view

#### Scenario: Adopting a foreign checkout
- **WHEN** the operator adopts a checkout under the isolation directory listed as theirs
- **THEN** it is recorded as UZE's and from then on is treated as any slot is

#### Scenario: Cleaning up what is done
- **WHEN** the operator asks to clean up and three of the operator's checkouts are clean, unused and in the target while a fourth holds work and a harness keeps one of its own
- **THEN** the three are removed with their branches kept, the fourth is listed as kept, with why, and the harness's is left to its harness

#### Scenario: Removal is refused over work
- **WHEN** the operator removes a checkout that holds uncommitted changes, or in which a process is working
- **THEN** nothing is removed and the reason is stated

### Requirement: The projected policy names the verbs
The projected worktree-policy region SHALL tell an agent to give its
subagents checkouts through the split verb, to bring their work back
through the join verb, and to list them through the list verb, and SHALL
NOT carry a Git command for making a worktree. It SHALL NOT prescribe a
directory for subagent checkouts.

#### Scenario: The projected text
- **WHEN** a project's context is reconciled
- **THEN** its worktree-policy region names the three verbs and contains no `git worktree add`

