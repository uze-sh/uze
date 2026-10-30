## Purpose

Defines what the trust question shows before a package from a source the
operator did not have in front of them is installed or updated: everything a
harness can execute from that package, and every change to it.

## ADDED Requirements

### Requirement: The trust request covers every execution the delivery hands a harness

When a package crosses the trust boundary, the trust request SHALL list
every execution a supported harness can start from what UZE delivers: the
canonical MCP servers, hooks and executing Skills and agents, and the hooks,
MCP or language servers, commands, executables and other components a
vendor envelope or vendor file of the package carries.

#### Scenario: A Claude envelope declares a SessionStart hook
- **WHEN** a remote package ships `.claude-plugin/plugin.json` with an inline `hooks.SessionStart` command and no canonical `hooks.json`
- **THEN** the trust request lists that command, and a non-interactive install fails with `TRUST_REQUIRED` naming it

#### Scenario: Auto-discovered vendor files
- **WHEN** a remote package with a vendor envelope ships `hooks/hooks.json` or `.mcp.json`
- **THEN** each command those files declare is listed in the trust request

#### Scenario: An Antigravity MCP config
- **WHEN** a remote package ships its own `mcp_config.json`
- **THEN** each server it declares with a command is listed in the trust request

### Requirement: What the trust request cannot read is asked about, never passed over

An envelope key outside the known declarative ones, a vendor file whose
executable content UZE cannot read, or a component UZE does not interpret
(commands, executables put on the shell path, monitors, settings) SHALL be
listed as one execution naming it, so the package asks for consent.

#### Scenario: An unknown envelope key
- **WHEN** a remote package's vendor envelope declares a key UZE does not know
- **THEN** the trust request lists an execution naming that key and its value

#### Scenario: A metadata-only envelope
- **WHEN** a remote package's envelope declares only metadata and canonical skill paths
- **THEN** the envelope adds nothing to the trust request

### Requirement: Envelope MCP documents carrying a terminal control are refused

An MCP document read from a vendor envelope SHALL be refused, like a
canonical `mcp.json`, when any key or string in it carries a terminal
control character.

#### Scenario: A control sequence in an envelope MCP command
- **WHEN** a remote package's `.mcp.json` declares a command holding an escape character
- **THEN** the install fails before anything is written to the Store

### Requirement: An approval covers the whole package tree

The code an approval to execute covers SHALL be the digest of the package's
entire tree, so an update that changes any byte of a package with an
execution asks again, including in a non-interactive run, which fails with
`TRUST_REQUIRED`. An update whose bytes are unchanged does not ask.

#### Scenario: A sourced helper changes
- **WHEN** an installed package's MCP command runs `run.sh` at the package root, and the next revision changes only `lib/payload.sh`, which `run.sh` sources
- **THEN** a non-interactive update refuses the revision for want of consent (`TRUST_REQUIRED`) and the installed revision stays

#### Scenario: Nothing changed
- **WHEN** an update resolves to the same bytes as the installed revision
- **THEN** no trust question is asked
