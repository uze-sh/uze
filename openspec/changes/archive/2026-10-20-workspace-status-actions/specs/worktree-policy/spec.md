## ADDED Requirements

### Requirement: Delivery runs on an explicit action and only the system writes the target
The system SHALL deliver a ready task only on an explicit operator action, in a project that declares a completion behavior, one task at a time, and never while one of its subagents' checkouts holds work not yet joined (`checkout-accounting`). No agent SHALL write the target branch; the system SHALL write it only in the fast-forward step of `merge`.

#### Scenario: A failed gate leaves the target untouched
- **WHEN** the gate fails on the rebased commits
- **THEN** the task reads as failed, the gate's output is kept where the operator can open it, and the target is unchanged

#### Scenario: A conflict stays paused in the task's checkout
- **WHEN** the rebase stops on conflicts
- **THEN** the rebase stays paused in the task's checkout, the task reads as conflicted, and the target is unchanged
- **AND** the next evaluation reads the task's state from its checkout

#### Scenario: The second task sees the first
- **WHEN** two ready tasks are delivered in sequence
- **THEN** the second is rebased onto a target that already contains the first

#### Scenario: Overlapping uncommitted work in the primary refuses a merge
- **WHEN** the operator has uncommitted changes in the primary checkout to files the task changed
- **THEN** delivery in merge mode is refused and reported, and nothing is written

#### Scenario: Sibling tasks share work only through the target
- **WHEN** one task has been delivered and another live task asks for the target
- **THEN** the second task receives the first's work by rebasing onto the target
- **AND** no task's branch ever carries another task's commits directly, save a subagent's checkout joined into its own agent (`checkout-accounting`)

#### Scenario: A live task follows the target automatically
- **WHEN** the target has moved and a live task's pane goes quiet with a clean working tree
- **THEN** the task's branch is rebased onto the target's tip inside its checkout, under the same rules as delivery
- **AND** a conflict pauses there, reads as conflicted, and the target is unchanged
- **AND** this holds under every completion behavior, from the target this machine already has and without a fetch of its own

#### Scenario: Work the target already holds is delivered, not replayed
- **WHEN** a live task's work reached the target as a squash or a rebase merge, so the target moved but none of the branch's commits is reachable from it
- **THEN** the task is read as delivered from the patch the target carries
- **AND** its branch is not rebased, nothing is sent to its agent, and its checkout is left as the agent left it

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

### Requirement: A completion behavior names where the work is put
`delivery:` SHALL be `merge`, putting the work into the local target, or `push`, putting it on the remote as the task's branch. Both SHALL first rebase the branch onto the target in its checkout and run the gate on the rebased commits, and SHALL write only what passed it. UZE SHALL NOT open a request: that is the agent's, asked through the project's instructions (`status-actions`).

#### Scenario: Push publishes exactly what passed the gate
- **WHEN** the operator delivers a ready task in a project declaring push
- **THEN** the branch is rebased onto the target, the gate passes, the rebased branch is pushed under its readable name, and the task reads as published
- **AND** no request is opened by UZE and the target is not written

#### Scenario: Merge advances the target linearly after the gate
- **WHEN** the operator delivers a ready task in a project declaring merge
- **THEN** the branch is rebased onto the target, the gate passes on the rebased commits, and the target moves to the branch's tip
- **AND** the target's history is linear and the task reads as delivered

#### Scenario: No behavior declared, no delivery
- **WHEN** the operator presses the deliver key in a project whose `agents.yaml` declares no `delivery:`
- **THEN** nothing is rebased, run or written, and the operator is told that the project declares no delivery

### Requirement: The target is read from the remote unless UZE writes it
A `push` delivery SHALL rebase onto the target's tip as fetched from the remote. A `merge` delivery SHALL rebase onto the local target, which it then fast-forwards. Delivery SHALL NOT update the operator's checked-out target except in the fast-forward step of `merge`.

#### Scenario: Push rebases onto the remote's target
- **WHEN** the operator delivers a ready task in a project declaring push and the remote target has moved
- **THEN** the branch is rebased onto the remote target's tip as fetched
- **AND** the operator's local target branch and primary checkout are not written

#### Scenario: Merge delivers onto the local target
- **WHEN** the operator delivers a ready task in a project declaring merge
- **THEN** the branch is rebased onto the local target branch's tip
- **AND** the operator's checked-out target moves only in the final fast-forward

## MODIFIED Requirements

### Requirement: The workspace client shows the policy in effect and where it came from
The agent context popup SHALL name the completion behavior, the delivery
target and the gate command in force for the repository, and SHALL state
whether they come from the project's manifest or from the built-in
default. A policy that has never been declared SHALL read as the default,
not as absent.

#### Scenario: An undeclared policy reads as the default, attributed
- **WHEN** the popup opens in a repository whose manifest declares no
  policy
- **THEN** it shows that no delivery is declared, attributed to the
  default rather than to the project

#### Scenario: A declared policy is attributed to the manifest
- **WHEN** the popup opens in a repository declaring `workspace.delivery: push`
- **THEN** it shows `push`, attributed to `agents.yaml`

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
- **WHEN** the operator commits the primary checkout's `agents.yaml` and `AGENTS.md` on the branch isolated checkouts are cut from (the declared target, else the primary checkout's branch), and pushes it when the repository has a remote
- **THEN** an agent placed in an isolated checkout afterwards reads the current region

#### Scenario: A project with no AGENTS.md
- **WHEN** the workspace synchronizes a project that declares a policy and has no `AGENTS.md`
- **THEN** no `AGENTS.md` is created, and the operator's checkout is left as it was

#### Scenario: A declaration changed through the workspace reaches the file
- **WHEN** the operator changes the policy through the workspace rather than by editing `agents.yaml`
- **THEN** the shared instruction file carries the new region without any command being run

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
- **WHEN** a task nobody named is delivered by a push delivery
- **THEN** the pushed branch is named from the task's first commit
- **AND** the task's identifier, checkout and state are unchanged

#### Scenario: State survives the checkout
- **WHEN** a task's checkout directory is removed
- **THEN** the task's state and transcript are still available

#### Scenario: An interrupted write leaves a valid state file
- **WHEN** the process is killed while task state is being written
- **THEN** the state file on disk is either the previous version or the new one, never truncated

## REMOVED Requirements

### Requirement: Delivery follows the declared completion and only the system writes the target
**Reason**: Its scenarios promised fixed messages to the agent and a `pr` behavior that pushed; both go with this change.
**Migration**: Replaced by "Delivery runs on an explicit action and only the system writes the target"; what the agent is told is the project's `workspace.actions` (`status-actions`).

### Requirement: Each completion behavior delivers in one way
**Reason**: `pr` is renamed `push` and no longer asks for a request; `handoff` goes, since validating alone was a step internal to delivery.
**Migration**: Replaced by "A completion behavior names where the work is put"; `pr` becomes `push` with an `on_publish` instruction asking for the request; `handoff` becomes no `delivery:` at all.

### Requirement: Delivery reads the target from where it lives
**Reason**: Restated for the renamed behaviors.
**Migration**: Replaced by "The target is read from the remote unless UZE writes it".
