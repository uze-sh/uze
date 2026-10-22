## MODIFIED Requirements

### Requirement: A handler's command is spelled per platform

A command handler SHALL declare its `command` as a string, or as a map
holding a `posix` spelling, a `windows` spelling, or both. A string SHALL
mean the `posix` spelling. The system SHALL run:

- the `posix` spelling through the POSIX shell on Linux and macOS;
- the `windows` spelling through Windows PowerShell 5.1 on Windows.

A spelling SHALL NEVER run in a shell it was not written for. A vendor's
Windows-specific command field met while importing SHALL be read as the
`windows` spelling.

#### Scenario: A handler spelled for both platforms

- **WHEN** a handler declares `posix: ./scripts/guard` and
  `windows: ./scripts/guard.ps1`
- **THEN** Linux and macOS run the first, and Windows runs the second

#### Scenario: The scaffold produces one exec-form entry

- **WHEN** `uze agent plugin create <name> --market <market> --hook` runs
- **THEN** the plugin carries a POSIX guard and its PowerShell twin under
  one extensionless name, and `hooks.json` declares one exec-form handler
  naming it
- **AND** `uze agent plugin check` passes on every platform

### Requirement: The system chooses the program that runs an exec-form script on each platform
The system SHALL choose the program that runs an exec-form script for the platform it delivers to: on POSIX a file with an execute bit runs itself; otherwise the file's extension selects an interpreter from a fixed table, and an `interpreter` list given by the author replaces the table. A script named without an extension SHALL resolve on Windows to its `.exe`, else its `.ps1`, sibling. A handler the system cannot place on a platform SHALL be treated there like a handler with no spelling for it.

#### Scenario: A Python guard written once runs on both platforms
- **WHEN** a deny group's handler is `{"type": "command", "command": "hooks/guard.py", "args": ["--strict"]}`
- **THEN** on Linux the wrapper runs `python3` with the script's absolute path and `--strict`
- **AND** on Windows it runs the Python the machine answers with (`py -3`, `python` or `python3`) with the same words

#### Scenario: A shebang decides on POSIX
- **WHEN** `hooks/guard` is executable and starts with `#!/usr/bin/env -S uv run --script`
- **THEN** the wrapper runs the file itself and no interpreter is added to the package's requirements

#### Scenario: One extensionless name, a native script per platform
- **WHEN** a handler is `{"command": "hooks/guard", "args": []}` and the package carries an executable `hooks/guard` and `hooks/guard.ps1`
- **THEN** Linux and macOS run `hooks/guard`, and Windows runs `hooks/guard.ps1` through Windows PowerShell

#### Scenario: A script nothing can run is not delivered
- **WHEN** a deny group's exec-form handler is `hooks/guard.sh` and the platform is Windows
- **THEN** the package is refused there exactly as for a deny group with no `windows` spelling
