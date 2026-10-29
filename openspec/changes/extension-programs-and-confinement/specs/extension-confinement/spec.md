## Purpose

What a program an extension runs can reach on a machine that confines it,
how the operator learns whether this machine confines, and the consent a
program needs before it runs, confined or not.

## ADDED Requirements

### Requirement: A confined program reaches only what it was granted
Where the platform confines, a program SHALL read only the checkout, its Git
common directory, its extension's directory, system prefixes, the directories
of pinned programs and the paths its `exec:` grant declares. It SHALL write
only a per-run scratch directory the host removes, SHALL reach no network,
and SHALL signal only its own process group.

#### Scenario: A write into the checkout is denied
- **WHEN** on a confining platform a program writes `.git/hooks/pre-commit` or `README.md`
- **THEN** the write fails and the run is reported as it exited

#### Scenario: A write into the home directory or UZE's state is denied
- **WHEN** a program writes `~/.bashrc` or a file under `$UZE_HOME`
- **THEN** the write fails

#### Scenario: A read outside the grant is denied
- **WHEN** a program reads `~/.ssh/id_ed25519` or another process's `/proc/<pid>/environ`
- **THEN** the read fails

#### Scenario: A local socket is out of reach
- **WHEN** a program connects to `/var/run/docker.sock`, the user bus or a TCP address
- **THEN** creating the socket fails

#### Scenario: A fork bomb is contained
- **WHEN** a program forks without end within its time limit
- **THEN** every process of the run is gone once the limit passes, and none could leave its process group

### Requirement: Confinement is checked on every run
Every run SHALL verify that the confinement it was promised applied in full.
A run whose confinement failed to apply SHALL be refused when the record
says confinement was enforced at acceptance, and the extension SHALL show as
needing consent again.

#### Scenario: Confinement that failed is not papered over
- **WHEN** the helper cannot apply the ruleset on a run of an extension accepted as confined
- **THEN** the run is refused, the extension needs consent again, and nothing ran unconfined

### Requirement: A program's path is pinned when the extension is enabled
A bare program name SHALL run only from the absolute path the host resolved
on its own search path and recorded at enabling, never by searching `PATH`
at run time. A resolution under the project, `$UZE_HOME`, a temporary
directory or a WSL `/mnt/*` path SHALL be refused, and the host SHALL resolve
again only when the operator asks.

#### Scenario: A changed PATH changes nothing
- **WHEN** `PATH` gains `./node_modules/.bin` after `gh` was pinned to `/usr/bin/gh`
- **THEN** a run still executes `/usr/bin/gh`

#### Scenario: A missing program does not block enabling
- **WHEN** `exec:gh` resolves to nothing on this machine
- **THEN** the extension can be enabled, its source says "`gh` not found", and the Extensions screen offers to resolve it again

### Requirement: Programs are consented to on a step of their own
The `exec:` grants SHALL be asked after the other grants, listing each pinned
path, each device it reads, and each general-purpose runner as able to run
anything. Where this machine does not confine, the step SHALL say the program
runs as the operator, and the extension SHALL be asked again on every update.

#### Scenario: Missing confinement is disclosed
- **WHEN** the platform offers no confinement and an extension wants `exec:nvidia-smi`
- **THEN** the step reads "This machine cannot confine programs. `nvidia-smi` runs as you, with everything you can reach: your files, your keys, the network.", and the record says confinement was not enforced

#### Scenario: A runner is named for what it is
- **WHEN** `wants` has `exec:python3`
- **THEN** the consent step says `python3` can run anything, the same as running any code as the operator

### Requirement: Whether this machine confines is visible
`uze doctor`, `uze status` and the Extensions screen SHALL say whether this
machine confines programs, and which rights it cannot enforce. Each extension
that runs a program SHALL show "confined" or "runs as you".

#### Scenario: The operator sees the machine's confinement
- **WHEN** the operator runs `uze doctor` on Linux with Landlock ABI 7
- **THEN** the report says programs run confined, with no right unenforced
