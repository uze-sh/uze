## MODIFIED Requirements

### Requirement: The declaration is projected without triggering foreign isolation
The system SHALL render a project's declaration into a marker-owned managed
region of the shared instruction file. The region SHALL open by stating that
it applies to an agent the workspace launched, and that an agent started any
other way can ignore it. It SHALL then state the isolation layout, that a
reader inside an isolated checkout is already isolated and commits on its
own branch and never on the target, that a reader anywhere else in the
project is on the operator's branch and commits there without switching,
resetting or stashing it, that delivery is performed by the system for
isolated work, and that a subagent's checkout is asked for, joined and
listed through the agent's `work` verbs rather than made with Git.
The region SHALL carry nothing that is not about the workspace, and in
particular nothing about authoring plugins.
The rendering SHALL be deterministic, and SHALL NOT instruct any reader to
create a top-level worktree.

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

### Requirement: Changing the policy from the popup is an explicit, versioned edit
The popup SHALL let the completion behavior be changed directly, and that
change SHALL write the project's manifest. Because the manifest is a
tracked file whose policy is projected into `AGENTS.md`, the popup SHALL
say so before writing — naming the two files the change will write — rather
than editing tracked files silently. The workspace SHALL then bring
`AGENTS.md` in step itself.

#### Scenario: Changing the behavior writes the manifest
- **WHEN** a completion behavior is chosen in the popup
- **THEN** `agents.yaml` records it, created first if the project had none

#### Scenario: The consequence is stated before the write
- **WHEN** a completion behavior is chosen
- **THEN** the popup names `agents.yaml` and `AGENTS.md` as the files that will change

#### Scenario: The projected text is not silently left stale
- **WHEN** the policy has changed from the popup
- **THEN** `AGENTS.md` carries the new region without a further command
- **AND** a region that could not be written (edited by hand) is reported by the workspace client

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

### Requirement: A policy change binds tasks created after it
A policy change SHALL apply to tasks created after it. A task already
running SHALL keep the policy it was launched under, because its agent was
placed, and told how its work is delivered, under that policy.

#### Scenario: A running agent keeps its launch policy
- **WHEN** the completion behavior changes while a task is live
- **THEN** that task is delivered the way it was launched, and the popup
  shows the running task's behavior alongside the new project default

#### Scenario: The next task takes the new policy
- **WHEN** a task is created after the change
- **THEN** it is delivered the new way

## ADDED Requirements

### Requirement: The workspace keeps its region in step while it runs
While the workspace is running on a project, the system SHALL keep the
shared instruction file in the project's primary checkout carrying the
region its current declaration renders, without the operator asking: when
a space opens on the project, when the declaration changes (from the popup
or by an edit to `agents.yaml`), and before an agent starts in the primary
checkout. The system SHALL NOT write the region into an isolated checkout:
there the file is part of the agent's branch, and what the agent reads is
the region its branch was cut with, which the operator brings forward by
committing the primary checkout's file like the declaration itself. The
system SHALL keep the region only in an `AGENTS.md` the project has, and
SHALL NOT create the file: a project without one has given its agents no
instructions yet, and `uze install` is what creates it. When an
agent is isolated with a copy of the primary checkout's changes, a change to
`AGENTS.md` that lies only inside UZE's managed regions SHALL NOT be copied.
A region edited by hand SHALL be reported once per client session and
region, left untouched, and never removed. The synchronization SHALL never
hold up what the workspace client draws.

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

#### Scenario: Isolating an agent does not carry the synchronized region
- **WHEN** an agent in the primary checkout is isolated with a copy of its changes while `AGENTS.md` differs from the last commit only inside UZE's managed regions
- **THEN** the isolated checkout's `AGENTS.md` is the committed one
- **AND** every other change, `agents.yaml` included, is carried as before

#### Scenario: A project with no AGENTS.md
- **WHEN** the workspace synchronizes a project that declares a policy and has no `AGENTS.md`
- **THEN** no `AGENTS.md` is created, and the operator's checkout is left as it was

#### Scenario: A hand-edited region
- **WHEN** the region has been changed by hand and the workspace synchronizes
- **THEN** the workspace reports the drift once and leaves the file's bytes untouched
