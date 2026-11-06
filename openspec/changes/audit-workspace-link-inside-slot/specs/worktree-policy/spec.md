## ADDED Requirements

### Requirement: A checkout is prepared only inside itself

What the system makes in an isolated checkout from the primary, a declared link
with its directories and a copy of an untracked change, SHALL land inside that
checkout. A directory on its path that the checkout holds as a link, or as
anything but a directory, SHALL stop that entry with a warning naming it, and
nothing SHALL be created or written where the link points.

#### Scenario: The target holds a declared link's directory as a link

- **WHEN** a declared link's directory is a real directory in the primary and a
  link in the tree an agent's checkout is cut from
- **THEN** the declared link SHALL NOT be made, and nothing SHALL be created
  where that link points
- **AND THEN** the checkout's warnings SHALL name the declared link

#### Scenario: A declared link sits in a directory the checkout lacks

- **WHEN** a declared link's directories exist in the primary, ignored, and not
  in the checkout
- **THEN** they SHALL be made inside the checkout and the link made there, with
  no warning

#### Scenario: An untracked change would be carried through a link

- **WHEN** an untracked file of the primary is carried into a checkout whose tree
  holds a link at the file or at a directory on its path
- **THEN** nothing SHALL be written where the link points
- **AND THEN** the carry SHALL report that file, and every other change SHALL
  still travel
