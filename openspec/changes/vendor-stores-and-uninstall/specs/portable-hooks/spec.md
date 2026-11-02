## MODIFIED Requirements

### Requirement: Delivered hooks run without the packager
The system SHALL deliver a package's portable hooks as a self-contained artifact in each harness's own hook form: the harness invokes a wrapper inside the package's own build in that harness's vendor store, never the `uze` binary, and the artifact SHALL name no path outside that store. A delivered hook SHALL keep working after `uze uninstall` without `--purge`, which removes the binary and keeps the vendor stores.

#### Scenario: Hook keeps working after UZE is uninstalled
- **WHEN** a package with a `deny` pre-tool hook was installed for Claude Code, Codex or Antigravity and `uze uninstall --yes` has run
- **THEN** the harness still invokes the delivered wrapper for a matching tool call
- **AND** the handler's denial is relayed to the harness in its own contract and the tool does not run

#### Scenario: Native entry names only the delivered artifact
- **WHEN** the native hook configuration for a package is inspected on Claude Code, Codex or Antigravity
- **THEN** each entry's command resolves to the wrapper inside the package's build in that harness's vendor store, with the group's effect and the handler paths inside the package's build as arguments
- **AND** no entry contains the path of the `uze` executable

#### Scenario: OpenCode receives the runtime as a plugin
- **WHEN** a package with hooks is installed for OpenCode
- **THEN** the delivered artifact is a document of the package's groups inside OpenCode's vendor store, read by the one generated runtime plugin that carries the same contract
- **AND** the runtime runs the author's handlers with the same context and decision contract as the wrapper on the other harnesses
