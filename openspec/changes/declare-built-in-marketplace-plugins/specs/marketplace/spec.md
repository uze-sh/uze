## ADDED Requirements

### Requirement: An earlier release's official records move under uze
Plugins an earlier release recorded under the official marketplace's old
name, `uze-official`, SHALL be taken off as a removal takes them and
installed again under `uze`, once, before anything is seeded.

#### Scenario: A machine seeded by an earlier release is moved
- **WHEN** a machine holds `uze@uze-official` and `workspace@uze-official`
- **THEN** after the next command it holds `uze@uze` and `workspace@uze`,
  and nothing under `uze-official`
