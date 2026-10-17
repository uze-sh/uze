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

### Requirement: Each launched agent is given exactly one way to write memory
The `uze agent memory` verbs SHALL be excluded from the help a person
reads, and SHALL be documented in the agent's context, never in
`AGENTS.md`. An agent on a harness whose own memory is how its agents
write facts SHALL be given no UZE write instruction. Every other agent
SHALL be given it, saying what is worth remembering, that facts are notes
rather than instructions, and that memory holds no secrets.

#### Scenario: A Codex agent
- **WHEN** an agent is launched on a harness whose agents do not write its own memory
- **THEN** its context documents `uze agent memory`, and `uze --help` does not list it

#### Scenario: A Claude agent
- **WHEN** an agent is launched on Claude, whose agents write Claude's own memory
- **THEN** its context carries the memory index and no instruction to write with `uze agent memory`, and Claude's own memory instruction is left as it is

#### Scenario: A Claude agent is not given its own facts twice
- **WHEN** an agent is launched on Claude
- **THEN** its index lists UZE's facts and other harnesses' entries, and none of the Claude facts Claude already loads itself

### Requirement: The operator reads memory through status
`uze status` SHALL summarize each scope in one line: how many facts it
holds and how many each provenance keeps. It SHALL list by name the
proposed machine facts waiting for the operator, with where each is kept,
and the files it could not read. The facts themselves SHALL be listed by
the workspace's space overview and by `uze status --json`, not by the
text report.

#### Scenario: Status summarizes memory
- **WHEN** the operator runs `uze status` in a project with facts at both scopes
- **THEN** it shows one line per scope with its count by provenance, and no fact's name or description

#### Scenario: What needs the operator is listed
- **WHEN** a machine fact is proposed or a fact file cannot be read
- **THEN** status names the proposal and where it is kept, and the unreadable file and why

### Requirement: Claude's own memory is read live as a source, never copied or redirected
At each launch, the system SHALL read the Claude memory kept for the
project's root and its checkouts, and add each fact to the project index
with `claude` provenance. It SHALL never copy, move, redirect or change
Claude's memory. On a name both hold, the UZE fact SHALL win.

#### Scenario: Claude's fact reaches a Codex agent
- **WHEN** Claude records a fact in its own memory for the project and a Codex agent is launched there afterwards
- **THEN** the Codex agent's index lists that fact, and no file was written under UZE's memory

#### Scenario: Claude outside the workspace keeps its memory
- **WHEN** the operator runs Claude outside the workspace after agents inside it learned facts on Claude
- **THEN** Claude reads those facts from its own memory as before

#### Scenario: Claude's index file is not a fact
- **WHEN** Claude's memory directory holds its `MEMORY.md` index
- **THEN** no fact named after it appears in UZE's index

### Requirement: Codex's consolidated memory is offered as one entry, never parsed
When Codex keeps a consolidated memory summary, the system SHALL add one
machine-scope index entry naming that file to the context of every agent
not launched on Codex. It SHALL never parse, copy or change the file.

#### Scenario: A Claude agent sees Codex's summary
- **WHEN** Codex has written its memory summary and a Claude agent is launched
- **THEN** the Claude agent's index has one entry naming that file, and the Codex file is unchanged

#### Scenario: A Codex agent is not told twice
- **WHEN** an agent is launched on Codex
- **THEN** its index has no entry for Codex's own summary

### Requirement: A harness's own memory can be turned off for launched agents, unless it is how they write
Where a harness lets a launch turn its own memory off, and that memory is
not the way its agents write facts, the system SHALL turn it off for the
agents the workspace launches when the machine configuration asks it to.
By default it SHALL leave it as the operator configured it. No harness
configuration file SHALL be changed.

#### Scenario: Turned off by configuration
- **WHEN** the machine configuration asks UZE to turn harness memory off and an agent is launched on Codex
- **THEN** that agent runs with Codex's own memory off, and no harness configuration file changed

#### Scenario: Claude is never turned off
- **WHEN** the machine configuration asks UZE to turn harness memory off and an agent is launched on Claude
- **THEN** Claude's own memory stays on, because it is how Claude agents write facts
