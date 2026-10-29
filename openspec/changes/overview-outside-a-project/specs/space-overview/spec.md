## REMOVED Requirements

### Requirement: An agent outside a project says so
**Reason**: It hid Memory and Settings, which need no project.
**Migration**: Replaced by "Every section stands outside a project".

## ADDED Requirements

### Requirement: Every section stands outside a project
The overview SHALL draw every section whatever its read answered. A
directory outside every project SHALL be overviewed as itself: Memory
lists what reaches its agents, Settings shows the defaults and its first
change creates `agents.yaml` there, Plugins and Worktrees show their empty
states, and no section is marked. A directory that is gone, or a read that
failed, SHALL be said in each section that cannot be read, which its tab
marks, while the Agent section keeps listing the prompts.

#### Scenario: A directory that is not a project
- **WHEN** the agent runs in a plain directory with no project in or above it
- **THEN** every section is drawn unmarked, and changing a setting creates `agents.yaml` in that directory

#### Scenario: A directory that is gone
- **WHEN** the agent's directory was deleted
- **THEN** every section is offered, each but Agent saying the directory no longer exists
