## ADDED Requirements

### Requirement: A surface belongs to the tab it was opened on
A surface standing in an agent's pane SHALL be put away for that tab when
another tab comes to the front, by whatever path, and SHALL stand again, as
it was left, when that tab comes back. Choosing the tab or the space already
in front SHALL take the pane back from it. The space overview opened over it
SHALL put it away the same way, and putting the overview away SHALL give it
back. What each tab kept SHALL last for the client's run only.

#### Scenario: Moving between two agents
- **WHEN** the architect surface is open on agent A and the operator selects agent B, then agent A
- **THEN** B's pane is shown, and on A the architect surface stands again

#### Scenario: The overview over a surface
- **WHEN** the operator opens the space overview while a surface stands on agent A, then puts the overview away
- **THEN** the surface stands again on A
