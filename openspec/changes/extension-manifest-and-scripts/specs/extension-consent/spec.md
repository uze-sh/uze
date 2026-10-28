## ADDED Requirements

### Requirement: An extension declares every grant it needs
A manifest SHALL list its grants in `wants`. In this version the check
SHALL accept only `read:checkout`, `clipboard` and `open-path`, and SHALL
refuse every other grant, naming the change that brings it.
`write:checkout` SHALL be refused as a grant that does not exist.

#### Scenario: A later grant is named, not accepted
- **WHEN** a manifest wants `net`
- **THEN** the check fails saying `net` arrives in a later change

#### Scenario: Writing the checkout does not exist
- **WHEN** a manifest wants `write:checkout`
- **THEN** the check fails saying no extension writes the checkout

### Requirement: Consent comes before the first run, and is recorded by identity
An extension that declares a script SHALL run nothing until the operator
accepts its grants. The acceptance SHALL be recorded per identity
`plugin@marketplace/id` with the grants and the digest of the loaded
directory. An extension with fixtures only runs nothing and SHALL need no
consent.

#### Scenario: A same-named extension from another plugin is a stranger
- **WHEN** `acme@github` and `evil@github` both ship `extensions/gsd-band/` and the first is consented to
- **THEN** the second shows as needing consent and shares no record, cache or keys

### Requirement: Enabling is per project
A consented extension SHALL run only in the projects it is enabled in, or
in every project once the operator enables it there, which an extension
wanting an `agent:*` grant never is. Opening a project that declares a
plugin with an extension not enabled there SHALL offer enabling through a
toast and one dialog listing them all, as a project's commands are
offered; putting the dialog away SHALL record nothing.

#### Scenario: Enabling does not cross projects
- **WHEN** `gsd-band` is enabled in project A and the operator opens project B, which declares the same plugin
- **THEN** B shows the extension as needing enabling and nothing of it runs in B

### Requirement: An update that widens grants waits for consent
An update SHALL be compared by grant set: a widening SHALL hold the
extension until accepted, a narrowing or the same set SHALL apply, and
the Extensions screen SHALL say "updated <date>, same grants" with the
new digest. Code that is not the digested bytes SHALL never run.

#### Scenario: A widened update is held
- **WHEN** an update adds `clipboard` to an extension that had `read:checkout`
- **THEN** the extension runs nothing until the operator accepts the new grant, which is named

### Requirement: Consent is given only from UZE's own surfaces
Consent and enabling SHALL be given only on the Extensions screen or at
the `uze install` prompt on a terminal. Both SHALL be refused when the
request comes from inside an agent's pane, and no flag SHALL skip them.
A non-interactive install SHALL leave the extension disabled and say so.

#### Scenario: An agent cannot enable an extension
- **WHEN** `uze install` runs inside an agent's pane
- **THEN** the extension is installed disabled, and the report says it needs enabling from the workspace

### Requirement: The checkout is a worktree root, never the home directory
The checkout an extension reads SHALL be a Git worktree root the workspace
knows, canonicalised, and SHALL never be `/`, a volume root, `$HOME` or an
ancestor of it, `$UZE_HOME` or anything above or below it, a temporary
directory or a bare repository. The runner SHALL check it on every answer.

#### Scenario: A home directory is not a checkout
- **WHEN** the workspace is opened on a repository whose root is `$HOME`
- **THEN** no extension source runs there and the Extensions screen says why

### Requirement: The operator can see each extension's state
The Extensions screen, `uze status` and `uze inspect <plugin>` SHALL say
of each declared extension whether it is enabled and in which projects,
needs consent, needs enabling here, or is held by a widened update, with
its grants.

#### Scenario: A held extension says why
- **WHEN** an extension is held by an update that wants `clipboard`
- **THEN** `uze status` lists it as held and names `clipboard`
