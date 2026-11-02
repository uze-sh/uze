## MODIFIED Requirements

### Requirement: A version change is written once, as a step between shapes

What a version change means SHALL be written in one place, as a step from
one shape to the next, so that the knowledge lives where the next document
can reach it rather than in one file's own prose. Each step SHALL name the
last release whose run could leave the shape it reads, or be unbounded
while a record in it is rewritten only when next changed, and SHALL be
removed once the supported floor is above that release.

#### Scenario: A document changes shape

- **WHEN** a change gives a document UZE owns a new shape
- **THEN** what the change means SHALL be written as a step from the
  previous shape to the new one, beside the steps before it
- **AND THEN** every reader of the document SHALL reach it through those
  steps rather than through knowledge kept in its own code

#### Scenario: No machine can be below a step

- **WHEN** the supported floor is raised above the last release that wrote
  the shape a step starts from
- **THEN** that step SHALL be removed, without changing what any build does
  with a document from a release at or above the floor
