## ADDED Requirements

### Requirement: Coverage is computed from the manifest the harness reads

When a package delivered to a harness carries more than one manifest the harness could read, the system SHALL determine which one the harness reads by that harness's precedence. It SHALL compute the capabilities that the envelope covers from that manifest alone, and SHALL deliver every other capability through exactly one other route. When the manifest the harness reads would expose the package in a way that UZE does not intend, the system SHALL deliver an envelope without it, and SHALL say so in the route's reason.

#### Scenario: Codex reads the root manifest first
- **WHEN** a package ships `.codex-plugin/plugin.json` and a root `plugin.json` declaring the Agent Plugins schema, and is delivered to Codex
- **THEN** the skills and MCP servers counted as covered are the ones Codex loads from the manifest it reads
- **AND** no MCP server is registered both by the plugin and by the vendor CLI

### Requirement: Delivered files speak the harness's current dialect

Every file that UZE writes for a harness SHALL use only fields that the harness's current release defines for that file. A field that the harness has removed or renamed SHALL NOT be written. A control that UZE cannot express in the current dialect (for example, hiding a skill from the user) SHALL be reported as a capability delivered short of native, with a reason that is true of the current release.

#### Scenario: OpenCode V2 skill without a user-invocation control
- **WHEN** a skill declares `invoke.user: false` and is delivered to OpenCode V2
- **THEN** its delivered file carries no field that V2 does not define
- **AND** the report says the skill stays invocable by the user on OpenCode, with the reason

#### Scenario: OpenCode V2 agent fields
- **WHEN** an agent's `opencode` block carries a field that V2 marks as legacy
- **THEN** the field is not written, and the route evidence names it as not carried
