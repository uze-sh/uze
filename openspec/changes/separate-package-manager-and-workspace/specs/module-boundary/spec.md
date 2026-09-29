## Purpose

Keeps uze's two modules, the package manager and the workspace, usable on
their own: the workspace may build on the package manager, and nothing the
package manager does depends on, turns on, or is failed by the workspace.

## ADDED Requirements

### Requirement: A project declares a workspace policy only by choosing one
The `agents.yaml` the system creates on a project's behalf SHALL declare
nothing about the workspace. Every workspace key SHALL be present only as a
comment showing its default, so a project carries an isolation policy only
after somebody chooses one, through the workspace or by editing the file.

#### Scenario: The first install declares no policy
- **WHEN** a plugin is installed into a project that has no `agents.yaml`
- **THEN** the `agents.yaml` written declares the plugin and its marketplace
- **AND** it declares no `worktrees:` value, only commented keys

#### Scenario: A package-only project carries no workspace region
- **WHEN** a project that never chose a workspace policy installs plugins
  and reconciles its context
- **THEN** its `AGENTS.md` carries no region about isolated checkouts, work
  naming or delivery of finished work

#### Scenario: Choosing a policy declares it
- **WHEN** a completion behavior is chosen in the workspace for a project
  whose `agents.yaml` only carries the commented keys
- **THEN** `agents.yaml` records that choice as a live `worktrees:` value

### Requirement: A workspace setting never fails a package-manager command
The system SHALL read `agents.yaml` for `install`, `update`, `remove`,
`status` and `inspect` without validating the workspace's sections against
the repository. A workspace section that is invalid for the workspace SHALL
be reported by the workspace, and SHALL NOT stop a package-manager command.
A section the package manager does not own SHALL NOT be rejected by it for
being unknown to it.

#### Scenario: A linked path Git does not ignore
- **WHEN** `agents.yaml` declares a `worktrees.link` path that the
  repository does not ignore, and a person runs `uze install`
- **THEN** the install completes
- **AND** the workspace reports the path when it next reads its policy

#### Scenario: A project that is not a Git repository
- **WHEN** a directory that is not a Git repository declares `worktrees:`
  and a person runs `uze status`
- **THEN** the status of its plugins and context is reported

### Requirement: The package manager reports only what it owns
The package manager's report of what a project still owes (`uze status`,
and whether `uze install` has anything to reconcile) SHALL count only
package-manager state: plugins, the lock, and the `AGENTS.md` regions the
package manager writes. The workspace's region SHALL be reported by the
workspace.

#### Scenario: A stale workspace region
- **WHEN** a project's workspace policy changed and its `AGENTS.md` still
  carries the previous workspace region
- **THEN** `uze status` does not report the package environment as having
  pending changes because of it
- **AND** the workspace brings the region in step the next time it runs on the project

### Requirement: Package-manager code never names a workspace concept
The build SHALL fail when code that belongs to the package manager names a
workspace concept: tasks, checkouts, work naming, landing finished work,
recorded conversations, or the workspace's policy. Code shared by both
modules SHALL be named as shared, and the exceptions SHALL be listed with
the reason for each.

#### Scenario: A package-manager path reaches for a task
- **WHEN** a change makes package-manager code refer to a workspace task or
  checkout
- **THEN** the architecture suite fails and names the file and the concept
