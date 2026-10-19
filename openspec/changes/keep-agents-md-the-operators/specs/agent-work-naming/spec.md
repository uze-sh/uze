## MODIFIED Requirements

### Requirement: The name is asked for as the agent's first action
The launch instructions SHALL ask an agent to name its work as its first
action, before it reads a file, plans, or edits, and the naming clause
SHALL be the first thing those instructions ask for. A name states an
intention, which an agent holds from the request it was given; a moment
reached after the work has begun is one weighed against the work.

#### Scenario: Naming is the first thing the instruction asks for
- **WHEN** a project declares a branch vocabulary and an agent is launched
- **THEN** the naming clause is the first item of its launch-instructions section, and
  it states the moment as the agent's first action rather than as a step
  before some later one

#### Scenario: A name is accepted before the work has a commit of its own
- **WHEN** an agent names its work before writing or committing anything
- **THEN** the branch and label take that name, with no commit required for
  it to be accepted

#### Scenario: A project that names nothing asks for nothing
- **WHEN** a project declares no branch vocabulary
- **THEN** the launch instructions carry no naming clause at all

### Requirement: The agent surface is documented where agents read
Commands whose audience is the agent SHALL be excluded from the help a
person reads and SHALL be documented where an agent reads: the launch
instructions for an agent the workspace launched, and the official plugin's
Skills for any agent. Neither SHALL be written into the project. The launch
instructions SHALL name the project's own vocabulary, so the instruction an
agent reads is the one its project will accept.

#### Scenario: The human help does not carry the agent surface
- **WHEN** a person asks for help
- **THEN** the agent-facing commands are not listed among the commands a
  person is offered

#### Scenario: The projected instruction carries the vocabulary in force
- **WHEN** a project declares its branch vocabulary and an agent is launched
- **THEN** its launch instructions name that vocabulary and the naming
  command, and the next launch after the declaration changes names the new one
