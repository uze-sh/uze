## MODIFIED Requirements

### Requirement: The plugin root placeholder means one thing everywhere

The system SHALL replace `${PLUGIN_ROOT}` with the delivered package root in
every text file it delivers that the author may use it in — `mcp.json`,
`hooks.json`, `SKILL.md` and agent definitions — and in no other file. The
delivered package root SHALL be the package's build in that harness's vendor
store, SHALL contain the whole package, and SHALL be the same on every
route of that harness. A path that harness would load as a harness-only
component from a plugin root SHALL be held under the build's `.uze-carried/`
at its own relative path, and a placeholder naming that path SHALL resolve
there.

#### Scenario: A skill names a file by the placeholder

- **WHEN** a skill body contains `${PLUGIN_ROOT}/phases/plan.md`
- **THEN** the body a harness loads names a path that exists
- **AND** the canonical `SKILL.md` in the package sources still contains the placeholder

#### Scenario: A server in a harness-only path

- **WHEN** a package's MCP server command is `${PLUGIN_ROOT}/bin/server` and it is delivered to Claude Code
- **THEN** the command Claude Code is handed names a file that exists in the package's build
- **AND** the build has no `bin/` at its plugin root

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

### Requirement: A harness-only component is not carried into a generated plugin

A path the harness would load as a component the canonical format does not
define SHALL NOT be placed where the harness loads it from the generated
plugin; the build holds it only under `.uze-carried/`. `uze inspect` SHALL
name each such path the package contains as not delivered to that harness.
For Claude Code these are `commands/`, `bin/`, `output-styles/`,
`workflows/`, `themes/`, `monitors/`, `settings.json`, `.lsp.json`, and its
own `hooks/hooks.json` and `.mcp.json`.

#### Scenario: A harness-only component is not delivered

- **WHEN** a package contains `bin/tool` and `commands/legacy.md`
- **THEN** neither is present at the generated Claude Code plugin's root
- **AND** both are held under the build's `.uze-carried/`
- **AND** both remain in the package sources
- **AND** `uze inspect` names `bin` and `commands` as not delivered to Claude
  Code

## ADDED Requirements

### Requirement: A linked source keeps the build's identity and unlinking restores it
While a marketplace is linked to a checkout, each of its packages' builds
SHALL keep its place, selector, enablement and receipts, SHALL record that
its source is linked, and SHALL be rendered again when the checkout changes.
After unlinking, the next resolution SHALL render each build again from the
commit pinned before the link.

#### Scenario: An edit while linked
- **WHEN** a file changes in the linked checkout
- **THEN** the package's build is rendered again in place, and the harness still lists the same plugin, enabled as before

#### Scenario: Unlinking offline
- **WHEN** the marketplace is unlinked with no network, and its cached clone holds the pinned commit's blobs
- **THEN** the next resolution renders the build again from that commit, and `agents.lock` is unchanged
