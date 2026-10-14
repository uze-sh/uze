## MODIFIED Requirements

### Requirement: Preserved work is found without the space it was left in
The preserved-work list SHALL answer from UZE's records of every project it
has recorded an agent for, not from the projects the current session
happens to have opened. Work held by an agent that no live pane carries
SHALL be listed whether or not a space is open on its project, and SHALL
remain listed until it is resumed, joined, discarded or forgotten, or its
work lands in the target. An agent that ended with nothing to deliver
SHALL be listed while the conversation it left began within the last
thirty days, since what was discussed is worth coming back to, and
resuming it SHALL cut its branch again from the target.

Because the list crosses projects, each entry SHALL name the project its
work belongs to when it is selected. Two agents carrying the same branch
name in two projects SHALL be distinguishable from the list alone. Work
whose project directory no longer exists SHALL still be listed, and SHALL
say that its project was not found.

#### Scenario: The space the work was left in was closed
- **WHEN** an agent holding work has no live pane and no space is open on its project
- **THEN** its work is listed and, when selected, names the project it belongs to

#### Scenario: A project the session never opened
- **WHEN** the operator opens the list in a session that has opened no space on a project holding preserved work
- **THEN** that project's preserved work is listed, without the operator having opened it first

#### Scenario: Work that is delivered or discarded
- **WHEN** an agent's work has been integrated or discarded
- **THEN** it is not listed

#### Scenario: An agent that ended with only a conversation
- **WHEN** an agent ended with nothing to deliver and its last conversation began within the last thirty days
- **THEN** it is listed, and resuming it places it on its branch cut again from the target, in that conversation

#### Scenario: An old conversation
- **WHEN** an agent ended with nothing to deliver and its last conversation began more than thirty days ago, or it never had one
- **THEN** it is not listed

#### Scenario: Two projects, one branch name
- **WHEN** two projects each hold preserved work on a branch of the same name
- **THEN** both are listed, and each names its own project when selected

#### Scenario: The project was deleted
- **WHEN** the directory of a project holding preserved work no longer exists
- **THEN** its work is listed and says that its project was not found

#### Scenario: The list stays quick as projects accumulate
- **WHEN** the operator opens the list on a machine holding many projects
- **THEN** it is drawn from what UZE recorded, without asking Git about any
  repository, and the operator waits on nothing
