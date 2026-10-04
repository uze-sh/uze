## ADDED Requirements

### Requirement: An MCP server command starts on Windows

When projecting an MCP server on Windows, UZE SHALL write its command in the
form the harness is measured to start on Windows. A command that resolves to a
`.cmd` or `.bat` launcher (such as `npx`) SHALL be written so that the
harness starts it through the launcher, never as a bare name the harness
cannot execute.

#### Scenario: An `npx` server on Windows

- **WHEN** a plugin declaring an MCP server with command `npx` is installed on
  Windows with Claude Code detected
- **THEN** the projected server starts, and Claude Code lists its tools
