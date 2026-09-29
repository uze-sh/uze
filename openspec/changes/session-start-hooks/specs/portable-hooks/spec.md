## ADDED Requirements

### Requirement: Session start is a portable observational event

The portable hook manifest SHALL accept a `SessionStart` event. Its groups
SHALL accept only the `observe` effect; a manifest declaring any other effect
for `SessionStart` SHALL be refused at check and install with the reason.
Handlers SHALL receive `HOOK_HARNESS`, `HOOK_EVENT=session_start`, `HOOK_CWD`,
`PLUGIN_ROOT` and, when the harness reports it, `HOOK_SOURCE` (one of
`startup`, `resume`, `clear`), and no tool fields. A handler failure SHALL be
reported and SHALL NOT prevent the session from starting. A group MAY declare
a matcher listing sources; without one it fires for every source.

#### Scenario: A session-start handler runs on a new session

- **WHEN** a package declares a `SessionStart` group running
  `${PLUGIN_ROOT}/hooks/ensure-ui.sh` and a new Claude Code session starts
- **THEN** the handler runs once with `HOOK_EVENT=session_start` and
  `HOOK_SOURCE=startup`

#### Scenario: A deny effect on session start is refused

- **WHEN** a manifest declares a `SessionStart` group with effect `deny`
- **THEN** `uze agent plugin check` reports the group and the allowed effect

### Requirement: An event a harness lacks does not block the package

When a harness offers no equivalent of a declared portable event, the system
SHALL deliver the package's other hooks and capabilities to that harness and
SHALL report the missing event as Unsupported for that harness with the
reason. The manifest SHALL NOT be refused for it.

#### Scenario: Antigravity and a session-start hook

- **WHEN** a package with `SessionStart` and `PreToolUse` groups is installed
  with Antigravity CLI detected
- **THEN** the `PreToolUse` group is delivered to Antigravity CLI
- **AND** the report lists `SessionStart` as Unsupported for Antigravity CLI
  with the reason
