# plugin Specification

## Purpose
Install, update, remove, and list plugins via `name@marketplace` through the existing acquisition → Store → native projection pipeline, keeping `uze add <path>` as a direct-source shortcut.
## Requirements
### Requirement: Plugin install resolves marketplace entry and converges into acquisition
The system SHALL resolve `uze <name>@<marketplace>` by looking up the marketplace registry, reading its `marketplace.json` to find the plugin entry's `source`, and then running the existing `PackageSource` acquisition (Git/Local) → `Store` → `native projection` pipeline. The Store SHALL remain unaware of marketplace.

#### Scenario: Install from local marketplace
- **WHEN** user runs `uze flow@ai` and `ai` was added via local path
- **THEN** system resolves `flow → ./plugins/flow`, materializes it, installs to Store, and attaches via native projection where applicable

#### Scenario: Install from Git marketplace
- **WHEN** user runs `uze flow@ai` and `ai` is a Git marketplace
- **THEN** system clones/marketplace-resolves the plugin subdirectory and installs it identically to a direct `uze add` of that subdirectory

### Requirement: Plugin install is idempotent and shows marketplace provenance
The system SHALL record installed plugins with their `{name, marketplace}` provenance and make installing idempotent.

A bare plugin name already active from a different marketplace SHALL be resolvable at the root install form: `--alias <name>` installs alongside it, `--replace` takes the other one's name once it is safe to (ADR-036).

#### Scenario: Re-install same plugin
- **WHEN** `flow@ai` is already installed and user runs `uze flow@ai` again
- **THEN** system reports already installed without re-cloning or duplicating

#### Scenario: A name already taken by another marketplace
- **WHEN** `uze flow@other` runs and `flow` is already active from `ai`
- **THEN** it is refused unless `--alias` or `--replace` says which resolution is wanted

### Requirement: Plugin list shows marketplace and update availability
The system SHALL list installed plugins with their marketplace and update availability, and `plugin list` SHALL show the marketplace column.

#### Scenario: List plugins
- **WHEN** user runs `uze plugin list`
- **THEN** output includes `name`, `marketplace` (e.g., `ai`, `uze-official`), `version`, and `update_available`

### Requirement: Plugin update and remove use marketplace-resolved source
The system SHALL update a marketplace-installed plugin by re-resolving its marketplace entry and running the existing update pipeline, and remove SHALL detach via native projection then delete Store bytes, respecting ADR-009.

#### Scenario: Update plugin
- **WHEN** user runs `uze plugin update flow@ai` and the marketplace's plugin has a newer commit
- **THEN** system re-acquires and replaces the Store package

#### Scenario: Remove plugin
- **WHEN** user runs `uze plugin remove flow`
- **THEN** system detaches via `Integration` (native or capability) per ADR-009 and removes Store bytes; `marketplace remove ai` remains blocked while plugins from `ai` are installed

### Requirement: Direct add remains shortcut
The system SHALL keep `uze add <path|git>` as a direct PackageSource install without requiring a marketplace entry, for single-plugin sources. It follows the same scope rule as every root verb: it declares in the project when there is one, and `-m` confines it to this machine.

#### Scenario: Direct add
- **WHEN** user runs `uze add ./plugins/flow --trust`
- **THEN** system installs `flow` directly, with `marketplace` shown as `-` or `direct` in the plugin listing

### Requirement: A name has one spelling, the one every harness accepts
The system SHALL hold every plugin name, marketplace name and install alias
to lowercase kebab-case: one or more runs of `a-z` and `0-9` joined by
single `-`, at most 64 characters. A name outside the rule SHALL be refused
before anything is written, and the refusal SHALL name the rule and, when
one can be derived, the corrected name.

#### Scenario: An uppercase plugin name is refused with the name it meant
- **WHEN** a marketplace's plugin carries `"name": "Flow"` in its
  `plugin.json` and the operator installs it
- **THEN** the install fails before any byte reaches the Store, and the
  error says `try \`flow\``

#### Scenario: A name the vendor CLIs would read as a flag is refused
- **WHEN** a plugin is named `-force`
- **THEN** it is refused by the same rule, never passed to a vendor CLI

#### Scenario: Two spellings never become two packages
- **WHEN** a package `flow@ai` is installed and the operator installs
  `Flow@AI`
- **THEN** it resolves to the same package `flow@ai`, and no second Store
  entry or registration is created

### Requirement: The case a person types is forgiven
The system SHALL lowercase a plugin, marketplace or alias name a person
types to refer to one that exists or to choose an alias (the
`<plugin>@<market>` install spec, the `update`, `remove` and `inspect`
plugin argument, the name argument of `uze market remove`, `link`, `unlink`
and `inspect`, `--alias`, the `--market` of `uze agent plugin create`, and a
name typed at the collision prompt) before resolving it. Resolution against
the record SHALL stay exact. A name that becomes an authored file's name,
the `<name>` of `uze agent market create` and `uze agent plugin create`,
SHALL NOT be lowercased but refused under the name rule, and neither SHALL
a name read from a file, because the harness reading the same file does
not forgive it.

#### Scenario: Install and remove in another case
- **WHEN** the operator runs `uze install -m Flow@AI` and then
  `uze remove -m FLOW`
- **THEN** `flow@ai` is installed and then removed

