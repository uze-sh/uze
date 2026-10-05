# module-boundary Specification

## Purpose
Keeps uze's two modules, the package manager and the workspace, usable on
their own: the workspace may build on the package manager, and nothing the
package manager does depends on, turns on, or is failed by the workspace.
## Requirements
### Requirement: A project declares a workspace policy only by choosing one
The `agents.yaml` the system creates on a project's behalf SHALL declare
nothing about the workspace and SHALL carry no workspace text at all, not
even commented: the package manager's keys sit at the root and the
workspace's live under one `workspace:` section, which appears only after
somebody chooses a policy, through the workspace or by editing the file.

#### Scenario: The first install declares no policy
- **WHEN** a plugin is installed into a project that has no `agents.yaml`
- **THEN** the `agents.yaml` written declares the plugin and its marketplace
- **AND** it carries no `workspace:` section and no commented workspace key

#### Scenario: A package-only project carries no workspace region
- **WHEN** a project that never chose a workspace policy installs plugins
  and reconciles its context
- **THEN** its `AGENTS.md` carries no region about isolated checkouts, work
  naming or delivery of finished work

#### Scenario: Choosing a policy declares it
- **WHEN** the workspace sets a delivery behavior for a project whose
  `agents.yaml` has no `workspace:` section
- **THEN** `agents.yaml` gains a `workspace:` section holding that choice,
  and its `marketplaces:` are left byte for byte as they were

### Requirement: A workspace setting never fails a package-manager command
The system SHALL read `agents.yaml` for `install`, `update`, `remove`,
`status` and `inspect` without validating the `workspace:` section against
the repository. A workspace section that is invalid for the workspace SHALL
be reported by the workspace, and SHALL NOT stop a package-manager command.
A key under `workspace:` SHALL NOT be rejected by the package manager for
being unknown to it.

#### Scenario: A linked path Git does not ignore
- **WHEN** `agents.yaml` declares a `workspace.link` path that the
  repository does not ignore, and a person runs `uze install`
- **THEN** the install completes
- **AND** the workspace reports the path when it next reads its policy

#### Scenario: A project that is not a Git repository
- **WHEN** a directory that is not a Git repository declares `workspace:`
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
The build SHALL fail when code that belongs to the package manager names
the workspace's domain (tasks, checkouts, work naming, landing finished
work, recorded conversations, or the workspace's policy), which lives in
the `uze-workspace` crate. In `uze-core` the crate graph holds the rule,
since `uze-core` does not depend on `uze-workspace`. In `uze-application`,
which orchestrates both, an architecture test holds it over a listed set of
package-manager and shared files, and the workspace's half is everything
the list leaves out.

#### Scenario: A package-manager path reaches for a task
- **WHEN** a change makes a listed package-manager file of
  `uze-application` name the workspace crate
- **THEN** the architecture suite fails and names the file

#### Scenario: The package manager's domain reaches for the workspace
- **WHEN** a change makes `uze-core` refer to a workspace task or checkout
- **THEN** the build fails, because `uze-core` does not depend on
  `uze-workspace`

