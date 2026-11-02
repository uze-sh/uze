## MODIFIED Requirements

### Requirement: The root verbs act on the machine, and on the project when there is one

`uze add`, `uze remove`, `uze update` and `uze status` SHALL act on this
machine. When they run inside a project they SHALL also maintain that
project's `agents.yaml` and `agents.lock`; when they do not, they SHALL do
the machine half alone. `uze <plugin>@<marketplace>` SHALL be `uze add`
without its verb.

Every one of them SHALL report which scopes it changed. A command SHALL NOT
change a scope without naming it.

#### Scenario: Using a plugin inside a project declares it
- **WHEN** `uze add <plugin>@<marketplace>` runs inside a project
- **THEN** the package is installed and delivered, the project declares it,
  and the report names both

#### Scenario: Using a plugin outside a project declares nothing
- **WHEN** `uze add <plugin>@<marketplace>` runs where there is no project
- **THEN** the package is installed and delivered, no project file is
  written anywhere, and the report says nothing was declared

#### Scenario: The shorthand is the same command
- **WHEN** `uze <plugin>@<marketplace>` runs with any of `add`'s flags
- **THEN** it does exactly what `uze add` does with them

#### Scenario: A plugin installed outside a project still works everywhere
- **WHEN** a package was installed where there was no project
- **THEN** every harness resolves it in every project, because delivery is
  user-scope and belongs to no project

### Requirement: `-m` states machine scope where a command has two

The verbs that act on both scopes SHALL accept `-m` / `--machine`, which
confines them to this machine and leaves every project file untouched. It
SHALL mean the same thing on each verb it appears on, and SHALL be accepted
whether or not a project is present.

It exists for the caller that cannot choose a working directory to express
intent — a script, a container, a test harness — and for the operator who
wants the machine half while standing in a project.

#### Scenario: A scripted install states its scope
- **WHEN** `uze add <plugin>@<marketplace> -m` runs inside a project
- **THEN** the package is installed and delivered and no project file is
  written

#### Scenario: The flag is not required outside a project
- **WHEN** a root verb runs where there is no project, with and without `-m`
- **THEN** both behave identically

### Requirement: One spelling per operation

No operation SHALL have two command spellings, save `uze
<plugin>@<marketplace>`, which is `uze add` without its verb. The `plugin`
namespace SHALL NOT exist, and no alias for its removed verbs SHALL be kept.

#### Scenario: The removed namespace is gone
- **WHEN** a command under the removed namespace is invoked
- **THEN** it is not recognized, and the message names the verb that
  replaced it

## ADDED Requirements

### Requirement: Install reproduces the project and adds nothing

`uze install [path]` SHALL install what the project declares — the project
here, or the one whose root `path` is — and SHALL NOT take a plugin. A
plugin spec, or a path that is not a project's root, SHALL be refused with
the `uze add` it meant, and nothing SHALL be written.

#### Scenario: Install converges the project
- **WHEN** `uze install` runs in a project that declares plugins
- **THEN** `agents.lock` records them and each is installed and delivered

#### Scenario: A plugin handed to install is refused
- **WHEN** `uze install <plugin>@<marketplace>` runs
- **THEN** it exits with a usage error naming `uze add
  <plugin>@<marketplace>`, and no project file is written

#### Scenario: A flag of add handed to install is refused
- **WHEN** `uze install -m` or `uze install --alias <name>` runs
- **THEN** it exits with a usage error and no project file is written
