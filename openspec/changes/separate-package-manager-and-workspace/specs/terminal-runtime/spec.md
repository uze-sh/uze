## ADDED Requirements

### Requirement: The workspace reaches harnesses through its own shims
The workspace SHALL start every pane with the shims directory ahead of any
other entry on `PATH`, and SHALL launch an agent through its harness's shim
by absolute path, so what the workspace carries into a harness (session
continuity, project resources a harness does not read natively) does not
depend on the person's shell configuration. A pane whose shell moved another
entry ahead of the shims SHALL say so when a harness is started from it.

#### Scenario: An agent launched from the workspace
- **WHEN** an agent is launched from the workspace
- **THEN** it is started through its harness's shim whatever the person's
  `PATH` holds outside the workspace

#### Scenario: A harness typed into a pane
- **WHEN** a person types a harness's command in a workspace pane whose
  shell kept the shims first
- **THEN** it starts through the shim

#### Scenario: A shell that reordered PATH
- **WHEN** a pane's shell startup moved another directory holding the
  harness ahead of the shims, and the harness is started from that pane
- **THEN** the pane states that the harness bypassed the workspace's shim,
  and what that costs
