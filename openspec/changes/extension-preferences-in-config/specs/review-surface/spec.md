## ADDED Requirements

### Requirement: A file read to the end is marked viewed as it is left
While `mark-viewed-automatically` is on, leaving a file for another or
closing the review SHALL mark it viewed when its diff's last line was on
screen and the diff was shown for at least two seconds. A file the
operator unmarked by hand SHALL not be marked again until its content
changes, and a file whose content Git cannot vouch for SHALL not be
marked.

#### Scenario: A diff read to its end is marked on the way to the next file
- **WHEN** the operator scrolls a diff to its last line, stays on it and goes to the next file
- **THEN** the file they left is marked viewed and the count rises by one

#### Scenario: A file only glanced at is not marked
- **WHEN** the operator opens a file and leaves it at once
- **THEN** it stays not viewed

#### Scenario: Turned off, nothing is marked but by hand
- **WHEN** `mark-viewed-automatically` is off and a diff is read to its end and left
- **THEN** it stays not viewed
