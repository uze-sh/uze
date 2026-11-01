## MODIFIED Requirements

### Requirement: Delivered hooks run without the packager
The system SHALL deliver a package's portable hooks as a self-contained artifact in each harness's own hook form: the harness invokes a wrapper inside the harness's own territory, never the `uze` binary, and the artifact SHALL contain no reference to the packager or to `$UZE_HOME`. A delivered hook SHALL keep working after the `uze` binary and `$UZE_HOME` are removed from the machine, as long as the harness remains.

#### Scenario: Hook keeps working after UZE is uninstalled
- **WHEN** a package with a `deny` pre-tool hook was installed for Claude Code and the `uze` binary is subsequently removed from `PATH`
- **THEN** the harness still invokes the delivered wrapper for a matching tool call
- **AND** the handler's denial is relayed to the harness in its own contract and the tool does not run

#### Scenario: Hook keeps working after the UZE home is deleted
- **WHEN** a package with a `deny` pre-tool hook was installed for Claude Code, Codex or Antigravity and `$UZE_HOME` is then removed
- **THEN** a matching tool call is still denied by the author's handler, not by a missing wrapper

#### Scenario: Native entry names only the delivered artifact
- **WHEN** the native hook configuration for a package is inspected on Claude Code, Codex or Antigravity
- **THEN** each entry's command resolves to a wrapper inside the harness's UZE directory, with the group's effect and the handler paths as arguments
- **AND** no entry contains the path of the `uze` executable or of `$UZE_HOME`

#### Scenario: OpenCode receives the runtime as a plugin
- **WHEN** a package with hooks is installed for OpenCode
- **THEN** the delivered artifact is a document of the package's groups inside OpenCode's UZE directory, read by the one generated runtime plugin that carries the same contract
- **AND** the runtime runs the author's handlers with the same context and decision contract as the wrapper on the other harnesses
