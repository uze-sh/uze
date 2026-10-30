## ADDED Requirements

### Requirement: A conversation id from outside UZE is validated before it is used
A conversation id read back from a harness's store or from UZE's own record
SHALL be accepted only when it is non-empty, bounded, begins with a letter
or digit and holds only letters, digits, `-` and `_`, and, when read from a
harness, has that harness's own shape. Any other value SHALL be ignored, as
if no conversation had been found.

#### Scenario: An id shaped like an option is never resumed
- **WHEN** a harness's store names a conversation whose id begins with `-`
  for the agent's checkout
- **THEN** the id is not recorded, and the next launch does not pass it to
  the harness

#### Scenario: An id with a path separator is refused
- **WHEN** an id holds `/`, `\`, whitespace or a control character
- **THEN** it is not accepted as a conversation id
