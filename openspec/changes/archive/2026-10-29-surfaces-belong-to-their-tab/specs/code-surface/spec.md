## ADDED Requirements

### Requirement: Returning to an agent shows the code surface as it was left
When an agent comes back to the front with the code surface kept for it, the
surface SHALL show the half it was left on, the selection, the opened
directories and the scroll. A door pressed afterwards SHALL still decide the
half, and a fresh open through a door SHALL NOT be given the half last used.

#### Scenario: Files left open on an agent
- **WHEN** the code surface shows a file in Files on agent A, and the operator selects agent B, then agent A
- **THEN** the code surface stands on A in Files, on that file
