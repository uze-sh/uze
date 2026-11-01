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

### Requirement: A vendor store is the one marketplace UZE registers
Each harness that installs plugins from a marketplace SHALL have exactly one
UZE marketplace, and it SHALL be that harness's vendor store. It SHALL hold
the builds of author-enveloped and generated packages alike, each build
recording its origin. Publishing SHALL replace only builds whose key changed.

#### Scenario: Both origins in one marketplace
- **WHEN** a package with its own Claude Code envelope and a package without one are installed for Claude Code
- **THEN** both are plugins of the one UZE marketplace Claude records, and that marketplace is Claude Code's vendor store

#### Scenario: An unchanged package is not rebuilt
- **WHEN** one package is updated while another stays the same
- **THEN** only the updated package's build is replaced, and a file a hook added to the other build is still there

#### Scenario: An upgrade of UZE re-renders
- **WHEN** UZE is upgraded to a build whose rendering changed and no package changed
- **THEN** every build is rendered again

#### Scenario: An earlier install moves without losing its plugins
- **WHEN** a machine with plugins under `uze-local` and `uze-store`, pointing into `runtime/`, is upgraded and installs again
- **THEN** every plugin is installed from the vendor store's one marketplace, and every enabled or disabled state on Claude Code is kept

#### Scenario: A disabled Codex plugin during the merge
- **WHEN** the operator had disabled a Codex plugin installed under `uze-local`
- **THEN** it is not installed again under the merged marketplace, and the report names it with the command that installs it

#### Scenario: A repoint that fails on Codex
- **WHEN** re-adding Codex's marketplace from the vendor store fails
- **THEN** the previous marketplace root is registered again and the failure is reported
