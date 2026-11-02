## MODIFIED Requirements

### Requirement: Plugin install resolves marketplace entry and converges into acquisition
The system SHALL resolve `uze add <name>@<marketplace>` (and its shorthand `uze <name>@<marketplace>`) by looking up the marketplace registry, reading its `marketplace.json` to find the plugin entry's `source`, and then running the existing `PackageSource` acquisition (Git/Local) → `Store` → `native projection` pipeline. The Store SHALL remain unaware of marketplace.

#### Scenario: Install from local marketplace
- **WHEN** user runs `uze add flow@ai` and `ai` was added via local path
- **THEN** system resolves `flow → ./plugins/flow`, materializes it, installs to Store, and attaches via native projection where applicable

#### Scenario: Install from Git marketplace
- **WHEN** user runs `uze add flow@ai` and `ai` is a Git marketplace
- **THEN** system clones/marketplace-resolves the plugin subdirectory and installs it identically to a local marketplace's

### Requirement: The case a person types is forgiven
The system SHALL lowercase a plugin, marketplace or alias name a person
types to refer to one that exists or to choose an alias before resolving
it. Resolution against the record SHALL stay exact.

#### Scenario: Install and remove in another case
- **WHEN** the operator runs `uze add -m Flow@AI` and then
  `uze remove -m FLOW`
- **THEN** `flow@ai` is installed and then removed

#### Scenario: Every typed reference is forgiven
- **WHEN** a person types a name in another case as the `<plugin>@<market>`
  install spec, the `update`, `remove` or `inspect` plugin argument, the
  name argument of `uze market remove`, `link`, `unlink` or `inspect`,
  `--alias`, the `--market` of `uze agent plugin create`, or at the
  collision prompt
- **THEN** the name is lowercased before it is resolved

## REMOVED Requirements

### Requirement: Direct add remains shortcut
**Reason**: Direct sources are refused (ADR-019: the marketplace is the provenance contract), and `uze add` now names the marketplace operation.
**Migration**: Publish the plugin in a marketplace (`uze market add <source>`) and run `uze add <name>@<market>`.
