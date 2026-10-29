## ADDED Requirements

### Requirement: Overrides do not cross harnesses
The system SHALL launch an agent continued on another harness without the
overrides it carried, since a flag belongs to one harness, and SHALL say on
its tab that they were left behind. Switching back SHALL launch it with the
overrides it last had on that harness.

#### Scenario: Continuing an agent launched with a model flag
- **WHEN** an agent launched on one harness with `--model opus` is continued in another harness
- **THEN** the new harness starts with no overrides and the tab says the earlier ones were left behind

#### Scenario: Switching back
- **WHEN** that agent is later continued in its first harness again
- **THEN** it starts with `--model opus` again
