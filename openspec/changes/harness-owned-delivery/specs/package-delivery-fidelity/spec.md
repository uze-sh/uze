## MODIFIED Requirements

### Requirement: The plugin root placeholder means one thing everywhere

The system SHALL replace `${PLUGIN_ROOT}` with the delivered package root in
every text file it delivers that the author may use it in — `mcp.json`,
`hooks.json`, `SKILL.md` and agent definitions — and in no other file. The
delivered package root SHALL contain the whole package and SHALL be in the
harness's own territory. Where the harness expands its own root placeholder,
the system SHALL use that placeholder; otherwise it SHALL use an absolute
path in the harness's UZE directory.

#### Scenario: A skill names a file by the placeholder

- **WHEN** a skill body contains `${PLUGIN_ROOT}/phases/plan.md`
- **THEN** the body a harness loads names a path that exists
- **AND** the canonical `SKILL.md` in the Store still contains the placeholder

#### Scenario: The root survives the UZE home

- **WHEN** an MCP server whose command is `${PLUGIN_ROOT}/bin/server` is delivered and `$UZE_HOME` is then removed
- **THEN** the harness still starts the server from the delivered package root

#### Scenario: A harness that expands its own placeholder

- **WHEN** a skill using `${PLUGIN_ROOT}` is delivered inside a Claude Code plugin
- **THEN** the delivered text names `${CLAUDE_PLUGIN_ROOT}`

#### Scenario: A write into the root never reaches the Store

- **WHEN** a hook writes a file into its package root
- **THEN** the Store package is unchanged
- **AND** the next delivery of that package restores the root to the Store's content
