## ADDED Requirements

### Requirement: A surface belongs to the agent it was opened on
A surface standing on an agent's tab or a shell beside it SHALL be put away
for that agent when another agent comes to the front, and SHALL stand again,
as it was left, when any of that agent's tabs comes back. Choosing the tab
or space in front, or another tab of the agent in front, SHALL take the pane
back from it. The space overview opened over it SHALL put it away the same
way, and putting the overview away SHALL give it back. What each agent kept
SHALL last for the client's run only.

#### Scenario: Moving between two agents
- **WHEN** the architect surface is open on agent A and the operator selects agent B, then agent A
- **THEN** B's pane is shown, and on A the architect surface stands again

#### Scenario: Moving to the shell beside the agent
- **WHEN** the architect surface is open on agent A's shell and the operator selects agent A, then the shell
- **THEN** A's pane is shown, then the shell's, and the architect surface stands on neither

#### Scenario: The overview over a surface
- **WHEN** the operator opens the space overview while a surface stands on agent A, then puts the overview away
- **THEN** the surface stands again on A
