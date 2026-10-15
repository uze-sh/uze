## ADDED Requirements

### Requirement: Windows provisioning takes the vendors' Windows routes

On Windows, `uze setup <harness>` SHALL take the harness vendor's documented
Windows installation or update route, without a POSIX shell. It SHALL verify
the executable under the name Windows resolves: `.exe`, or a `PATHEXT`
launcher such as `.cmd`. A harness whose vendor documents no automatic
Windows route SHALL be reported Blocked, naming the documented manual routes,
as on any other platform.

#### Scenario: Claude Code on a clean Windows machine

- **WHEN** `uze setup claude` runs on Windows and Claude Code is not
  installed
- **THEN** UZE runs Claude Code's official PowerShell installer
- **AND** verifies `claude --version` at the location that installer
  documents before marking setup ready

#### Scenario: OpenCode on a clean Windows machine

- **WHEN** `uze setup opencode` runs on Windows and OpenCode is not installed
- **THEN** setup is Blocked, naming the vendor's documented package-manager
  commands
- **AND** after the user installs OpenCode by one of them, `uze setup
  opencode` detects it and completes

#### Scenario: A harness installed through npm

- **WHEN** a harness was installed on Windows as an npm launcher
  (`codex.cmd`)
- **THEN** UZE detects it, verifies it and launches it through that launcher

### Requirement: An installed harness is updated through its own update command

A harness that is already installed by any route SHALL be updated through the
harness's own update command.

#### Scenario: A harness already installed on Windows

- **WHEN** `uze setup <harness>` runs on Windows and the harness is already
  installed, by whatever route
- **THEN** UZE updates it through the harness's own update command

### Requirement: Setup never updates an executable a running process holds

Setup SHALL NOT update a harness executable that a running process holds.

#### Scenario: A harness running while setup runs

- **WHEN** `uze setup <harness>` runs on Windows while a process runs that
  harness's executable
- **THEN** the executable is not updated

### Requirement: A runtime shim on Windows is a copy of the binary that setup keeps current

On Windows, a runtime shim SHALL be a copy of the installed `uze.exe` named
after the harness, placed without requiring Developer Mode or administrator
rights. Setup, `uze upgrade` and the workspace's own setup pass SHALL
replace a shim whose content differs from the installed `uze.exe`. A shim
that is running SHALL be set aside, never overwritten, and the image set
aside SHALL be removed once nothing runs it.

#### Scenario: A shim after an upgrade while a harness runs

- **WHEN** `uze upgrade` installs a new release on Windows while an agent
  runs through the `claude.exe` shim
- **THEN** the running session is untouched
- **AND THEN** the next launch of `claude` in a pane runs a copy of the new
  `uze.exe`

### Requirement: A shim on Windows stands in for the harness it launches

A shim SHALL:

- launch the real harness and forward its exit code;
- leave Ctrl+C to the harness;
- end the harness and everything it started when the shim is ended.

The workspace SHALL see the harness, not the shim, as the pane's foreground
program. On Windows, shims SHALL take effect inside the workspace's panes.

#### Scenario: A pane launched through a shim

- **WHEN** the workspace launches `claude` in a pane on Windows
- **THEN** the pane reports Claude Code as its foreground program, and
  closing the pane ends the shim, Claude Code and its descendants
