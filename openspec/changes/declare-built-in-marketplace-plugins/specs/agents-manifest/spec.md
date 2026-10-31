## ADDED Requirements

### Requirement: The built-in marketplace's plugins are declared without a source
A project SHALL be able to declare plugins of the marketplace built into
uze, listed under its name with no source: the binary is the source. A
`git:` or `path:` on that name SHALL be refused, naming it as built in.

#### Scenario: Installing a built-in plugin in a project declares it
- **WHEN** `uze add workspace@uze` runs in a project
- **THEN** `agents.yaml` lists `workspace` under `uze`, with no `git:` or
  `path:`, and the workspace offers that project the plugin's extensions

#### Scenario: A source on the built-in name is refused
- **WHEN** a manifest declares `uze` with a `git:` or a `path:`
- **THEN** the manifest is reported as malformed, naming `uze` and saying
  it is built into uze

#### Scenario: A built-in declaration loads on any machine
- **WHEN** a manifest lists plugins under `uze` with no source
- **THEN** it loads, and its plugins resolve from the running binary's
  marketplace with nothing registered

### Requirement: A bundled plugin is never declared
A plugin the machine's bootstrap installs everywhere SHALL NOT be written
into a project manifest: every project has it already, and a line
declaring it says nothing.

#### Scenario: Installing the bundled plugin in a project writes nothing
- **WHEN** `uze add uze@uze` runs in a project
- **THEN** nothing is written to `agents.yaml`, and a project that had no
  manifest still has none

#### Scenario: A manifest declaring the bundled plugin is refused
- **WHEN** a manifest lists `uze` under the built-in marketplace
- **THEN** the manifest is reported as malformed, saying it is bundled
  with uze and every project has it

## MODIFIED Requirements

### Requirement: A marketplace declares its source and what the project takes from it
A marketplace entry SHALL carry the name the project refers to it by,
exactly one source — a Git remote with an optional reference and
subdirectory, or a local path — and the plugins the project installs from
it, as bare names; the built-in marketplace alone carries no source. A
plugin's identity is `plugin@marketplace` (ADR-036), and a plugin entry
SHALL carry no fields of its own. There SHALL be no top-level `plugins:`.

#### Scenario: Each source form parses to its own variant
- **WHEN** a manifest declares a marketplace as a Git remote or as a local
  path
- **THEN** it parses into the corresponding source variant carrying that
  variant's own fields, and the plugins under it are attributed to it

#### Scenario: A marketplace with no source, or with two, is rejected
- **WHEN** a marketplace entry other than the built-in one declares
  neither `git:` nor `path:`, or any entry declares both
- **THEN** the manifest is reported as malformed, naming the marketplace

#### Scenario: A source may be declared with nothing taken from it
- **WHEN** a marketplace is declared with no plugins under it
- **THEN** the manifest loads, declaring that source and no plugin

#### Scenario: The same plugin under two marketplaces is refused
- **WHEN** two marketplaces each list a plugin of the same name
- **THEN** the manifest is reported as malformed naming both marketplaces,
  because the file cannot say which of the two identities answers to that
  name locally

## REMOVED Requirements

### Requirement: The built-in marketplace is never declared
**Reason**: Written when the built-in marketplace held only the bundled
plugin. It now carries plugins bootstrap does not install, which a
project could not otherwise declare.
**Migration**: A project lists the built-in marketplace's plugins under
`uze` with no source; the bundled plugin stays undeclared.
