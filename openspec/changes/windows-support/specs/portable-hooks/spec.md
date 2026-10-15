## ADDED Requirements

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

#### Scenario: The scaffold produces both spellings

- **WHEN** `uze agent plugin create <name> --market <market> --hook` runs
- **THEN** the plugin carries a POSIX guard and its PowerShell twin, and
  `hooks.json` declares both spellings
- **AND** `uze agent plugin check` passes on every platform

### Requirement: A handler with no spelling for its platform is settled by its group's effect

When a handler has no spelling for the platform it is installed on:

- a group whose effect is `deny` or `ask` SHALL fail the install, naming the
  group, and nothing of that package SHALL be attached;
- an observational group SHALL be reported Unsupported for that platform,
  with the reason, and the rest of the package SHALL be delivered.

#### Scenario: A guard with no Windows spelling

- **WHEN** a package whose `PreToolUse` `deny` group declares only a string
  command is installed on Windows
- **THEN** the install fails with a non-zero exit, naming the group and its
  missing `windows` spelling
- **AND** nothing of that package is attached

#### Scenario: An observer with no Windows spelling

- **WHEN** a package whose `PostToolUse` observational group declares only a
  string command is installed on Windows
- **THEN** the group is reported Unsupported on Windows with the reason, and
  the package's other capabilities are delivered

### Requirement: Trust covers every spelling of a handler

Trust SHALL show both spellings. A change to either spelling SHALL count as a
change to the hook.

#### Scenario: Only the Windows spelling changed

- **WHEN** an update changes only a handler's `windows` spelling
- **THEN** trust asks again before the hook is attached

### Requirement: Command hooks reach Windows through a PowerShell wrapper

On Windows, the system SHALL compile the portable command-hook contract into
a generated PowerShell wrapper. The wrapper SHALL preserve:

- the `HOOK_*` environment;
- exit `0` allows;
- the canonical deny exit denies, with a bounded reason from stderr;
- any other status is a failure, resolved by the group's effect;
- sequential manifest order;
- each handler's timeout;
- first-deny-wins;
- fail-open for observational groups, fail-closed for `deny` and `ask`.

#### Scenario: A deny hook on Windows

- **WHEN** a `PreToolUse` `deny` group spelled for Windows is installed with
  Claude Code detected, and the handler exits with the deny code
- **THEN** the tool call is denied with the handler's reason, as on Linux

#### Scenario: The OpenCode bridge on Windows

- **WHEN** a package with hooks spelled for Windows is delivered to OpenCode
  on Windows
- **THEN** the generated bridge runs handlers through Windows PowerShell,
  never `/bin/sh`

### Requirement: The wrapper needs only Windows PowerShell 5.1

The wrapper SHALL depend on nothing outside Windows PowerShell 5.1.

#### Scenario: A stock Windows machine

- **WHEN** hooks run on a Windows machine with neither PowerShell 7 nor any
  POSIX tool installed
- **THEN** the wrapper runs every handler as specified

### Requirement: The wrapper is reached through the entry form each harness executes

The wrapper SHALL be reached through the entry form each harness is measured
to execute on Windows, recorded per harness version.

#### Scenario: A harness version is measured on Windows

- **WHEN** hooks are delivered to a harness on Windows
- **THEN** the wrapper is attached through the entry form recorded for that
  harness's version

### Requirement: A Windows handler's status is its last command's

A handler's exit status SHALL be the status of its last command. A failing
command within a handler SHALL fail the handler.

#### Scenario: A failing command before the last

- **WHEN** a handler's Windows spelling runs two commands and the first fails
- **THEN** the handler fails, and the group's effect decides the outcome

#### Scenario: The deny code survives the shell

- **WHEN** a handler's Windows spelling runs a native program that exits with
  the deny code
- **THEN** the wrapper reads the deny code, not a generic failure

### Requirement: A timed-out handler on Windows is ended with what it started

When a timed-out handler is ended on Windows, the processes it started SHALL
be ended with it. A descendant whose parent has already exited MAY survive,
and this limitation SHALL be documented.

#### Scenario: A handler that outlives its timeout on Windows

- **WHEN** a handler runs past its declared timeout
- **THEN** it and the processes it started are ended, and the group's
  effect decides the outcome

### Requirement: Hooks are Unsupported where policy stops the wrapper

On a machine whose policy prevents the wrapper from running (an execution
policy enforced by Group Policy, or a constrained language mode), hooks SHALL
be reported Unsupported, with that reason, and SHALL NOT be attached.

#### Scenario: A machine where scripts are restricted by policy

- **WHEN** a package with hooks is installed on Windows where Group Policy
  enforces an execution policy
- **THEN** the hooks are reported Unsupported with that reason and are not
  attached
