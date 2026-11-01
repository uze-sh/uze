## Purpose

Lets a person take UZE off a machine in one command, so that nothing UZE
delivered or registered is left behind except what the person changed or
chose to keep.

## ADDED Requirements

### Requirement: Uninstall shows its plan before acting
`uze uninstall` SHALL act on the machine and SHALL first report what it will
remove per harness, what it will leave, and why. It SHALL remove nothing
until the person confirms, or until `--yes` is given.

#### Scenario: The plan is declined
- **WHEN** a person runs `uze uninstall` and answers no
- **THEN** nothing on the machine has changed

#### Scenario: Non-interactive use
- **WHEN** `uze uninstall` runs without a terminal and without `--yes`
- **THEN** it prints the plan, removes nothing and exits non-zero naming `--yes`

### Requirement: Uninstall detaches by receipt
Uninstall SHALL inspect every receipt UZE holds, including those of machine
deliveries made by `setup`, and SHALL detach exactly those whose artifact
still matches. An artifact that drifted, conflicts or cannot be inspected
SHALL be left in place and listed with its path.

#### Scenario: Everything matches
- **WHEN** uninstall runs on a machine where nothing UZE wrote was edited
- **THEN** afterwards no file under any harness root names `$UZE_HOME`, and no UZE directory remains under any harness root
- **AND** the vendor registries no longer list a UZE marketplace, plugin or MCP entry

#### Scenario: An edited skill
- **WHEN** a delivered skill directory was edited by hand
- **THEN** uninstall leaves it, lists its path, and still removes everything else

### Requirement: Uninstall can run again until it is done
Uninstall SHALL be safe to repeat: a second run after a partial one SHALL
continue from what remains and SHALL report nothing for what is already gone.

#### Scenario: Interrupted
- **WHEN** an uninstall is interrupted after detaching some receipts
- **THEN** running it again detaches the rest

### Requirement: Uninstall refuses while agents run
Uninstall SHALL refuse, naming them, while agents UZE launched are alive. It
SHALL stop the terminal runtime before removing anything it uses.

#### Scenario: A live agent
- **WHEN** an agent launched by the workspace is still running
- **THEN** uninstall changes nothing and names the agent

### Requirement: Records are kept unless purged
Uninstall SHALL keep the operator's records (tasks, memory, approvals,
reviews) and say where they are, unless `--purge` is given.

#### Scenario: Default
- **WHEN** uninstall completes without `--purge`
- **THEN** the records remain and the report names their directory

#### Scenario: Purge
- **WHEN** uninstall completes with `--purge`
- **THEN** no part of `$UZE_HOME` remains

### Requirement: Uninstall never touches a project or unlanded work
Uninstall SHALL NOT change a project's files (`agents.yaml`, `agents.lock`,
`.agents/`, `AGENTS.md`), checkouts, branches or refs. It SHALL report
checkouts that hold work not yet delivered.

#### Scenario: A checkout with unlanded commits
- **WHEN** a recorded checkout holds commits its target lacks
- **THEN** uninstall leaves it and names its path and commit count

### Requirement: Harness binaries are the person's
Uninstall SHALL NOT remove a harness that `setup` installed through the
vendor's route, and SHALL say that it left it.

#### Scenario: A provisioned harness
- **WHEN** `setup` had installed OpenCode
- **THEN** OpenCode remains installed and the report says so

### Requirement: The binary is removed only where UZE placed it
Uninstall SHALL remove the `uze` binary when UZE's own installer placed it,
and otherwise SHALL print the command that removes it for the way it was
installed.

#### Scenario: Installed by cargo
- **WHEN** the binary was installed with `cargo install`
- **THEN** uninstall leaves it and prints `cargo uninstall uze`
