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

#### Scenario: A harness with no shim is not a bypass
- **WHEN** a harness whose setup could not place its shim is started in a
  pane
- **THEN** nothing is said about a bypass, since the plain binary was the
  only thing there was to start

### Requirement: The workspace sets up this machine's harnesses before it opens
Before its first frame the workspace SHALL decide what it needs from the
harnesses this machine has, never from whether UZE ran on it before. A
harness is set up for the workspace when a setup verified its executable
and its shim is in place; what every command prepares on its way through
makes a harness able to receive plugins and is not that. A harness that is
installed and not set up SHALL be set up from the executable already there,
without asking and without taking the vendor's update route. Only a machine
with no harness installed SHALL be asked which harnesses to provision, and
that answer takes the vendor's official route.

#### Scenario: A clean install on a machine that already has harnesses
- **WHEN** the workspace opens with no harness set up and harnesses
  installed on the machine
- **THEN** each installed harness is set up without a question, its shim is
  placed, no vendor install or update route runs, and the workspace opens
  on the report, pausing only when something failed or warned

#### Scenario: A machine with no harness
- **WHEN** the workspace opens and no harness is installed on the machine
- **THEN** the person is asked which harnesses to provision, before
  anything is recorded or a shim placed
- **AND** each harness picked goes through its vendor's official installer,
  and no other installer runs
- **AND** declining the question sets up nothing, and the workspace opens

#### Scenario: The terminal runtime ran first
- **WHEN** the terminal runtime already laid out its own directory under
  UZE's state (for instance `uze workspace stop` ran before the first
  open), and no harness is set up
- **THEN** the workspace still sets up the installed harnesses, and still
  asks a machine with none

#### Scenario: A harness installed later, or a shim removed
- **WHEN** the workspace opens and a harness installed since the last setup,
  or one whose shim is gone, is found
- **THEN** that harness alone is set up, and the others are left as they are
