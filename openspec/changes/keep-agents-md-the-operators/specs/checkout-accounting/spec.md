## MODIFIED Requirements

### Requirement: Content UZE derives never parks a checkout
Judging a checkout free or parked, the system SHALL NOT count as work an
uncommitted change confined to content UZE derives: a lock file change
moving no pin (each plugin in both locks keeps its revision and digest). A
checkout with only such changes SHALL be free if nothing else holds it, and
reuse SHALL discard them. A moved pin is work. Rebase, join and delivery
SHALL still require a working tree with no uncommitted change.

#### Scenario: A lock rewritten inside a slot
- **WHEN** an agent's checkout ends with the lock file changed only by entries added or dropped, and its branch holds nothing the target lacks
- **THEN** the checkout is free, and the next agent placed there starts from the base's lock file

#### Scenario: A pin moved on purpose
- **WHEN** an agent's checkout ends with the lock file moving a plugin's pin
- **THEN** the checkout holds work and is parked

#### Scenario: A managed region reprojected inside a slot
- **WHEN** `uze install` runs in an isolated checkout whose `AGENTS.md` still carries a retired region
- **THEN** the region is left as committed, nothing in the checkout changes, and the checkout stays free

#### Scenario: A hand edit beside a managed region
- **WHEN** the instruction file has any uncommitted change
- **THEN** the checkout holds work and is parked

### Requirement: The projected policy names the verbs
The launch-instructions section SHALL tell an isolated agent to give its
subagents checkouts through the split verb, to bring their work back
through the join verb, and to list them through the list verb, and SHALL
NOT carry a Git command for making a worktree. It SHALL NOT prescribe a
directory for subagent checkouts.

#### Scenario: The projected text
- **WHEN** an isolated agent is launched
- **THEN** its launch-instructions section names the three verbs and contains no `git worktree add`
