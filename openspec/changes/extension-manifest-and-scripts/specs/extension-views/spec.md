## MODIFIED Requirements

### Requirement: Data comes from fixtures, and actions are recorded
A source SHALL be fed by the fixture it declares (`fixture "…"`), a JSON
file in the extension's directory, bounded in size, or by its script once
the extension is consented to and enabled in the project. A `Button`'s
action SHALL run the declared `action` of that id; an action the manifest
does not declare SHALL run nothing and be recorded in the inspector with
the arguments it would receive.

#### Scenario: A board drawn from a fixture
- **WHEN** a source declares `fixture "fixtures/board.json"`
- **THEN** the surface is drawn from that file's data

#### Scenario: A button is recorded, not run
- **WHEN** the operator presses a `Button` with `action="next"` and the manifest declares no action `next`
- **THEN** nothing runs and the inspector lists `next` with the item it would have received

#### Scenario: A declared action runs
- **WHEN** the operator presses a `Button` with `action="refresh"` and the manifest declares `action "refresh"` with `refresh "board"`
- **THEN** the `board` source is asked again
