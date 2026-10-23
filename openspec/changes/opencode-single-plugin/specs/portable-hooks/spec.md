## MODIFIED Requirements

### Requirement: Managed Hook delivery preserves user configuration
The system SHALL add only receipt-owned Hook artifacts and configuration
entries. Reconciliation SHALL inspect the exact managed content and removal
SHALL refuse drift/conflicts and preserve unrelated user hooks/plugins.

#### Scenario: Existing OpenCode plugins survive bridge delivery
- **WHEN** OpenCode's global plugin directory already contains a foreign plugin file
- **THEN** UZE delivers its hooks as a document of the package's groups, read by its one generated runtime plugin in that directory, without touching the foreign file or adding a `plugin` entry to `opencode.json`
- **AND** removal deletes only that package's document, and only when its content still matches what UZE generated

#### Scenario: A foreign file where the runtime plugin goes
- **WHEN** the file at the runtime plugin's path does not carry UZE's generated header
- **THEN** UZE does not write over it, attaches no OpenCode hook, and reports the hooks route blocked by that file

## ADDED Requirements

### Requirement: OpenCode runs every package's hooks in one pass
On OpenCode, the system SHALL run the groups of every installed package for
an event in one pass, in a deterministic order, so a denial by any package
stops the handlers after it and reaches the call exactly once.

#### Scenario: Two packages guard the same tool
- **WHEN** package `a` delivers a deny group and package `b` an observe group, both matching `shell`, and `a`'s handler denies a call
- **THEN** the call does not run and the denial reason reaches the model
- **AND** `b`'s observer sees the call under its own tool name or does not run, never under a substituted one

#### Scenario: A package installed while the service runs
- **WHEN** a package with hooks is installed while OpenCode's background service is running
- **THEN** the next matching tool call runs its groups without the service being restarted

### Requirement: An unreadable hook document fails closed
When a package's hook document cannot be read or parsed on OpenCode, the
system SHALL refuse every tool call, naming the document, until it is
generated again: which calls the package guarded is unknown without it.

#### Scenario: A corrupted document
- **WHEN** a package's hook document holds invalid JSON
- **THEN** a tool call is refused with a reason naming that document
- **AND** other capabilities the runtime carries keep working
