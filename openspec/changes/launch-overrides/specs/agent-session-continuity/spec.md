## MODIFIED Requirements

### Requirement: The operator's own invocation is never rewritten
The system SHALL leave an invocation the operator composed exactly as
typed. Continuity SHALL apply only where the operator asked for nothing
about sessions themselves: an invocation carrying the harness's own session
or resume arguments SHALL be passed through untouched, and an invocation
made outside a managed task SHALL behave exactly as it did before this
capability existed. A launch the workspace composes with overrides SHALL
NOT carry an argument its harness's catalog marks as a session or resume
argument, so its continuity stays UZE's.

#### Scenario: An explicit session argument wins
- **WHEN** a harness is started inside a managed task with its own resume or session argument
- **THEN** that argument decides the conversation and UZE adds nothing

#### Scenario: Outside a managed task nothing changes
- **WHEN** a harness is started in a directory that belongs to no managed task
- **THEN** it starts exactly as it would without UZE, with no conversation carried over

#### Scenario: Overrides never turn continuity off
- **WHEN** an agent is launched from the workspace with overrides
- **THEN** none of them is an argument the catalog marks as UZE's, and the agent resumes or starts its task's conversation as it would with none
