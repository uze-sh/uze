## MODIFIED Requirements

### Requirement: Marketplace add validates marketplace manifest
The system SHALL validate that the marketplace source contains a readable `marketplace.json` with `plugins[]` entries (`name`, `source`), and that the manifest's own `name` is lowercase kebab-case of at most 64 characters (see the `plugin` capability). The name SHALL be checked before anything is recorded or mirrored.

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
