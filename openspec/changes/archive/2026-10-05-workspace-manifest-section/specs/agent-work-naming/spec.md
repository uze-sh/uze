## MODIFIED Requirements

### Requirement: The project declares the vocabulary and every name is validated against it
A project SHALL declare which branch types its names may use, as
`workspace.branch`: a named preset or its own list. A proposed name SHALL be accepted only when its
type is in that vocabulary and its subject is a well-formed single path
segment. A refusal SHALL say which half was wrong, so the next attempt is
informed rather than guessed.

#### Scenario: A name outside the vocabulary is refused
- **WHEN** an agent proposes a type the project does not declare
- **THEN** the name is refused, naming the declared vocabulary, and the
  branch is unchanged

#### Scenario: A malformed subject is refused
- **WHEN** a proposed subject is empty, carries a path separator, or is
  longer than the subject limit
- **THEN** the name is refused with that reason

#### Scenario: A name already taken is refused
- **WHEN** the proposed branch name already exists in the repository
- **THEN** the name is refused rather than silently disambiguated

#### Scenario: An undeclared vocabulary keeps today's behavior
- **WHEN** a project declares no branch vocabulary
- **THEN** branches keep the generated identifier exactly as before, and
  a naming call is refused saying the project must declare
  `workspace.branch`
