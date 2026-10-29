## MODIFIED Requirements

### Requirement: Which turns ring follows the choice

A turn's end SHALL be the agent's reported `turn.finished`, or, where no
report arrived, the moment the sidebar stops showing it as working, and a
turn SHALL ring only once it has stayed ended for a settle window of about
ten seconds: an agent that resumes work inside that window SHALL NOT ring
for the pause. With Out of sight, a settled turn SHALL ring only while its
tab still carries the finished check. With Always, every settled turn
SHALL ring. With Silent, none SHALL.

#### Scenario: A pause the agent resumes from is silent

- **WHEN** the choice is Always and an agent stops repainting long enough for
  its spinner to stop, then resumes work within the settle window
- **THEN** no sound is made

#### Scenario: A background tab looked at before it settles is silent

- **WHEN** the choice is Out of sight, an agent finishes in another tab, and
  the operator switches to that tab within the settle window
- **THEN** no sound is made

#### Scenario: Out of sight ignores the tab on screen

- **WHEN** the choice is Out of sight and the agent in the focused tab
  finishes its turn
- **THEN** no sound is made

#### Scenario: Out of sight rings for a background tab

- **WHEN** the choice is Out of sight and an agent in another tab finishes
  its turn and stays idle through the settle window
- **THEN** the bell rings once

#### Scenario: Always rings for the tab on screen

- **WHEN** the choice is Always and the agent in the focused tab finishes
  its turn and stays idle through the settle window
- **THEN** the bell rings once

#### Scenario: A reported end rings on every harness

- **WHEN** the choice is Always and an agent of any supported harness reports its turn's end
- **THEN** one sound is made once the settle window passes
