## ADDED Requirements

### Requirement: An open workspace keeps plugins current

While the workspace client is open, on the same tick as the release check,
UZE SHALL fetch each registered Git marketplace whose last answer is over
an hour old, one process at a time, and SHALL then apply the pending
updates that need no new trust. Each outcome SHALL be raised as a toast.

#### Scenario: A plugin released while the workspace is open

- **WHEN** a marketplace publishes a plugin's new revision that asks for no
  new execution, and the workspace has been open for over an hour
- **THEN** a later tick installs it and a toast names the plugin

#### Scenario: An update that needs consent

- **WHEN** the new revision asks to execute something new
- **THEN** it is not installed, and a toast that stays names the plugin and
  says it waits for confirmation

#### Scenario: Two clients

- **WHEN** two clients tick while a marketplace's answer is stale
- **THEN** one fetches it and the other fetches nothing

### Requirement: A marketplace mirror fetches branches and tags only

Fetching a marketplace's mirror SHALL bring its branches and tags and no
other refs.

#### Scenario: A GitHub marketplace with open pull requests

- **WHEN** the mirror of a GitHub marketplace is fetched
- **THEN** no `refs/pull/*` ref is fetched into it
