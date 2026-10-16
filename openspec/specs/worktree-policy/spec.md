# worktree-policy Specification

## Purpose
Keeps every agent the workspace launches in an isolated checkout of its own, reuses those checkouts as slots without ever discarding work on an automatic path, and lets work reach the target branch only through the system, as the project declares it.

## Requirements

### Requirement: Every agent is isolated and the primary checkout belongs to the operator
The system SHALL start every agent it launches into a worktree space (see
the `space-kinds` capability) in an isolated checkout of its own, created
before the agent's process starts, without asking the operator and without
requiring any harness to cooperate. The primary checkout SHALL never be
assigned to an agent of a worktree space. Where a slot cannot be acquired
the system SHALL NOT start the agent and SHALL state the reason.

#### Scenario: The first agent is isolated
- **WHEN** an operator creates an agent in a worktree space over a repository with no other agent running
- **THEN** it starts in an isolated checkout on its own branch, not in the primary checkout

#### Scenario: Concurrent agents never share a working tree
- **WHEN** three agents are live in one worktree space
- **THEN** each works in a distinct checkout and none of them is the primary

#### Scenario: The operator's uncommitted work is untouched by agents
- **WHEN** the operator has uncommitted changes in the primary checkout and agents of a worktree space run to completion
- **THEN** the primary checkout's working tree and index are exactly what the operator left

#### Scenario: A terminal that is not an agent runs where the operator is
- **WHEN** the operator opens a shell tab
- **THEN** it starts in the space's root and no checkout is created

#### Scenario: A harness the operator starts by hand is shown as unmanaged
- **WHEN** the operator runs a harness inside a shell tab
- **THEN** the tab is listed with the harness's name and its real directory, and carries no task, checkout, state or delivery action
- **AND** nothing is evaluated for it

#### Scenario: A slot that cannot be acquired refuses the launch
- **WHEN** an agent is created in a worktree space and no slot can be acquired for it
- **THEN** no agent starts in the space's root
- **AND** the operator is told why

#### Scenario: An agent of a workspace space works in the root
- **WHEN** the operator creates an agent in a workspace space
- **THEN** it starts in the space's root, by the operator's deliberate choice, and no checkout is created for it

### Requirement: Isolated checkouts are reusable slots
The system SHALL keep isolated checkouts under the primary checkout's fixed
isolation directory and SHALL reuse a free checkout before creating
another, so their number is bounded by peak concurrency; a project MAY
declare a cap. Reuse SHALL put the working tree at the task's base with no
tracked or untracked file from the previous task, and SHALL preserve
ignored files.

#### Scenario: A free checkout is reused
- **WHEN** a task ends with its checkout clean and a new agent is created
- **THEN** the new agent starts in that checkout, on a new branch from the base
- **AND** ignored files such as build caches remain in place

#### Scenario: A previous task's edits never reach the next
- **WHEN** a checkout whose last task was delivered is reused
- **THEN** the working tree matches the base and carries none of the previous branch's edits

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

### Requirement: A checkout holding work is never reused
A checkout holding uncommitted changes, or commits absent from its base,
SHALL never be reused; uncommitted changes confined to content the system
derives are not work (`checkout-accounting`). A checkout in which any
process is working SHALL never be reused.

#### Scenario: A checkout holding work is never reused
- **WHEN** a task's agent is gone and its checkout has uncommitted changes or commits absent from its base
- **THEN** the checkout is parked, listed to the operator, and not offered to a new agent

### Requirement: An agent's checkout stands at the agent's own path
Each isolated checkout SHALL stand at the path its agent owns, so an agent
finds it at the same path every time it is placed: the directory it last
held, or its own identifier when it never held one. A reused checkout SHALL
be renamed to that path with Git's own worktree move, preferring one whose
last agent will not come back. A checkout Git refuses to move, or one any
process works in, SHALL NOT be renamed nor stand in the way of creating one
at the agent's path; nothing SHALL be copied.

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

### Requirement: Nothing that can hold work is removed automatically
The system SHALL NOT remove a checkout where a process is working, nor, on
any automatic path, a working tree with uncommitted changes or a branch with
commits absent from its target. A branch whose commits, or their patches,
the target carries SHALL count as integrated whatever the commit identities,
and MAY be removed, as MAY a free checkout's directory past the spare slots
(`checkout-accounting`), keeping its branch. Only an explicit operator
action naming the task SHALL discard work.

#### Scenario: A dirty orphan is preserved, not deleted
- **WHEN** startup finds a checkout with uncommitted changes and no live agent
- **THEN** the checkout is parked and every file in it is preserved

#### Scenario: An unintegrated branch outlives its checkout
- **WHEN** a clean checkout is idle beyond the declared age and its branch has commits absent from the target
- **THEN** the directory may be removed and the branch remains

#### Scenario: An integrated branch is pruned
- **WHEN** every commit of a task's branch is reachable from the target
- **THEN** the branch may be removed without an operator action

#### Scenario: A squash-merged branch still counts as integrated
- **WHEN** a branch's work is merged by squashing, so none of its commits is reachable from the target
- **THEN** the branch is read as integrated from the patch the target carries, without asking any forge, and may be pruned
- **AND** the same holds for a rebase merge, which keeps every commit under a new identity

#### Scenario: Only the operator discards
- **WHEN** a parked task is discarded
- **THEN** the action was taken by the operator on that task, and no automatic path could have taken it

### Requirement: A task's identity is immutable and its name is derived
The system SHALL give every agent launch a generated task identifier that
never changes, keying its checkout and persisted state. The branch SHALL
start as that identifier under `agent/`; branch and label SHALL then be
replaced once while still generated and never overwritten afterwards
(`agent-work-naming`). A name derived at publish time SHALL be only the
fallback for a task nobody named. Task state SHALL be persisted outside
every checkout and written atomically.

#### Scenario: The branch starts from the identifier
- **WHEN** an agent is created
- **THEN** its branch is the identifier under the `agent/` prefix, and its
  label is the generated one until the work is named

#### Scenario: A published branch carries a readable name
- **WHEN** a task nobody named is delivered by opening a pull request
- **THEN** the pushed branch is named from the task's first commit
- **AND** the task's identifier, checkout and state are unchanged

#### Scenario: State survives the checkout
- **WHEN** a task's checkout directory is removed
- **THEN** the task's state and transcript are still available

#### Scenario: An interrupted write leaves a valid state file
- **WHEN** the process is killed while task state is being written
- **THEN** the state file on disk is either the previous version or the new one, never truncated

### Requirement: Readiness is observed, never declared
The system SHALL decide whether a task is ready to deliver from the state of
its checkout: ready when its branch has commits absent from the base and the
working tree is clean. It SHALL evaluate that state when the agent's pane
goes quiet and whenever the operator asks, and SHALL NOT rely on the agent
announcing completion. An end-of-turn signal from a harness, where one is
delivered, MAY trigger an evaluation but SHALL NOT be required.

#### Scenario: Commits and a clean tree read as ready
- **WHEN** the agent's pane goes quiet with commits on the task's branch and a clean working tree
- **THEN** the task is reported ready and delivery is offered

#### Scenario: Uncommitted changes are surfaced, not delivered
- **WHEN** the agent's pane goes quiet with uncommitted changes in the task's checkout
- **THEN** the tab reports uncommitted work and delivery is not offered

#### Scenario: A quiet pane that resumes is not stuck as ready
- **WHEN** a task was reported ready and its agent produces further changes
- **THEN** the next evaluation reflects the checkout's current state

### Requirement: An agent is placed on the target as the last sync left it
The system SHALL fast-forward an isolating project's local target onto
the remote's on the workspace's clock, and SHALL NOT ask the remote while
placing an agent: a new agent's branch SHALL be cut from
the most advanced tip of the target the last sync left, the remote's
tracking ref when it descends from the local target, else the local
target. The agent's base SHALL name the declared target and its base commit
the commit it was cut from. A delivery SHALL fetch the target itself.

#### Scenario: A placement does not wait on the remote
- **WHEN** the remote's target has moved since the last sync and an agent is created
- **THEN** the agent's branch is cut from the tips the last sync left without contacting the remote

#### Scenario: A new agent starts from what the remote has
- **WHEN** a sync ran after the remote's target moved and the target is not checked out in the primary checkout
- **THEN** the local target is fast-forwarded onto the remote's tip and the next agent's branch is cut from there

#### Scenario: A repository with no remote is placed from its own target
- **WHEN** an agent is created in a repository with no remote
- **THEN** the agent is placed from the local target and nothing is reported

### Requirement: The sync never rewrites a target it cannot fast-forward
A target carrying commits the remote lacks, one Git refuses to move, and one
checked out in the primary checkout SHALL be left exactly as they stand. The
system SHALL say once, until the target catches up, only when a new agent
would start behind the remote's tip, because the local target carries
commits the remote lacks, and SHALL say nothing when the placement is cut
from the remote's tip anyway.

#### Scenario: A new agent starts from the remote while the operator stands on the target
- **WHEN** the operator has the target checked out in the primary checkout, the remote has moved, and a sync ran
- **THEN** the sync leaves the target and the working tree as they stand, says nothing, and the next agent's branch is cut from the remote's tip

#### Scenario: The operator's own commits are never rewritten to sync
- **WHEN** the local target carries commits the remote does not and the remote has moved
- **THEN** the local target is left where it stands, the next agent's branch is cut from it, and the system says once how far behind the remote the local target is

### Requirement: An agent moved out of the operator's checkout continues from its HEAD
An agent moved into a checkout of its own from the primary checkout SHALL
be cut from the primary checkout's `HEAD`, whatever the target's tips say.

#### Scenario: An agent moved into a checkout of its own continues the operator's work
- **WHEN** an agent in the primary checkout is moved into a checkout of its own while the remote's tip is ahead of the local target
- **THEN** its branch is cut from the primary checkout's `HEAD`, not from the remote's tip

### Requirement: An agent's work is judged against the target it was cut from
Every judgement the system makes about an isolated agent's work SHALL be
asked against the most advanced of the local target and the remote's
tracking ref for it that this machine already has; when the two have
diverged, against the one the agent's branch descends from, and the local
target when it descends from neither. The same rule SHALL hold for every
agent, however it was placed, and asking it SHALL NOT contact the remote.

#### Scenario: Every judgement asks the same target
- **WHEN** the system asks whether an agent's branch is in the target, whether its checkout, branch or shelf still holds work the target lacks, how many commits are its own, or what a refresh rebases it onto
- **THEN** each question is asked against the same target ref, chosen by this rule

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

### Requirement: Delivery follows the declared completion and only the system writes the target
The system SHALL deliver a ready task only on an explicit operator action,
one task at a time, according to the project's declared completion behavior,
and never while one of its subagents' checkouts holds work not yet joined
(`checkout-accounting`). No agent SHALL write the target branch; the system
SHALL write it only in the fast-forward step of `merge`.

#### Scenario: A gate failure leaves the target untouched
- **WHEN** the gate fails on the rebased commits
- **THEN** the task is returned to its agent with the gate's output and the target is unchanged

#### Scenario: A conflict is returned to the agent that owns the task
- **WHEN** the rebase stops on conflicts
- **THEN** the rebase stays paused in the task's checkout, the agent is told which files conflict and how far the target moved, and the target is unchanged
- **AND** the next evaluation reads the task's state from its checkout

#### Scenario: The second task sees the first
- **WHEN** two ready tasks are delivered in sequence
- **THEN** the second is rebased onto a target that already contains the first

#### Scenario: Overlapping uncommitted work in the primary refuses delivery
- **WHEN** the operator has uncommitted changes in the primary checkout to files the task changed
- **THEN** delivery in merge mode is refused and reported, and nothing is written

#### Scenario: In pr mode the target is the remote's
- **WHEN** a task is rebased or delivered in a project declaring pr and the remote target has moved
- **THEN** the branch is rebased onto the remote target's tip as fetched
- **AND** delivery does not modify the operator's local target branch or primary checkout

#### Scenario: Sibling tasks share work only through the target
- **WHEN** one task has been delivered and another live task asks for the target
- **THEN** the second task receives the first's work by rebasing onto the target
- **AND** no task's branch ever carries another task's commits directly, save a subagent's checkout joined into its own agent (`checkout-accounting`)

#### Scenario: A live task follows the target automatically
- **WHEN** the target has moved and a live task's pane goes quiet with a clean working tree
- **THEN** the task's branch is rebased onto the target's tip inside its checkout, under the same rules as delivery
- **AND** a conflict is returned to the agent and the target is unchanged
- **AND** this holds under every completion behavior, from the local target and without a fetch of its own

#### Scenario: Work the target already holds is delivered, not replayed
- **WHEN** a live task's work reached the target as a squash or a rebase merge, so the target moved but none of the branch's commits is reachable from it
- **THEN** the task is read as integrated from the patch the target carries
- **AND** its branch is not rebased, nothing is returned to its agent, and its checkout is left as the agent left it

#### Scenario: Work after a delivery is moved alone
- **WHEN** an agent keeps committing on a branch whose earlier work reached the target as a squash or a rebase merge, and the target moves
- **THEN** only the commits made after the delivery are rebased onto the target
- **AND** the task counts only those commits as what is left to deliver

#### Scenario: A request number answers for its own branch
- **WHEN** the branch a task is published under changes, or the task takes on new work after a delivery
- **THEN** the request number recorded for the earlier branch or work is dropped, and the remote is asked again for the current one

#### Scenario: A task mid-edit is not rebased under its agent
- **WHEN** the target has moved while a live task's working tree is dirty
- **THEN** the task is left as it is until its tree is clean and its pane quiet

### Requirement: Each completion behavior delivers in one way
`handoff` SHALL leave the branch for the operator. `merge` SHALL rebase the
task's branch onto the target's tip inside the task's checkout, run the
project's declared gate on the rebased commits, and advance the target by
fast-forward only. `pr` SHALL publish the branch under its readable name and
open a pull request against the target.

#### Scenario: Handoff never touches the target
- **WHEN** the operator delivers a ready task in a project declaring handoff
- **THEN** the branch is reported as ready for the operator and the target is unchanged

#### Scenario: Merge advances the target linearly after the gate
- **WHEN** the operator delivers a ready task in a project declaring merge
- **THEN** the branch is rebased onto the target, the gate passes on the rebased commits, and the target moves to the branch's tip
- **AND** the target's history is linear

#### Scenario: A pull request targets the declared branch
- **WHEN** the operator delivers a ready task in a project declaring pr
- **THEN** the branch is pushed under its readable name and a pull request against the target is opened

### Requirement: Delivery reads the target from where it lives
The target's tip SHALL be taken from the remote-tracking branch after a
fetch when delivery publishes a pull request, and from the local branch
otherwise. Delivery SHALL NOT update the operator's checked-out target
except in the fast-forward step of `merge`; the only other write to it is
the fast-forward that brings it in line with the remote before an agent is
placed.

#### Scenario: Merge mode delivers onto the local target
- **WHEN** the operator delivers a ready task in a project declaring merge
- **THEN** the branch is rebased onto the local target branch's tip
- **AND** the operator's checked-out target moves only in the final fast-forward

#### Scenario: Pr mode leaves the operator's target alone
- **WHEN** the operator delivers a ready task in a project declaring pr
- **THEN** the target's tip is read from the remote-tracking branch after a fetch
- **AND** the operator's local target branch is not written

### Requirement: Existing checkouts are adopted at startup
The system SHALL reconcile the isolation directory, `agent/` branches and
task state when a repository's space starts. A checkout is UZE's only as
`checkout-accounting` states: by its record, or on sight when a launched
agent's record names it or it bears a legacy `agent-<n>` name. Any other
SHALL NOT be adopted by inference, and is listed to the operator. A task
without a checkout SHALL be marked from where its branch stands. Stale
registry entries SHALL be pruned only after reconciliation.

#### Scenario: A legacy checkout is adopted
- **WHEN** the isolation directory holds checkouts named `agent-<n>`, created before task state existed
- **THEN** each is adopted as a task labelled from its branch, and no branch is renamed

#### Scenario: A checkout nobody recorded is not adopted
- **WHEN** the isolation directory holds a worktree without the system's record, not named `agent-<n>`, and no launched agent's record names it
- **THEN** it is not adopted, and it is listed to the operator

#### Scenario: Prune never runs before adoption
- **WHEN** startup finds a registry entry whose directory is gone
- **THEN** the entry is pruned only after every remaining checkout has been adopted

### Requirement: The declaration is projected without triggering foreign isolation
The system SHALL render a project's declaration deterministically into a
marker-owned managed region of the shared instruction file. The region SHALL
open by stating that it applies only to an agent the workspace launched,
then state the isolation layout, where each reader commits, that the system
delivers isolated work, and how a subagent gets a checkout. It SHALL carry
nothing but the workspace, and SHALL NOT tell any reader to create a
top-level worktree.

#### Scenario: Reconciling a declaration projects it
- **WHEN** the workspace is running on a project that declares an isolation policy
- **THEN** the shared instruction file carries the declaration in a managed region
- **AND** a second synchronization changes no bytes

#### Scenario: A project declaring nothing carries no region
- **WHEN** the workspace synchronizes a project that declares no isolation policy
- **THEN** no region is written and none is reported, and a region an earlier policy left is removed

#### Scenario: The package manager leaves the region alone
- **WHEN** a person runs `uze install`, `uze update` or `uze agent context reconcile` on a project that declares a policy
- **THEN** the workspace's region is neither written, removed, nor counted as a pending change

#### Scenario: The projection does not activate a harness's own worktree mechanism
- **WHEN** the projected text is read by a harness that creates worktrees when instructed to
- **THEN** it finds no instruction to create a top-level worktree
- **AND** it is told it is already isolated

#### Scenario: The projected text keeps the target for the system
- **WHEN** a project declares any completion behavior
- **THEN** the projected text states that behavior, that an isolated reader commits on its own branch, and that the target is written by the system

#### Scenario: The projected text addresses the reader on the operator's branch
- **WHEN** the projected text is read by an agent standing in the project's root rather than in an isolated checkout
- **THEN** it is told it is on the operator's branch, to commit there, and never to switch, reset or stash it

#### Scenario: An agent the workspace did not launch
- **WHEN** the projected text is read by an agent a person started by hand
- **THEN** the text's first statement tells it the region does not apply to it
- **AND** no instruction before that statement asks it to run a command that needs a launched agent

#### Scenario: The region says nothing about plugins
- **WHEN** a project declares a policy with a work-naming vocabulary
- **THEN** the projected region carries no instruction about creating, checking or installing plugins or marketplaces

#### Scenario: The projected text states the layout and the subagent verbs
- **WHEN** a project declares an isolation policy
- **THEN** the projected text states the isolation layout and that delivery of isolated work is performed by the system
- **AND** it states that a subagent's checkout is asked for, joined and listed through the agent's `work` verbs, never made with Git

### Requirement: A declaration stays editable
The system SHALL key the projected region's identity on the rendered
content, so changing the declaration supersedes one region and creates
another rather than drifting the existing one. A region edited by hand SHALL
be reported as drifted and left untouched.

#### Scenario: Editing the declaration replaces its region
- **WHEN** a project's declaration changes and the workspace synchronizes
- **THEN** the superseded region is removed and the new one attached
- **AND** exactly one region remains

#### Scenario: An edited region is refused, not rewritten
- **WHEN** the region's content has been changed by hand
- **THEN** the synchronization reports drift and leaves the file's bytes untouched

### Requirement: The workspace client shows the policy in effect and where it came from
The agent context popup SHALL name the completion behavior, the delivery
target and the gate command in force for the repository, and SHALL state
whether they come from the project's manifest or from the built-in
default. A policy that has never been declared SHALL read as the default,
not as absent.

#### Scenario: An undeclared policy reads as the default, attributed
- **WHEN** the popup opens in a repository whose manifest declares no
  policy
- **THEN** it shows the default completion behavior, attributed to the
  default rather than to the project

#### Scenario: A declared policy is attributed to the manifest
- **WHEN** the popup opens in a repository declaring `workspace.delivery: pr`
- **THEN** it shows `pr`, attributed to `agents.yaml`

### Requirement: Changing the policy from the popup is an explicit, versioned edit
The popup SHALL let the completion behavior be changed directly, and that
change SHALL write the project's manifest. Because the manifest is a
tracked file whose policy is projected into `AGENTS.md`, the popup SHALL
say so before writing — naming the file it will change and the
reconciliation the change still needs — rather than editing a tracked
file silently.

#### Scenario: Changing the behavior writes the manifest
- **WHEN** a completion behavior is chosen in the popup
- **THEN** `agents.yaml` records it as `workspace.delivery`, created first if the project had none

#### Scenario: The consequence is stated before the write
- **WHEN** a completion behavior is chosen
- **THEN** the popup names the file being changed and that `AGENTS.md`
  needs reconciliation to carry the new text

#### Scenario: The projected text is not silently left stale
- **WHEN** the policy has changed and `AGENTS.md` still carries the
  previous projection
- **THEN** the workspace client reports the projection as out of date

### Requirement: A policy change binds tasks created after it
A policy change SHALL apply to tasks created after it. A task already
running SHALL keep the policy it was launched under, because its agent was
placed, and told how its work is delivered, under that policy.

#### Scenario: A running agent keeps its launch policy
- **WHEN** the completion behavior changes while a task is live
- **THEN** that task is delivered the way it was launched, and the
  workspace shows the running task's own behavior

#### Scenario: The next task takes the new policy
- **WHEN** a task is created after the change
- **THEN** it is delivered the new way

### Requirement: The workspace keeps its region in step while it runs
While the workspace runs on a project, the system SHALL keep the primary
checkout's shared instruction file carrying the region its current
declaration renders, unasked: when a space opens on it, when the declaration
changes, and before an agent starts in the primary checkout. It SHALL NOT
write it into an isolated checkout, whose file is part of the agent's
branch, and SHALL keep it only in an `AGENTS.md` the project has, never
creating one, and SHALL never hold up what the client draws.

#### Scenario: An edit to agents.yaml reaches the file
- **WHEN** the operator edits the policy in `agents.yaml` while the workspace is running
- **THEN** the shared instruction file carries the new region without any command being run

#### Scenario: An agent in the primary checkout reads the current declaration
- **WHEN** an agent is launched in the primary checkout after the declaration changed
- **THEN** the shared instruction file there carries the current region before the agent starts

#### Scenario: The workspace never writes into an isolated checkout
- **WHEN** an agent is placed in, or is running in, an isolated checkout after the declaration changed
- **THEN** that checkout's shared instruction file is not written by the synchronization
- **AND** nothing the synchronization did can reach the agent's branch or its delivery

#### Scenario: A committed declaration reaches the next isolated agent
- **WHEN** the operator commits the primary checkout's `agents.yaml` and `AGENTS.md` on the branch isolated checkouts are cut from (the declared target, else the primary checkout's branch), and pushes it when completion is `pr`
- **THEN** an agent placed in an isolated checkout afterwards reads the current region

#### Scenario: A project with no AGENTS.md
- **WHEN** the workspace synchronizes a project that declares a policy and has no `AGENTS.md`
- **THEN** no `AGENTS.md` is created, and the operator's checkout is left as it was

#### Scenario: A declaration changed through the workspace reaches the file
- **WHEN** the operator changes the policy through the workspace rather than by editing `agents.yaml`
- **THEN** the shared instruction file carries the new region without any command being run

### Requirement: Isolating an agent leaves the synchronized region behind
When an agent is isolated with a copy of the primary checkout's changes, a
change to `AGENTS.md` that lies only inside UZE's managed regions SHALL NOT
be copied; every other change is carried.

#### Scenario: Only the synchronized region stays behind
- **WHEN** an agent in the primary checkout is isolated while `AGENTS.md` differs from the last commit outside UZE's managed regions as well as inside them
- **THEN** the isolated checkout carries the change outside the regions and not the change inside them

#### Scenario: Isolating an agent does not carry the synchronized region
- **WHEN** an agent in the primary checkout is isolated with a copy of its changes while `AGENTS.md` differs from the last commit only inside UZE's managed regions
- **THEN** the isolated checkout's `AGENTS.md` is the committed one
- **AND** every other change, `agents.yaml` included, is carried as before

### Requirement: A hand-edited region is reported and left alone
A region edited by hand SHALL be reported once per client session and file,
left untouched, and never removed.

#### Scenario: A hand-edited region
- **WHEN** the region has been changed by hand and the workspace synchronizes
- **THEN** the workspace reports the drift once and leaves the file's bytes untouched

#### Scenario: A hand-edited region is reported once
- **WHEN** the workspace synchronizes the same hand-edited region again in one client session
- **THEN** the drift is not reported a second time and the region is still not removed
