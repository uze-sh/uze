## Purpose

How UZE declares, bounds and retires its support for the state and
behaviour an earlier release left on a machine, so that every piece of it
can be found, and a whole release's worth of it removed by changing one
value.

## ADDED Requirements

### Requirement: One floor names the oldest release UZE carries across

UZE SHALL name, in one place, the oldest release a machine can come from
and still be carried across. A machine last run on a release at or above
the floor SHALL keep its work; one below it SHALL meet the floor of each
record's class, as a shape this build cannot climb does.

#### Scenario: A machine at the floor

- **WHEN** the release named by the floor has left agents, prompts and
  approvals on a machine and this build opens it
- **THEN** the same agents, prompts and approvals are there
- **AND THEN** nothing is reported

#### Scenario: A machine below the floor

- **WHEN** a release older than the floor left a record whose shape no
  entry carries across
- **THEN** a rebuildable record is set aside and recorded again, and an
  irreplaceable one refuses with its remedy

### Requirement: Every piece of support for an earlier release is an entry naming its last release

Each thing UZE does only because an earlier release left something behind
— a record's shape, a path in its home, a file in a harness's
configuration or in a project, a wire protocol, a command spelling — SHALL
be an entry in the registry of its kind, and SHALL name the last release
that wrote what it handles.

#### Scenario: A new shape for a record

- **WHEN** a change gives a record a new shape
- **THEN** the step from the previous shape is an entry naming the last
  release that wrote it

#### Scenario: A path an earlier release wrote

- **WHEN** a change moves or renames a path UZE owns
- **THEN** the old path is a retired entry naming the last release that
  wrote it, with the step that carries it across when it holds a record

#### Scenario: A file an integration left in a harness's configuration

- **WHEN** an integration stops writing a file it used to deliver
- **THEN** removing what an earlier release wrote is an entry of that
  integration naming the last release that wrote it

### Requirement: Raising the floor names every entry to delete

When the floor is raised, the build SHALL fail on every entry whose last
release is below it, naming each entry and where it is, so that retiring a
release's support is the deletion of those entries and nothing else.

#### Scenario: The floor passes an entry

- **WHEN** the floor is raised above an entry's last release
- **THEN** the build fails, naming that entry and its file

#### Scenario: The entries are deleted

- **WHEN** every entry the build named is deleted
- **THEN** the build passes with no other change

### Requirement: Support for an earlier release lives only in its registry

Code that reads, moves or removes something only an earlier release wrote
SHALL live in the registry of its kind or in the `earlier` module of the
crate that owns it. The build SHALL fail on such code anywhere else,
naming the file and the module it belongs in.

#### Scenario: A fallback written inline

- **WHEN** a change adds a branch that reads an old path, an old field or
  an old name outside a registry or an `earlier` module
- **THEN** the build fails with the place it belongs in

#### Scenario: A list of retired names written twice

- **WHEN** a retired path or name is spelled in a second place instead of
  being read from its registry
- **THEN** the build fails, naming both places

#### Scenario: An alias for an old field

- **WHEN** a record gains a serde alias or default that exists to read a
  field an earlier shape wrote
- **THEN** the build fails, pointing at the record's ladder
