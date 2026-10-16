## MODIFIED Requirements

### Requirement: Every worktree of a project is accounted for
The system SHALL account for every worktree the repository registers,
wherever it is on disk, and SHALL classify each by owner: an agent's
checkout, a subagent's checkout (with the agent it belongs to), a
harness's own isolation, or the operator's. A harness's own isolation
SHALL be recognised from where the integration for that harness declares
it keeps worktrees, either relative to any checkout of the project or
under a directory of that harness's own outside the project, including
worktrees a harness creates when the model calls a worktree tool the
harness provides. A worktree no integration claims and UZE did not record
SHALL be the operator's. For each, the system SHALL know its branch,
whether it holds uncommitted changes, whether its work is in the target,
when it was last changed, and whether it is in use.

#### Scenario: A harness's own isolation is named as such
- **WHEN** a harness creates a worktree where its integration declares that harness keeps them, from the primary or from inside a slot
- **THEN** it is accounted for as that harness's, not as the operator's and never as UZE's

#### Scenario: A worktree a harness keeps outside the project
- **WHEN** an agent's model calls the harness's own worktree tool and the harness creates the worktree in a directory of its own outside the project
- **THEN** it is accounted for as that harness's, shown, never driven and never removed

#### Scenario: A worktree outside every known place
- **WHEN** the operator adds a worktree anywhere with Git
- **THEN** it is accounted for as the operator's, with its facts
