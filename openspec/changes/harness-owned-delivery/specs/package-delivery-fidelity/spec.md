## MODIFIED Requirements

### Requirement: The plugin root placeholder means one thing everywhere

The system SHALL replace `${PLUGIN_ROOT}` with the delivered package root in
every text file it delivers that the author may use it in — `mcp.json`,
`hooks.json`, `SKILL.md` and agent definitions — and in no other file. The
delivered package root SHALL contain the whole package and SHALL be one
directory per package inside the harness's UZE directory, the same on every
route of that harness.

#### Scenario: A skill names a file by the placeholder

- **WHEN** a skill body contains `${PLUGIN_ROOT}/phases/plan.md`
- **THEN** the body a harness loads names a path that exists
- **AND** the canonical `SKILL.md` in the Store still contains the placeholder

#### Scenario: The root survives the UZE home

- **WHEN** an MCP server whose command is `${PLUGIN_ROOT}/bin/server` is delivered and `$UZE_HOME` is then removed
- **THEN** the harness still starts the server from the delivered package root

#### Scenario: One root on every route

- **WHEN** a package's skill is delivered inside a Claude Code plugin and its hook through `settings.json`
- **THEN** both name the same package root inside Claude Code's UZE directory

#### Scenario: A write into the root never reaches the Store

- **WHEN** a hook writes a file into its package root
- **THEN** the Store package is unchanged
- **AND** the file stays while the package is unchanged

#### Scenario: A changed package replaces its root

- **WHEN** a package whose root holds a hook-written file is updated to new content
- **THEN** its root is replaced whole by the new content
- **AND** the roots of every other package are untouched
