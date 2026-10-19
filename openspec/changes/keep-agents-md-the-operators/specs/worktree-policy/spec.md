## REMOVED Requirements

### Requirement: The declaration is projected without triggering foreign isolation
**Reason**: The policy applies only to an agent the workspace launched, which
now receives it through its launch context (`launch-instructions`).
**Migration**: `uze install` in the primary checkout removes the region
(`project-footprint`). The rendering is the ADDED requirement below.

### Requirement: A declaration stays editable
**Reason**: With no region there is nothing to supersede or drift; every
launch renders the declaration in force.
**Migration**: None. Edit `agents.yaml`: the next launch reads it.

### Requirement: The workspace keeps its region in step while it runs
**Reason**: There is no region to keep in step.
**Migration**: None. The next launch or resume renders the current
declaration.

### Requirement: A hand-edited region is reported and left alone
**Reason**: The workspace no longer synchronizes a region. A retired region
edited by hand is reported by `uze install` (`project-footprint`).
**Migration**: None.

## MODIFIED Requirements

### Requirement: Changing the policy from the popup is an explicit, versioned edit
The popup SHALL let the completion behavior be changed directly, and that
change SHALL write the project's manifest. Because the manifest is a
tracked file, the popup SHALL say so before writing, naming the file it
will change and that agents launched afterwards take the new policy,
rather than editing a tracked file silently.

#### Scenario: Changing the behavior writes the manifest
- **WHEN** a completion behavior is chosen in the popup
- **THEN** `agents.yaml` records it as `workspace.delivery`, created first if the project had none

#### Scenario: The consequence is stated before the write
- **WHEN** a completion behavior is chosen
- **THEN** the popup names `agents.yaml` as the file being changed and that agents launched afterwards take the new policy, and names no other file

#### Scenario: The projected text is not silently left stale
- **WHEN** the primary checkout's `AGENTS.md` still carries a region an earlier UZE wrote
- **THEN** the workspace client reports it, naming `uze install` as what removes it

### Requirement: Isolating an agent leaves the synchronized region behind
When an agent is isolated with a copy of the primary checkout's changes, a
change to `AGENTS.md` that only removes regions an earlier UZE wrote SHALL
NOT be copied; every other change is carried.

#### Scenario: Only the synchronized region stays behind
- **WHEN** an agent in the primary checkout is isolated while `AGENTS.md` differs from the last commit by a retired region's removal and by an edit outside it
- **THEN** the isolated checkout carries the edit outside the region and not the removal

#### Scenario: Isolating an agent does not carry the synchronized region
- **WHEN** an agent in the primary checkout is isolated while `AGENTS.md` differs from the last commit only by retired regions' removal
- **THEN** the isolated checkout's `AGENTS.md` is the committed one
- **AND** every other change, `agents.yaml` included, is carried as before

## ADDED Requirements

### Requirement: The declaration is rendered for the launch without triggering foreign isolation
The system SHALL render a project's effective declaration deterministically
as the launch-instructions section: where the reader commits, that the
system delivers isolated work, and how a subagent gets a checkout. It SHALL
carry nothing but the workspace, and SHALL NOT tell any reader to create a
top-level worktree.

#### Scenario: Rendering is stable
- **WHEN** an agent is launched twice under the same declaration, in the same checkout, on the same branch
- **THEN** its instructions section has the same bytes both times

#### Scenario: The rendering does not activate a harness's own worktree mechanism
- **WHEN** the section is read by a harness that creates worktrees when instructed to
- **THEN** it finds no instruction to create a top-level worktree
- **AND** it is told it is already isolated

#### Scenario: The rendering keeps the target for the system
- **WHEN** a project declares any completion behavior
- **THEN** the section states that behavior, that an isolated reader commits on its own branch, and that the target is written by the system

#### Scenario: The section says nothing about plugins
- **WHEN** a project declares a policy with a work-naming vocabulary
- **THEN** the section carries no instruction about creating, checking or installing plugins or marketplaces

#### Scenario: The section states the subagent verbs
- **WHEN** an isolated agent is launched
- **THEN** its section states that delivery of isolated work is performed by the system
- **AND** that a subagent's checkout is asked for, joined and listed through the agent's `work` verbs, never made with Git

#### Scenario: Editing the declaration reaches the next launch
- **WHEN** a project's declaration changes between two launches of one agent
- **THEN** the second launch's section renders the new declaration and no tracked file is written by the launch
