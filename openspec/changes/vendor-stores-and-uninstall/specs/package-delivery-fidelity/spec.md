## MODIFIED Requirements

### Requirement: The plugin root placeholder means one thing everywhere

The system SHALL replace `${PLUGIN_ROOT}` with the delivered package root in
every text file it delivers that the author may use it in — `mcp.json`,
`hooks.json`, `SKILL.md` and agent definitions — and in no other file. The
delivered package root SHALL be the package's build in that harness's vendor
store, SHALL contain the whole package, and SHALL be the same on every
route of that harness.

#### Scenario: A skill names a file by the placeholder

- **WHEN** a skill body contains `${PLUGIN_ROOT}/phases/plan.md`
- **THEN** the body a harness loads names a path that exists
- **AND** the canonical `SKILL.md` in the package sources still contains the placeholder

#### Scenario: One root on every route

- **WHEN** a package's skill is delivered inside a Claude Code plugin and its hook through `settings.json`
- **THEN** both name the package's build in Claude Code's vendor store

#### Scenario: A write into the root never reaches the sources

- **WHEN** a hook writes a file into its package root
- **THEN** the package sources are unchanged
- **AND** the file stays while the package is unchanged

#### Scenario: A changed package replaces its build

- **WHEN** a package whose build holds a hook-written file is updated to new content
- **THEN** its build is replaced whole by the new content
