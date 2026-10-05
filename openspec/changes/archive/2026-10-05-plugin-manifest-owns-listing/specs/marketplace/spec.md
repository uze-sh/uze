## MODIFIED Requirements

### Requirement: Marketplace add validates marketplace manifest
The system SHALL validate that the marketplace source contains a readable `marketplace.json` with `plugins[]` entries (`name`, `source`, and an optional free-text `category`), and that the manifest's own `name` is lowercase kebab-case of at most 64 characters (see the `plugin` capability). The name SHALL be checked before anything is recorded or mirrored. An entry SHALL NOT describe its plugin: any other field on it, `description` and `keywords` included, SHALL be ignored rather than refused, so a manifest written for another reader still registers.

#### Scenario: Valid marketplace
- **WHEN** `marketplace.json` exists and is well-formed
- **THEN** `marketplace add` succeeds

#### Scenario: Invalid marketplace
- **WHEN** `marketplace.json` is missing or malformed
- **THEN** `marketplace add` fails with a clear error and records nothing

#### Scenario: A marketplace named outside the rule
- **WHEN** `marketplace.json` carries `"name": "My_Market"`
- **THEN** `marketplace add` fails, names `my-market` as the name it meant,
  and records nothing

#### Scenario: An entry that still describes its plugin
- **WHEN** a `plugins[]` entry carries `description` or `keywords`
- **THEN** `marketplace add` succeeds and neither value is read

## ADDED Requirements

### Requirement: A plugin's listing is read from its own manifest
What the system shows about an offered plugin — its `description` and
`keywords`, in the plugin listing, `market inspect`, the plugins screen and
every JSON report of them — SHALL be read from that plugin's `plugin.json`
at the revision the catalogue was read at: in place for a local
marketplace, from the cached catalogue for a Git one, from the embedded
snapshot for the official one. Reading it SHALL NOT clone or fetch beyond
what reading the catalogue already did. A plugin whose `plugin.json` is
missing, unreadable or carries no such field SHALL be listed without it,
never dropped from the listing and never failing it.

#### Scenario: Editing the plugin's manifest changes the listing
- **WHEN** a linked marketplace's plugin changes the `description` in its
  `plugin.json`, and nothing else
- **THEN** the next plugin listing shows the new description

#### Scenario: A Git marketplace lists from its cached catalogue
- **WHEN** a marketplace registered from a Git URL is listed while its
  repository is unreachable and its cached catalogue stands
- **THEN** every plugin is listed with the `description` and `keywords`
  its `plugin.json` had at the catalogue's revision

#### Scenario: A plugin with a broken manifest is still listed
- **WHEN** one plugin's `plugin.json` is missing or is not JSON
- **THEN** the listing shows that plugin without a description, and every
  other plugin of the marketplace as usual
