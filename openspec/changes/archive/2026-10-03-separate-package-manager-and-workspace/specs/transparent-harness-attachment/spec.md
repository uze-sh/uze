## MODIFIED Requirements

### Requirement: No launcher and no wrapper are required for normal invocation
The system SHALL NOT require `uze claude`, `uze codex`, or any UZE-specific
subcommand to start a harness with the attached capability available. The
system SHALL NOT install a process wrapper that replaces the harness's own
executable on the user's PATH, and SHALL NOT edit the user's shell startup
files. Outside the workspace, a harness command SHALL run the harness's own
executable. Where a capability reaches a harness only when the workspace
launches it, the system SHALL report that, never leave it silently missing.

#### Scenario: Plain harness invocation is sufficient
- **WHEN** `uze setup` and `uze add` have completed for a harness
- **THEN** starting that harness with its own normal command and no UZE
  arguments makes the attached skill available
- **AND THEN** no UZE-provided executable is required to be on PATH ahead of
  the harness's own executable

#### Scenario: Setup leaves the shell alone
- **WHEN** `uze setup` completes for any harness
- **THEN** no shell startup file under the user's home has changed

#### Scenario: A capability only the workspace can deliver
- **WHEN** a project authors skills under `.agents/skills` and a harness
  reads them only through the workspace's launch
- **THEN** `uze inspect` and `uze status` say they reach that harness inside
  the workspace
