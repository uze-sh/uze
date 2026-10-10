## MODIFIED Requirements

### Requirement: Isolated checkouts are reusable slots
The system SHALL keep isolated checkouts under the primary checkout's fixed
isolation directory, each named by a generated identifier that never
changes, and SHALL reuse a free checkout for a new agent before creating
another. Reuse SHALL put the working tree at the task's base with no tracked
or untracked file from the previous task, and SHALL preserve ignored files.
A checkout SHALL hold no work of its own once its agent ends: uncommitted
work is shelved (`work-shelf`) and commits are kept by the task's branch,
and the checkout is then free. Uncommitted changes confined to content the
system derives are not work (`checkout-accounting`). Only these SHALL pin a
checkout so that it is neither reused nor removed: an operation Git has
paused in it (a rebase, a merge, a cherry-pick, a revert or a bisect), any
process working in it (`checkout-accounting`), and work in it that cannot be
shelved or whose shelf could not be written (`work-shelf`); the operator
SHALL be told which one holds each pinned checkout. A slot SHALL have
exactly one holder, decided by which task was last placed in it and never
by when tasks were created; where records left by an earlier build name
one slot from several tasks, the one whose branch the checkout has is its
holder, and when that does not decide it the checkout is pinned rather
than freed. The number of checkouts SHALL be
bounded by peak concurrency, and a project MAY declare a cap, which SHALL
count only checkouts held by a live agent or pinned.

#### Scenario: A free checkout is reused
- **WHEN** a task ends with its checkout clean and a new agent is created
- **THEN** the new agent starts in that checkout, on a new branch from the base
- **AND** ignored files such as build caches remain in place

#### Scenario: A previous task's edits never reach the next
- **WHEN** a checkout whose last task was delivered is reused
- **THEN** the working tree matches the base and carries none of the previous branch's edits

#### Scenario: A checkout whose agent left work is reused once the work is kept
- **WHEN** a task's agent is gone and its checkout had uncommitted changes or commits absent from its base
- **THEN** the changes are shelved, the commits stay on the task's branch, and the checkout is offered to the next agent

#### Scenario: A checkout holding work is never reused
- **WHEN** a checkout still holds uncommitted changes that are not shelved, because shelving them failed or an operation pins it
- **THEN** it is not offered to a new agent, and the operator is told why

#### Scenario: A paused operation pins a checkout
- **WHEN** a task's agent is gone and a rebase or a merge is paused in its checkout
- **THEN** nothing is shelved, the checkout is neither reused nor removed, and the operator is told it waits on that operation

#### Scenario: A slot whose work was squash-merged is free again
- **WHEN** a task's work reached the target as a squash or a rebase merge, so none of the branch's own commits is reachable from it, and the agent is gone
- **THEN** the checkout is free and the next agent is placed in it rather than in a new one

#### Scenario: A reused slot answers for its new task alone
- **WHEN** a new agent is placed in a checkout that earlier tasks ran in, and the record of one of them still reads as live
- **THEN** only the new task holds that checkout
- **AND** each earlier task ends by what its own branch and shelf hold, and keeps them

#### Scenario: Two records name one slot
- **WHEN** records left by an earlier build name one checkout from two tasks, one of whose branches the checkout has
- **THEN** that task holds the checkout and the other no longer names it

#### Scenario: The clock going back does not move a slot
- **WHEN** the system clock steps backwards between two placements, so a task placed later carries an earlier creation time
- **THEN** the task placed later still holds its checkout, and the task running in it is not ended

#### Scenario: A worktree the system did not create is not a slot
- **WHEN** a harness, an agent or an operator creates a worktree without the system's record, anywhere — inside the isolation directory, or a harness's own isolation started from inside an agent's checkout
- **THEN** it is never adopted as a slot, offered to an agent, swept as idle or removed, and its branch is never pruned
- **AND** nothing the system projects forbids it: work on another branch is the agent's to bring back to its own

#### Scenario: A new checkout is created only when none is free
- **WHEN** every existing checkout is held by a live agent or pinned
- **THEN** a new checkout is created

#### Scenario: A declared cap holds
- **WHEN** a project declares a maximum number of checkouts and all are held by live agents or pinned
- **THEN** no new agent is started, and the operator is told that every checkout is in use and that closing an agent frees one

#### Scenario: Shelved work does not count against the cap
- **WHEN** a project declares a cap of two and two agents ended holding work
- **THEN** a new agent is placed, in one of their checkouts

### Requirement: Nothing that can hold work is removed automatically
The system SHALL NOT remove a working tree holding uncommitted changes, a
branch holding commits absent from its target, nor a shelf holding changes
absent from its target, on any automatic path; shelving the changes first
is how a checkout stops holding them. Whether a branch's work is in the
target SHALL be answered by what the target carries and not by commit
identity alone: every commit reachable from it, or the same patch present
under commits of its own, as a squash merge and a rebase merge each leave
it. A branch whose work is in the target MAY be removed, as MAY the
directory of a free checkout unused for longer than the idle age
`checkout-accounting` keeps, while keeping its branch. A checkout in which
any process is working, or in which a rebase or a merge is paused, SHALL
NOT be removed. Discarding work SHALL happen only on an explicit operator
action naming the task.

#### Scenario: A dirty orphan is preserved, not deleted
- **WHEN** startup finds a checkout with uncommitted changes and no live agent, nothing working in it and no operation paused in it
- **THEN** every change the repository reports is in the task's shelf before anything in the checkout is reset

#### Scenario: An unintegrated branch outlives its checkout
- **WHEN** a free checkout is idle beyond the idle age and its branch has commits absent from the target
- **THEN** the directory may be removed and the branch remains

#### Scenario: An integrated branch is pruned
- **WHEN** every commit of a task's branch is reachable from the target
- **THEN** the branch may be removed without an operator action

#### Scenario: A trimmed checkout's integrated branch goes in the same pass
- **WHEN** a collection removes an idle checkout whose branch holds nothing the target lacks
- **THEN** that branch is removed by the same collection

#### Scenario: A squash-merged branch still counts as integrated
- **WHEN** a branch's work is merged by squashing, so none of its commits is reachable from the target
- **THEN** the branch is read as integrated from the patch the target carries, without asking any forge, and may be pruned
- **AND** the same holds for a rebase merge, which keeps every commit under a new identity

#### Scenario: Only the operator discards
- **WHEN** a shelved task is discarded
- **THEN** the action was taken by the operator on that task, and no automatic path could have taken it

### Requirement: Existing checkouts are adopted at startup
The system SHALL reconcile the isolation directory, the `agent/` branches,
the shelves and its persisted task state when a repository's space starts.
A checkout is taken as the system's own only as `checkout-accounting`
states: by the record it carries, or on sight when a launched agent's
record names it or it bears a legacy `agent-<n>` name. A checkout without a
record SHALL NOT be adopted by inference, whatever its name or place; it is
listed to the operator. A task without a checkout SHALL be marked from what
its branch and its shelf hold. A recorded checkout holding work for a task
whose agent has ended SHALL have that work shelved and SHALL be freed,
unless it is pinned. Stale worktree registry entries SHALL be pruned only
after reconciliation.

#### Scenario: A legacy checkout is adopted
- **WHEN** the isolation directory holds checkouts named `agent-<n>`, created before task state existed
- **THEN** each is adopted as a task labelled from its branch, and no branch is renamed

#### Scenario: A checkout nobody recorded is not adopted
- **WHEN** the isolation directory holds a worktree without the system's record, not named `agent-<n>`, and no launched agent's record names it
- **THEN** it is not adopted, and it is listed to the operator

#### Scenario: Checkouts parked by an earlier build are freed
- **WHEN** a project holds recorded checkouts that an earlier build left holding uncommitted work for agents that have ended
- **THEN** after the first reconciliation each one's work is shelved and each is free, except those pinned

#### Scenario: Prune never runs before adoption
- **WHEN** startup finds a registry entry whose directory is gone
- **THEN** the entry is pruned only after every remaining checkout has been adopted
