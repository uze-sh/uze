## ADDED Requirements

### Requirement: A harness reads nothing under the UZE home
No file UZE delivers to a harness, and no entry UZE writes into a harness's
configuration, SHALL name a path under `$UZE_HOME`. What a harness reads at
session time lives in that harness's own root. A launch argument for one
process is the only exception.

#### Scenario: Every receipt is free of the UZE home
- **WHEN** plugins with skills, agents, MCP servers, hooks and rules are installed for every harness
- **THEN** no delivered file and no configuration entry any receipt names contains the `$UZE_HOME` path

#### Scenario: The harness keeps working without the UZE home
- **WHEN** `$UZE_HOME` is moved away after an install
- **THEN** each harness still lists and loads every delivered plugin, skill, agent and MCP server
- **AND** its hooks and rules still run

#### Scenario: A workspace launch is per process
- **WHEN** the workspace launches an agent with a context directory under `$UZE_HOME`
- **THEN** that path appears only in the launched process's arguments or environment, never in a harness configuration file

### Requirement: Each harness has one UZE-owned directory in its own root
UZE SHALL keep everything it writes into a harness's territory inside one
directory of its own under that harness's root. The only exceptions are the
entries a harness reads from fixed locations (skill, agent and plugin
discovery roots, its configuration files), and each of those is owned by a
receipt.

#### Scenario: Where the material goes
- **WHEN** the marketplace source, package roots, hook wrapper and rules engine are delivered for a harness
- **THEN** each is inside that harness's UZE directory

#### Scenario: The harness ignores the directory on its own
- **WHEN** a harness starts with its UZE directory present
- **THEN** it loads nothing from it except what an entry UZE registered names

### Requirement: A marketplace UZE registers has its source in the harness's territory
When a harness reads a plugin from its marketplace source, or requires that
source to exist, the source UZE registers SHALL be inside that harness's UZE
directory, and SHALL be replaced whole on every publication.

#### Scenario: Claude reads the source
- **WHEN** a generated plugin is installed for Claude Code
- **THEN** the marketplace Claude records points inside `~/.claude`'s UZE directory

#### Scenario: Codex needs the source to exist
- **WHEN** a generated plugin is installed for Codex and `$UZE_HOME` is then removed
- **THEN** `codex plugin list` still lists the plugin as installed and enabled
