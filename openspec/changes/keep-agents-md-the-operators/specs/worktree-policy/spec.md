## REMOVED Requirements

### Requirement: The declaration is projected without triggering foreign isolation
**Reason**: The policy only applies to an agent the workspace launched, and
that agent now receives it through its launch context
(`launch-instructions`). A region of `AGENTS.md` reached every other reader
too, and a project that stopped using UZE was left with it.
**Migration**: `uze install` or `uze agent context reconcile` removes the
region (`project-footprint`). What the region said is the launch-instructions
section, under `### Requirement: The declaration is rendered for the launch
without triggering foreign isolation` below.

### Requirement: A declaration stays editable
**Reason**: With no region there is nothing to supersede or to drift. The
launch section is composed again on every launch from the declaration in
force.
**Migration**: None. Edit `agents.yaml`: the next launch reads it.

## ADDED Requirements

### Requirement: The declaration is rendered for the launch without triggering foreign isolation
The system SHALL render a project's effective declaration deterministically
as the launch-instructions section: where the reader commits, that the
system delivers isolated work, and how a subagent gets a checkout. It SHALL
carry nothing but the workspace, and SHALL NOT tell any reader to create a
top-level worktree.

#### Scenario: Rendering is stable
- **WHEN** the same agent is launched twice under the same declaration
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
- **THEN** the second launch's section renders the new declaration and nothing is written into the checkout
