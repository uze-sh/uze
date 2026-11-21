## MODIFIED Requirements

### Requirement: A marketplace check covers its entries and their plugins
A marketplace check SHALL also report each `plugins[]` or `extensions[]` entry whose `name` is outside the name rule, whose `source` does not resolve inside the marketplace or whose plugin or extension fails its own check.

#### Scenario: A marketplace check covers its plugins
- **WHEN** the agent runs `uze agent market check` on a scaffolded marketplace whose one plugin has an invalid `hooks.json`
- **THEN** the command exits non-zero and the answer locates the finding in that plugin

#### Scenario: An entry whose source leaves the marketplace
- **WHEN** a `plugins[]` entry's `source` resolves outside the marketplace
- **THEN** `uze agent market check` exits non-zero naming the entry

#### Scenario: A marketplace check covers its extensions
- **WHEN** the agent runs `uze agent market check` on a marketplace whose one extension's manifest is invalid
- **THEN** the command exits non-zero and the answer locates the finding in that extension
