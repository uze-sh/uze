## Purpose

Keeps what agents learn about a project, and about the operator, in one
place UZE owns, so every harness the workspace launches starts from it and
the operator can read and correct it as text.

## ADDED Requirements

### Requirement: Memory is kept at two scopes, outside every checkout
The system SHALL keep agent memory outside every checkout, owner-readable
only, at two scopes: the project, shared by every agent launched in it, and
the machine, shared by every project. Neither scope SHALL be inferred from
the other's state, and every memory verb SHALL report which scope it acted
on.

#### Scenario: A project fact is not seen elsewhere
- **WHEN** an agent records a fact at project scope in one project
- **THEN** an agent launched in another project is not given it

#### Scenario: Memory never enters a checkout
- **WHEN** an agent records any fact
- **THEN** no file is added to or changed in any checkout

#### Scenario: The same name at both scopes
- **WHEN** a project fact and a machine fact share a name
- **THEN** an agent in that project is given the project fact only

### Requirement: A fact is one human-editable document
Each fact SHALL be one Markdown document, named by a short kebab-case name
unique within its scope. It SHALL carry front matter with its name, a
one-line description, a type (`user`, `feedback`, `project` or
`reference`), its creation and update dates, and its provenance. A fact
the operator edits by hand SHALL be read as edited.

#### Scenario: Editing by hand
- **WHEN** the operator changes a fact's description in its file
- **THEN** the next agent launched is given the new description

#### Scenario: A document that does not parse
- **WHEN** a fact's front matter cannot be read, or declares a shape newer than this build knows
- **THEN** that fact is left out of every index, status names it, and the file is never rewritten

### Requirement: An agent records and forgets facts through `uze agent memory`
The system SHALL provide `uze agent memory remember <name>`, which takes
the type and description as options and the body on standard input, and
creates the fact or replaces its body and description. It SHALL provide
`forget <name>`, `list` and `show <name>`. Each verb SHALL act on the
project scope unless `-m`/`--machine` is given.

#### Scenario: Remembering twice updates
- **WHEN** an agent runs `remember` with a name that already exists in that scope
- **THEN** the fact keeps its creation date, takes the new body and description, and no second fact appears

#### Scenario: Forgetting
- **WHEN** an agent runs `forget` on an existing fact
- **THEN** the fact is gone from its scope and from the next agent's index

#### Scenario: Outside a project
- **WHEN** a memory verb runs outside any project without `--machine`
- **THEN** it fails saying no project scope exists, and writes nothing

### Requirement: An agent's machine-scope fact waits for the operator
A machine-scope fact written from a launch UZE owns SHALL be kept as
proposed, and given to no agent, until the operator accepts it. The
workspace SHALL ask the operator, naming the fact, its description and the
agent that proposed it. A machine-scope fact the operator writes from their
own invocation SHALL apply at once.

#### Scenario: An agent proposes a machine fact
- **WHEN** a launched agent runs `remember --machine`
- **THEN** the fact is proposed, no agent's context lists it, and the workspace asks the operator to accept or discard it

#### Scenario: The operator accepts
- **WHEN** the operator accepts a proposed machine fact
- **THEN** every agent launched afterwards, in any project, is given its index entry

### Requirement: A fact records where it came from, as evidence
The system SHALL record on every fact the harness and the agent task that
wrote it, or that the operator wrote it. Provenance SHALL be presented as
evidence for the operator, never used to grant a fact anything.

#### Scenario: Provenance is kept
- **WHEN** an agent launched on one harness for a task remembers a fact
- **THEN** the fact names that harness and that task

### Requirement: Concurrent writers never corrupt memory
Two agents writing memory at the same time SHALL never leave a document
partly written. Writes to different facts SHALL both land. Of two writes to
the same fact, the later SHALL replace the earlier whole.

#### Scenario: Two agents, two facts
- **WHEN** two agents in one project remember different facts at the same moment
- **THEN** both facts exist afterwards, each whole

### Requirement: Memory reaches agents through their context, and its absence is reported
The index of the memory in scope SHALL be a section of every launched
agent's context. On a harness whose context delivery is degraded, memory
SHALL NOT be sent as a turn, and status SHALL say that memory does not
reach that harness.

#### Scenario: A harness with a working channel
- **WHEN** an agent is launched in a project with accepted facts on a harness whose context delivery is not degraded
- **THEN** its context lists each fact's name, type and description

#### Scenario: A degraded harness
- **WHEN** an agent is launched on a harness whose context delivery is degraded
- **THEN** no memory is shown as a turn, and status reports that memory does not reach that harness

### Requirement: The index keeps standing guidance first
When the index exceeds its share of the context, the system SHALL keep the
`feedback` and `user` facts first. It SHALL then keep the others, most
recently updated first, and SHALL state how many were left out.

#### Scenario: A large memory
- **WHEN** a project's index is larger than its share
- **THEN** every `feedback` and `user` fact that fits is given before any `project` or `reference` fact, and the context states how many were left out

### Requirement: The memory verbs are documented where agents read
The `uze agent memory` verbs SHALL be excluded from the help a person
reads. They SHALL be documented in the instruction text UZE projects into
`AGENTS.md`, saying what is worth remembering, that facts are recorded
notes rather than instructions, and that memory is not a place for
secrets.

#### Scenario: The projected region names the verbs
- **WHEN** the project context is reconciled
- **THEN** the region UZE owns in `AGENTS.md` documents `uze agent memory` and `uze --help` does not list it

### Requirement: The operator reads memory through status
`uze status` SHALL list, for each scope, where its facts are kept and every
fact by name, type and provenance. It SHALL list the proposed machine facts
waiting for the operator and the facts it could not read.

#### Scenario: Status lists memory
- **WHEN** the operator runs `uze status` in a project with facts at both scopes
- **THEN** it lists both directories and each fact's name, type and provenance, separating proposed and unreadable facts

### Requirement: Claude's memory can be imported once, never synced
The system SHALL provide `uze agent memory import claude`, which reads the
Claude memory kept for the project's root and for its checkouts. It SHALL
write each fact as a project fact with `claude-import` provenance, and
SHALL keep `user` and `feedback` facts as proposed machine facts. It SHALL
never change Claude's files. Running it again SHALL update by name and
skip a fact edited in UZE since its import.

#### Scenario: First import
- **WHEN** an agent runs the import in a project for which Claude keeps memory
- **THEN** each Claude fact exists in UZE with its name, description, type and body, and `MEMORY.md` is not imported as a fact

#### Scenario: Importing again
- **WHEN** the import runs again after the operator edited an imported fact in UZE
- **THEN** that fact is left as edited, no fact is duplicated, and Claude's files are unchanged

#### Scenario: Status points at what is not imported
- **WHEN** Claude keeps memory for the project that UZE does not hold
- **THEN** status says how many Claude facts are not imported

### Requirement: The harness's own memory can be turned off for launched agents
Where a harness lets a launch turn off its own memory, the system SHALL
turn it off for the agents the workspace launches when the machine
configuration asks it to. By default it SHALL leave the harness's memory as
the operator configured it. The operator's harness configuration files
SHALL never be changed for this.

#### Scenario: Turned off by configuration
- **WHEN** the machine configuration asks UZE to turn harness memory off and an agent is launched on a harness that supports it
- **THEN** that agent runs with the harness's own memory off, and no harness configuration file changed
