## Purpose

Lets a person take UZE off a machine in one command, so that nothing UZE
delivered or registered is left behind except what the person changed or
chose to keep.

## ADDED Requirements

### Requirement: Uninstall shows its plan before acting
`uze uninstall` SHALL act on the machine and SHALL first report what it will
remove per harness, what it will leave and why, and the fate of each
top-level entry of `$UZE_HOME`. It SHALL remove nothing until the person
confirms or `--yes` is given.

#### Scenario: The plan is declined
- **WHEN** a person runs `uze uninstall` and answers no
- **THEN** nothing on the machine has changed

#### Scenario: Non-interactive use
- **WHEN** `uze uninstall` runs without a terminal and without `--yes`
- **THEN** it prints the plan, removes nothing and exits non-zero naming `--yes`

### Requirement: Uninstall delivers nothing
Uninstall SHALL NOT run the bootstrap that other commands run first: no
default plugin is seeded, no harness is prepared and no catalogue is
published before or during it.

#### Scenario: A second run after a partial one
- **WHEN** uninstall is run again after a run that detached the bundled plugin
- **THEN** the bundled plugin is not delivered again

### Requirement: Uninstall detaches by receipt
Uninstall SHALL inspect every receipt UZE holds, including those of `setup`,
and SHALL detach exactly those whose artifact still matches. An artifact
that drifted, conflicts or cannot be inspected SHALL be left and listed. So
SHALL a vendor registration whose vendor CLI is missing.

#### Scenario: Everything matches
- **WHEN** uninstall runs on a machine where nothing UZE wrote was edited
- **THEN** no file under the home directory names `$UZE_HOME`, and no UZE directory remains under any harness root
- **AND** the vendor registries no longer list a UZE marketplace, plugin or MCP entry

#### Scenario: An edited skill
- **WHEN** a delivered skill directory was edited by hand
- **THEN** uninstall leaves it, lists its path, and still removes everything else

#### Scenario: A vendor CLI is gone
- **WHEN** the Codex binary was removed before uninstall
- **THEN** Codex's marketplace and plugin registrations are listed as left, and its files are not edited by hand

### Requirement: Readers are detached before what they read
Within each harness, uninstall SHALL remove the entries and plugins that read
UZE material before it removes that material. An interruption SHALL never
leave a harness reading a missing file it fails closed on.

#### Scenario: Interrupted on OpenCode
- **WHEN** uninstall is interrupted while detaching OpenCode
- **THEN** OpenCode either still runs `uze.ts` with all its documents, or no longer loads `uze.ts`

### Requirement: Uninstall can run again until it is done
Uninstall SHALL be safe to repeat. A second run after a partial one SHALL
continue from what remains, including removing the binary, and SHALL report
nothing for what is already gone.

#### Scenario: Interrupted with purge
- **WHEN** an uninstall with `--purge` is interrupted after removing deliveries
- **THEN** running it again removes the binary and the rest of `$UZE_HOME`

### Requirement: Uninstall refuses while UZE runs
Uninstall SHALL refuse, naming what is running, when it is run inside a
workspace pane or while agents, workspace clients, the terminal runtime with
panes, or a background upgrade are alive.

#### Scenario: A live agent
- **WHEN** an agent launched by the workspace is still running
- **THEN** uninstall changes nothing and names the agent

#### Scenario: Inside a pane
- **WHEN** uninstall is run from a shell in a workspace pane
- **THEN** it changes nothing and says to run it outside the workspace

### Requirement: Records are kept unless purged
Uninstall SHALL keep the operator's records (tasks, memory, approvals,
reviews) and say where they are, unless `--purge` is given. Without
`--purge` it SHALL still clear the package registry and receipt ledger.

#### Scenario: Default
- **WHEN** uninstall completes without `--purge`
- **THEN** the records remain, the report names their directory, and the next `uze` run lists no installed package

#### Scenario: Purge
- **WHEN** uninstall completes with `--purge`
- **THEN** no part of `$UZE_HOME` remains

### Requirement: Uninstall never touches a project's work
Uninstall SHALL NOT change a project's files (`agents.yaml`, `agents.lock`,
`.agents/`, `AGENTS.md`), checkouts, branches or refs. It SHALL report
checkouts and shelves holding work not yet delivered. It SHALL remove
workspace links that point into `$UZE_HOME`.

#### Scenario: A checkout with unlanded commits
- **WHEN** a recorded checkout holds commits its target lacks
- **THEN** uninstall leaves it and names its path and commit count

#### Scenario: A shelf
- **WHEN** a project holds a UZE shelf ref
- **THEN** uninstall leaves the ref and names it

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
- **THEN** the user `Path` entry it added is removed and the running binary is gone once the process exits
