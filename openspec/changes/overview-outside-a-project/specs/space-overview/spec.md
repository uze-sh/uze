## REMOVED Requirements

### Requirement: An agent outside a project says so
**Reason**: It hid Memory, which needs no project.
**Migration**: Replaced by "The sections follow the directory".

## ADDED Requirements

### Requirement: The sections follow the directory
Inside a Git repository the overview SHALL show all five sections. Outside
every Git repository it SHALL show the Agent and Memory sections alone,
mark neither for what is missing, and write no settings: applying a
`workspace:` edit there SHALL be refused. A project that could not be read
SHALL be said in Settings, Plugins and Worktrees, which their tabs mark,
while Memory is still listed.

#### Scenario: A directory outside every repository
- **WHEN** the agent runs in a plain directory with no Git repository in or above it
- **THEN** the bar shows Agent and Memory, unmarked, and the machine's facts are listed

#### Scenario: A project that could not be read
- **WHEN** the agent's repository cannot be read as a project
- **THEN** all five sections are shown, and Settings, Plugins and Worktrees say why
