## ADDED Requirements

### Requirement: A harness reads only its own vendor store
No file UZE delivers to a harness, and no configuration entry UZE or a vendor
CLI on its behalf writes, SHALL name a path under `$UZE_HOME` outside that
harness's vendor store. The exceptions are a launch argument or environment
for one process, and an earlier artifact that drifted and is reported.

#### Scenario: Every harness file points only into its store
- **WHEN** plugins with skills, agents, MCP servers, hooks and rules are installed for every harness
- **THEN** every `$UZE_HOME` path in any file under the home directory outside `$UZE_HOME` lies inside the vendor store of the harness that file belongs to

#### Scenario: The disposable tiers really are
- **WHEN** `sources/`, `runtime/` and `cache/` are moved away after an install
- **THEN** each harness still lists and loads every delivered plugin, skill, agent and MCP server, and its hooks and rules still run

#### Scenario: A workspace launch is per process
- **WHEN** the workspace launches an agent with a context directory under `$UZE_HOME`
- **THEN** that path appears only in the launched process's arguments or environment, never in a harness configuration file

### Requirement: A marketplace UZE registers is its vendor store
When a harness installs plugins from a marketplace, the marketplace UZE
registers for it SHALL be that harness's vendor store. Publishing SHALL
replace only the builds whose package changed.

#### Scenario: Claude reads the store
- **WHEN** a plugin is installed for Claude Code
- **THEN** the marketplace Claude records is Claude Code's vendor store

#### Scenario: An unchanged package is not rebuilt
- **WHEN** one package is updated while another stays the same
- **THEN** only the updated package's build is replaced, and a file a hook wrote into the other build is still there

#### Scenario: An earlier install moves without losing its plugins
- **WHEN** a machine whose marketplaces point into `runtime/` is upgraded and installs again
- **THEN** every plugin is still installed and enabled, and its marketplace is the harness's vendor store
