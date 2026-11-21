## MODIFIED Requirements

### Requirement: Marketplace add validates marketplace manifest
The system SHALL validate that the marketplace source contains a readable `marketplace.json`, whose `plugins[]` and `extensions[]` entries, either list optional, hold `name`, `source` and an optional free-text `category`, and that its own `name` is lowercase kebab-case of at most 64 characters (the `plugin` capability's rule), checked before anything is recorded or mirrored. An entry SHALL NOT describe what it offers: any other field on it, `description` and `keywords` included, SHALL be ignored rather than refused.

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
- **WHEN** a `plugins[]` entry carries `description` or `keywords`, as a
  manifest written for another reader may
- **THEN** `marketplace add` succeeds and neither value is read

#### Scenario: A marketplace with no plugins list
- **WHEN** `marketplace.json` carries `extensions[]` and no `plugins`
- **THEN** `marketplace add` succeeds
