## ADDED Requirements

### Requirement: The built-in marketplace's plugins are declared without a source
A project SHALL be able to declare plugins of the marketplace built into
uze, listed under its name with no source: the binary is the source. A
`git:` or `path:` on that name SHALL be refused, naming it as built in.

#### Scenario: Installing a built-in plugin in a project declares it
- **WHEN** `uze install workspace@uze` runs in a project
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
- **WHEN** `uze install uze@uze` runs in a project
- **THEN** nothing is written to `agents.yaml`, and a project that had no
  manifest still has none

## REMOVED Requirements

### Requirement: The built-in marketplace is never declared
**Reason**: Written when the built-in marketplace held only the bundled
plugin. It now carries plugins bootstrap does not install, which a
project could not otherwise declare.
**Migration**: A project lists the built-in marketplace's plugins under
`uze` with no source; the bundled plugin stays undeclared.
