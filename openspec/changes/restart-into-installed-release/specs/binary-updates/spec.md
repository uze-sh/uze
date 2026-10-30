## ADDED Requirements

### Requirement: The workspace restarts into an installed release

While the sidebar announces an installed release, the workspace SHALL offer
to restart into it. Restarting SHALL restore the terminal, run the
installed binary in the client's place on Linux, macOS and Windows, and
reattach to the running terminal runtime where the operator was. It SHALL
NOT stop the runtime or any pane.

#### Scenario: Restarting from the notice

- **WHEN** the operator activates the notice's "restart to use it" row
- **THEN** the client is replaced by the installed release, which attaches
  to the same runtime and shows the space and tab that were in front

#### Scenario: A pane during the restart

- **WHEN** an agent is running in a pane and the operator restarts
- **THEN** the agent keeps running, and the restarted client shows it

#### Scenario: From the keyboard

- **WHEN** a release is installed and the operator opens the action index
- **THEN** it offers the restart, and choosing it restarts as the row does
