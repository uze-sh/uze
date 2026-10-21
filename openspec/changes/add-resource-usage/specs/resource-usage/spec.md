## Purpose

Tells the operator what UZE as a whole costs the machine it runs on: the
CPU, memory and processes of everything the workspace runs, the disk UZE
and its checkouts hold, and how long the workspace has been up.

## ADDED Requirements

### Requirement: One report for the whole of UZE on this machine
`uze usage` SHALL print one report for this machine, never narrowed to a project, space or agent: the machine's capacity, the workspace's uptime, a table of processes by component, and a table of disk by tier. It SHALL take no scope flag.

#### Scenario: Run anywhere
- **WHEN** `uze usage` runs inside a project, outside one, or in a workspace pane
- **THEN** the report is the same

### Requirement: The machine and the workspace's uptime head the report
The report SHALL open with the system it runs on (operating system, WSL when it is, architecture), its core count and its total memory, and, when a workspace is running, how long its server has been up and since when.

#### Scenario: A workspace up since the morning
- **WHEN** the server started at 09:14 and it is 12:26
- **THEN** the head reads the workspace as up 3h 12m since 09:14

### Requirement: Processes by component
The process table SHALL have one row each for agents, shells, the terminal server and clients, then a total, with CPU, memory and process count per row. Agents are every agent tab's processes and the shell tabs born of it; shells are the tabs no agent owns, including a finished agent's; clients are the attached `uze workspace` processes. Agents, shells and clients show how many there are.

#### Scenario: A shell opened from an agent
- **WHEN** a shell tab born of an agent runs a dev server
- **THEN** the dev server is counted in the agents row, not the shells row

#### Scenario: A finished agent
- **WHEN** an agent exited and its tab became a shell
- **THEN** that tab is counted in the shells row

### Requirement: A pane's processes are everything it started
A tab's processes SHALL be its pane program and every process still in its session, below it, or carrying its pane's mark; on Windows, its pane's job.

#### Scenario: A job of a shell
- **WHEN** a shell in a pane runs a command in a process group of its own
- **THEN** the command is counted with that pane

#### Scenario: A process that left the session
- **WHEN** a process started in a pane detached into a session of its own, keeping its environment
- **THEN** it is still counted with that pane

### Requirement: CPU is the machine's share over one second
CPU SHALL be the CPU time spent over a one-second window as a share of the whole machine, 0 to 100%, including processes that started and ended within it where the platform accounts them to a parent or job. A process missed by the first reading SHALL count only time spent after the window opened.

#### Scenario: A build of short compiler runs
- **WHEN** an agent's build runs compilers that each live under a second
- **THEN** their CPU is in the agents row on Linux, macOS and Windows

#### Scenario: Four busy cores out of eight
- **WHEN** agents keep four of eight cores busy for the window
- **THEN** the agents row reads about 50%

### Requirement: Memory is the process's own share
Memory SHALL count each process's own share, never twice the pages processes share: the proportional set size on Linux, the physical footprint on macOS, the private working set on Windows. Where only resident size can be read, the figure SHALL be marked as an estimate.

#### Scenario: Two agents run the same binary
- **WHEN** two agents each run the same harness binary
- **THEN** the agents row is not twice the binary's shared pages

### Requirement: Disk by tier
The disk table SHALL have a row for the checkouts UZE made for agents and subagents in every project it keeps records for, one per tier of UZE's home (packages, records, generated, cache, settings), then a total. A primary checkout, one a person made and a harness's own are not UZE's and SHALL NOT be counted; a checkout nested in one of UZE's SHALL NOT be counted with it; a project whose directory is gone SHALL be skipped.

#### Scenario: Someone else's checkout
- **WHEN** a person added a worktree by hand beside an agent's slot
- **THEN** only the slot is in the checkouts row

#### Scenario: A harness's checkout inside a slot
- **WHEN** a harness keeps a worktree of its own inside an agent's slot
- **THEN** its bytes are not in the checkouts row

#### Scenario: A broken declaration
- **WHEN** a project's `agents.yaml` does not parse
- **THEN** its agents' checkouts are still measured

### Requirement: No workspace running is not a failure
With no server running, the command SHALL exit successfully, say that no workspace is running in place of the process table, and still report the machine and the disk.

#### Scenario: After a reboot
- **WHEN** no workspace has been opened since the machine started
- **THEN** the report reads that no workspace is running, shows the disk table, and exits 0

### Requirement: A workspace from another build is named
When the running server cannot answer because it is another build, the command SHALL say so in place of the process table, name `uze workspace stop` as the way to replace it, report disk, and exit successfully.

#### Scenario: Upgraded under a running workspace
- **WHEN** the binary was replaced while an older server runs
- **THEN** the report names the mismatch and `uze workspace stop`

### Requirement: Machine-readable report
`--format json` SHALL print one JSON document with every figure the text shows, unrounded: the machine (system, cores, memory bytes), the workspace state (running, not running, another build) and its start time, each process row as CPU seconds per second of window and as a share, memory bytes with its estimate flag and process count, and each disk row in bytes.

#### Scenario: Scripted with no workspace
- **WHEN** `uze usage --format json` runs with no server running
- **THEN** the document's state is not running, it has no process rows, and it has the disk rows
