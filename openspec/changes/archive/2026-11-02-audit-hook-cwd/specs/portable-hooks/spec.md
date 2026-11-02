## MODIFIED Requirements

### Requirement: Handlers receive the hook context as environment and answer with exit codes
Command handlers SHALL receive the hook context as environment variables and SHALL NOT be required to parse any harness payload or emit any harness JSON. The wrapper SHALL set `HOOK_HARNESS`, `HOOK_EVENT`, `HOOK_TOOL` (the portable alias, or empty), `HOOK_TOOL_NATIVE`, `HOOK_CWD`, `HOOK_WORKDIR`, `HOOK_INPUT` (the input as JSON) and `PLUGIN_ROOT`, plus the matched alias's portable fields. A handler's exit code SHALL be the decision: `0` allows, `3` denies with the reason read from stderr.

#### Scenario: A shell handler reads the portable command
- **WHEN** a `shell`-matched pre-tool hook fires on any harness for a command line
- **THEN** the handler observes `HOOK_TOOL=shell`, `HOOK_COMMAND` equal to that command line, `HOOK_TOOL_NATIVE` equal to the harness's own tool name, `HOOK_CWD` equal to the workspace directory and `HOOK_WORKDIR` equal to the directory the command runs in
- **AND** the same handler, unchanged, observes the same variables on every other harness that delivers the hook

#### Scenario: Denial by exit code is relayed natively
- **WHEN** a handler of a `deny` group exits with code 3 and writes a reason to stderr
- **THEN** the harness receives its own blocking decision carrying that reason and the tool does not run
- **AND** the remaining handlers of the group are not started

## ADDED Requirements

### Requirement: The workspace is the session's, apart from where a command runs
`HOOK_CWD` SHALL be the session's workspace, read from a source the harness keeps for the session and the call cannot choose. `HOOK_WORKDIR` SHALL be set for the `shell` alias only: the absolute directory the command runs in, as the harness reports it for that event, resolved against the workspace when relative, and the workspace when the harness reports none. It SHALL be empty for every other tool and on `stop` and `session_start`.

#### Scenario: A call that names another directory keeps the workspace
- **WHEN** a shell call runs its command in a directory outside the workspace
- **THEN** a pre-tool handler observes `HOOK_CWD` equal to the workspace and `HOOK_WORKDIR` equal to that directory

#### Scenario: A call that names no directory runs in the workspace
- **WHEN** a shell call names no directory of its own
- **THEN** the handler observes `HOOK_WORKDIR` equal to `HOOK_CWD`

#### Scenario: A call that is not a shell command carries no directory
- **WHEN** a hook fires for a tool other than `shell`, or on `stop` or `session_start`
- **THEN** the handler observes `HOOK_CWD` equal to the workspace and an empty `HOOK_WORKDIR`
