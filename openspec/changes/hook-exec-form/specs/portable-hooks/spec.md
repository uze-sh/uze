## ADDED Requirements

### Requirement: A handler may name a program and its words instead of a shell line
A command handler MAY give `args`, a list of words, beside `command`. With `args` present, `command` SHALL be the path of a script or executable inside the package, relative to its root, and each word of `args` SHALL reach the handler intact, whatever it contains. The system SHALL choose the program that runs the script for the platform it delivers to: on POSIX a file with an execute bit runs itself; otherwise the file's extension selects an interpreter from a fixed table, and an `interpreter` list given by the author replaces the table. A handler the system cannot place on a platform SHALL be treated there like a handler with no spelling for it.

#### Scenario: A Python guard written once runs on both platforms
- **WHEN** a deny group's handler is `{"type": "command", "command": "hooks/guard.py", "args": ["--strict"]}`
- **THEN** on Linux the wrapper runs `python3` with the script's absolute path and `--strict`
- **AND** on Windows it runs the Python the machine answers with (`py -3`, `python` or `python3`) with the same words

#### Scenario: Words with spaces and quotes arrive intact
- **WHEN** the package root contains a space and an argument is `it's "$HOME"`
- **THEN** the handler receives the script path and that argument as single, literal words

#### Scenario: A shebang decides on POSIX
- **WHEN** `hooks/guard` is executable and starts with `#!/usr/bin/env -S uv run --script`
- **THEN** the wrapper runs the file itself and no interpreter is added to the package's requirements

#### Scenario: A script nothing can run is not delivered
- **WHEN** a deny group's exec-form handler is `hooks/guard.sh` and the platform is Windows
- **THEN** the package is refused there exactly as for a deny group with no `windows` spelling

### Requirement: A Windows Python alias that only opens the Store is not a Python
When choosing Python on Windows, the system SHALL run each candidate and SHALL treat a candidate that does not answer with a Python 3 version as absent.

#### Scenario: Only the Store alias is present
- **WHEN** `python3` on `PATH` is the App Execution Alias and no Python is installed
- **THEN** the launcher reports Python missing as a requirement of the hook, with the command that installs it
- **AND** the delivered line names `python`, so it runs once Python is installed

### Requirement: The harness's own runtime runs JavaScript where it carries one
On a harness that runs plugins in an embedded JavaScript runtime, the system SHALL run a `.js`, `.mjs`, `.cjs` or `.ts` exec-form handler in that runtime and SHALL NOT require `node` for it.

#### Scenario: A JavaScript guard on OpenCode
- **WHEN** a handler is `{"command": "hooks/guard.js", "args": []}` and the harness is OpenCode
- **THEN** the handler runs in OpenCode's own runtime and the package's requirements gain no `node` for it

### Requirement: An exec-form handler's interpreter is a requirement of the package
The executable an exec-form handler's launcher needs SHALL join the package's effective requirements, attributed to the hook. An interpreter every supported machine of that platform carries SHALL add nothing. When the hook fails closed, the report SHALL state that the calls it matches are denied until the executable is present.

#### Scenario: Missing Python under a deny group
- **WHEN** a deny group matching `shell` uses `hooks/guard.py` and `python3` is absent
- **THEN** the package installs, `python3` is reported unmet and attributed to that hook
- **AND** the report states every matching call is denied until `python3` is installed
