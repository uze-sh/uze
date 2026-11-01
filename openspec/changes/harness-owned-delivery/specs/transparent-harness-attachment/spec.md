## ADDED Requirements

### Requirement: A harness reads nothing under the UZE home
No file UZE delivers to a harness, and no entry UZE or a vendor CLI on its
behalf writes into a harness's configuration, SHALL name a path under
`$UZE_HOME`. The exceptions are a launch argument or environment for one
process, and an earlier artifact that drifted and is reported as left.

#### Scenario: Every harness file is free of the UZE home
- **WHEN** plugins with skills, agents, MCP servers, hooks and rules are installed for every harness
- **THEN** no file under the home directory outside `$UZE_HOME` contains the `$UZE_HOME` path

#### Scenario: The harness keeps working without the UZE home
- **WHEN** `$UZE_HOME` is moved away after an install
- **THEN** each harness still lists and loads every delivered plugin, skill, agent and MCP server
- **AND** its hooks and rules still run

#### Scenario: A workspace launch is per process
- **WHEN** the workspace launches an agent with a context directory under `$UZE_HOME`
- **THEN** that path appears only in the launched process's arguments or environment, never in a harness configuration file

### Requirement: Each harness has one UZE-owned directory in its own root
Each integration SHALL name one directory of its own under its harness's
root, and UZE SHALL keep everything it writes into that harness's territory
inside it. The only exceptions are the fixed locations the harness reads, and
each entry there SHALL be owned by a receipt.

#### Scenario: Where the material goes
- **WHEN** the marketplace source, package roots, hook wrapper and rules engine are delivered for a harness
- **THEN** each is inside that harness's UZE directory

#### Scenario: The harness ignores the directory on its own
- **WHEN** a skill, an agent and a hook configuration that no entry names are placed in a harness's UZE directory
- **THEN** the harness loads none of them

### Requirement: A marketplace UZE registers has its source in the harness's territory
When a harness reads a plugin from its marketplace source, or requires that
source to exist, the source UZE registers SHALL be inside that harness's UZE
directory. Publishing SHALL replace only the packages that changed.

#### Scenario: Claude reads the source
- **WHEN** a generated plugin is installed for Claude Code
- **THEN** the marketplace Claude records points inside Claude Code's UZE directory

#### Scenario: Codex needs the source to exist
- **WHEN** a generated plugin is installed for Codex and `$UZE_HOME` is then removed
- **THEN** `codex plugin list` still lists the plugin as installed and enabled

#### Scenario: An earlier install moves without losing its plugins
- **WHEN** a machine whose marketplaces point into `$UZE_HOME` is upgraded and installs again
- **THEN** every plugin is still installed and enabled, and its marketplace points inside the harness's UZE directory
