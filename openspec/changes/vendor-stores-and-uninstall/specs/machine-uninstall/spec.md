## Purpose

Lets a person take UZE off a machine in one command, either keeping what it
delivered to the harnesses working or removing every trace of it.

## ADDED Requirements

### Requirement: Uninstall shows its plan before acting
`uze uninstall` SHALL act on the machine and SHALL first report, per
harness and per top-level entry of `$UZE_HOME`, what it will remove, keep or
leave and why. It SHALL remove nothing until the person confirms or `--yes`
is given.

#### Scenario: The plan is declined
- **WHEN** a person runs `uze uninstall` and answers no
- **THEN** nothing on the machine has changed

#### Scenario: Non-interactive use
- **WHEN** `uze uninstall` runs without a terminal and without `--yes`
- **THEN** it prints the plan, removes nothing and exits non-zero naming `--yes`

### Requirement: Uninstall delivers nothing
Uninstall SHALL NOT run the bootstrap other commands run first: no default
plugin is seeded, no harness is prepared and no catalogue is published.

#### Scenario: A second run after a partial purge
- **WHEN** `uze uninstall --purge` is run again after a run that detached the bundled plugin
- **THEN** the bundled plugin is not delivered again

### Requirement: By default, what was delivered keeps working
Without `--purge`, uninstall SHALL remove the binary, the package sources and
the generated, remembered and shim directories. It SHALL keep the vendor
stores and the records, so every delivered plugin keeps working and a
reinstalled UZE adopts it by its receipts.

#### Scenario: Default uninstall
- **WHEN** `uze uninstall --yes` completes
- **THEN** every harness still loads every UZE-delivered plugin, and its hooks and rules still run
- **AND** only `store/` and `state/` remain under `$UZE_HOME`

#### Scenario: Reinstalled
- **WHEN** UZE is installed again after a default uninstall
- **THEN** `uze status -m` lists the delivered plugins as installed, and no harness is delivered to twice

### Requirement: Purge removes every trace by receipt
With `--purge`, uninstall SHALL detach every receipt whose artifact still
matches, removing each harness's readers before the material they read,
and SHALL then remove `$UZE_HOME`. An artifact that drifted, conflicts or
cannot be inspected, or a registration whose vendor CLI is missing, SHALL be
left and listed.

#### Scenario: Everything matches
- **WHEN** `uze uninstall --yes --purge` runs where nothing UZE wrote was edited
- **THEN** no file under the home directory names `$UZE_HOME`, and `$UZE_HOME` is gone
- **AND** the vendor registries list no UZE marketplace, plugin or MCP entry

#### Scenario: An edited skill
- **WHEN** a delivered skill directory was edited by hand
- **THEN** purge leaves it, lists its path, and removes everything else

#### Scenario: Interrupted on OpenCode
- **WHEN** a purge is interrupted while detaching OpenCode
- **THEN** OpenCode either still runs `uze.ts` with all its documents, or no longer loads `uze.ts`

#### Scenario: A vendor CLI is gone
- **WHEN** the Codex binary was removed before a purge
- **THEN** Codex's registrations are listed as left, and its files are not edited by hand

### Requirement: Uninstall can run again until it is done
Uninstall SHALL be safe to repeat. A later run SHALL continue from what
remains, including removing the binary, and SHALL report nothing for what is
already gone.

#### Scenario: Interrupted
- **WHEN** an uninstall is interrupted after removing the package sources
- **THEN** running it again removes the binary and the rest of what its mode removes

### Requirement: Uninstall refuses while UZE runs
Uninstall SHALL refuse, naming what is running, inside a workspace pane and
while agents, workspace clients, the terminal runtime with panes, or a
background upgrade are alive.

#### Scenario: A live agent
- **WHEN** an agent launched by the workspace is still running
- **THEN** uninstall changes nothing and names the agent

#### Scenario: Inside a pane
- **WHEN** uninstall is run from a shell in a workspace pane
- **THEN** it changes nothing and says to run it outside the workspace

### Requirement: Uninstall never touches a project's work
Uninstall SHALL NOT change a project's files (`agents.yaml`, `agents.lock`,
`.agents/`, `AGENTS.md`), checkouts, branches or refs. It SHALL report
checkouts and shelves holding unlanded work, and SHALL remove workspace
links that point into `$UZE_HOME`.

#### Scenario: A checkout with unlanded commits
- **WHEN** a recorded checkout holds commits its target lacks
- **THEN** uninstall leaves it and names its path and commit count

#### Scenario: A context link
- **WHEN** a checkout holds a link UZE created that points into `$UZE_HOME`
- **THEN** uninstall removes the link

### Requirement: Harness binaries are the person's
Uninstall SHALL NOT remove a harness that `setup` installed through the
vendor's route, and SHALL say that it left it.

#### Scenario: A provisioned harness
- **WHEN** `setup` had installed OpenCode
- **THEN** OpenCode remains installed and the report says so

### Requirement: The binary is removed only where UZE placed it
Uninstall SHALL remove the `uze` binary, and undo any `PATH` edit its
installer recorded, when UZE's own installer placed it. Otherwise it SHALL
print the command that removes the binary for the way it was installed.

#### Scenario: Installed by cargo
- **WHEN** the binary was installed with `cargo install`
- **THEN** uninstall leaves it and prints `cargo uninstall uze`

#### Scenario: Installed by the Windows installer
- **WHEN** the binary was placed by `install.ps1`
- **THEN** the user `Path` entry it added is removed and the binary is gone once the process exits
