## MODIFIED Requirements

### Requirement: Overview reports semantic project state
The system SHALL report the workspace project half as semantic states, not file facts: `Environment` ∈ {NotConfigured, Invalid, InstallRequired, Ready}, `Context` ∈ {None, Ready, Issue}, and a `Plugins` quantity. The state SHALL be computed by the Application layer; the TUI SHALL render it verbatim and never re-derive it from `agents.lock` bytes.

#### Scenario: Plain directory
- **WHEN** the cwd contains neither `agents.lock` nor `marketplace.json`
- **THEN** Environment is `NotConfigured`, Context is `None`, Plugins shows none, and no MARKETPLACE column is rendered

#### Scenario: AGENTS.md only
- **WHEN** the cwd contains `AGENTS.md` and no anchors
- **THEN** Environment is `NotConfigured` and Context is `Ready`

#### Scenario: Valid lock, everything installed
- **WHEN** `agents.lock` parses and every declared plugin is installed in the Store
- **THEN** Environment is `Ready` and Plugins shows the installed quantity with no warning indicator

#### Scenario: Valid lock, nothing installed
- **WHEN** `agents.lock` parses and no declared plugin is installed
- **THEN** Environment is `InstallRequired` and Plugins shows a `! 0/N installed` divergence

#### Scenario: Valid lock, partially installed
- **WHEN** `agents.lock` parses and some declared plugins are missing
- **THEN** Environment is `InstallRequired` and only the missing plugins are reported as missing

#### Scenario: Malformed lock
- **WHEN** `agents.lock` exists but cannot be parsed (malformed or unsupported version)
- **THEN** Environment is `Invalid`, Plugins is unknown, and `Ready` is never emitted

#### Scenario: Ready is never emitted with a known unsatisfied requirement
- **WHEN** any declared plugin is missing from the Store
- **THEN** Environment SHALL NOT be `Ready`
