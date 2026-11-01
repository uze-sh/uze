## ADDED Requirements

### Requirement: A harness reads only its own vendor store
No configuration or discovery file of a harness that UZE, or a vendor CLI on
its behalf, writes SHALL name a path under `$UZE_HOME` outside that harness's
vendor store. The exceptions are a launch argument or environment for one
process, an earlier artifact reported as drifted, and the harness's own
transcripts.

#### Scenario: Every harness file points only into its store
- **WHEN** plugins with skills, agents, MCP servers, hooks and rules are installed for every harness
- **THEN** every `$UZE_HOME` path in any harness configuration or discovery file lies inside the vendor store of the harness that owns the file

#### Scenario: The disposable tiers really are
- **WHEN** `runtime/` and `cache/` are moved away after an install
- **THEN** each harness still lists and loads every delivered plugin, skill, agent and MCP server, and its hooks and rules still run

#### Scenario: A workspace launch is per process
- **WHEN** the workspace launches an agent with a context directory under `$UZE_HOME`
- **THEN** that path appears only in the launched process's arguments or environment, never in a harness configuration file

### Requirement: A delivery never depends on the package bytes being present
A package whose bytes are missing from the sources SHALL keep its builds, its
place in every catalogue and its receipts. It SHALL be reported as missing
its bytes, and SHALL NOT be pruned, detached or rebuilt until the bytes
return.

#### Scenario: Sources moved away
- **WHEN** `sources/` is moved away and `uze doctor` or the workspace starts
- **THEN** every harness still lists and loads every delivered plugin
- **AND** each package is reported as missing its bytes, and no receipt is detached

### Requirement: Each marketplace UZE registers lies in its vendor store
Every marketplace UZE registers with a harness SHALL be a directory inside
that harness's vendor store, one per marketplace UZE keeps for it.
Publishing SHALL replace only the builds whose build key changed.

#### Scenario: Claude reads the store
- **WHEN** a generated plugin and an author-enveloped plugin are installed for Claude Code
- **THEN** both marketplaces Claude records lie inside Claude Code's vendor store

#### Scenario: An unchanged package is not rebuilt
- **WHEN** one package is updated while another stays the same
- **THEN** only the updated package's build is replaced, and a file a hook added to the other build is still there

#### Scenario: An upgrade of UZE re-renders
- **WHEN** UZE is upgraded to a build whose rendering changed and no package changed
- **THEN** every build is rendered again

#### Scenario: An earlier install moves without losing its plugins
- **WHEN** a machine whose marketplaces point into `runtime/` is upgraded and installs again
- **THEN** every plugin is still installed and enabled, and each marketplace lies in the harness's vendor store

#### Scenario: A repoint that fails on Codex
- **WHEN** re-adding Codex's marketplace from the vendor store fails
- **THEN** the previous marketplace root is registered again and the failure is reported
