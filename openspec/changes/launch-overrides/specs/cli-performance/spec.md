## MODIFIED Requirements

### Requirement: The management screens' data is one read model within the budget
The data every management screen of the TUI shows SHALL be composed by
the application into one read model, and refreshing it SHALL be held to
the budget by a test that times that one call on a fresh application.

#### Scenario: A refresh is one call
- **WHEN** the TUI's management worker refreshes
- **THEN** it obtains plugins, health, marketplaces, marketplace plugins,
  workspace, context status and prompt history from a single application
  call, and composes nothing of its own beyond presentation ordering

#### Scenario: A warm refresh meets the budget without probing anything
- **WHEN** the read model is produced again with nothing changed
- **THEN** no harness is probed, no marketplace is cloned, and the call
  completes within the budget
