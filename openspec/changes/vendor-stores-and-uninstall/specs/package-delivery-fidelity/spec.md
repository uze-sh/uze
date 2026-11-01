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

- **WHEN** a package whose build holds a hook-added file is updated to new content
- **THEN** its build is replaced whole by the new content

#### Scenario: An edited build is kept

- **WHEN** a file UZE wrote into a build was edited and the package is then updated
- **THEN** the build is kept as it is and reported as drifted, naming the file

## ADDED Requirements

### Requirement: A linked source keeps the build's identity and unlinking restores it
While a marketplace is linked to a checkout, each of its packages' builds
SHALL keep its place, selector, enablement and receipts, SHALL record that
its source is linked, and SHALL be rendered again when the checkout changes.
Unlinking SHALL render each build again from the pinned commit.

#### Scenario: An edit while linked
- **WHEN** a file changes in the linked checkout
- **THEN** the package's build is rendered again in place, and the harness still lists the same plugin, enabled as before

#### Scenario: Unlinking offline
- **WHEN** the marketplace is unlinked with no network and its cached clone holds the pinned commit
- **THEN** the build is rendered again from the pinned commit, and `agents.lock` is unchanged
