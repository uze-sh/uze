## ADDED Requirements

### Requirement: The runtime reports what it runs
The runtime SHALL answer a usage request with when it started, the window it sampled, and four totals: agent tabs with the shells born of them, the other tabs, the server process itself, and the attached clients. Each total SHALL carry how many tabs or clients it covers, the CPU time spent in the window, memory and process count. The answer SHALL carry no label, identity, pane contents or launch variable.

#### Scenario: An agent tab and its shell
- **WHEN** a client asks for usage while one agent tab, a shell born of it and one other shell are open
- **THEN** the agent total covers one tab and both of their processes, and the shell total covers one tab

#### Scenario: Nothing that names anything
- **WHEN** an agent tab's launch stamped its identity and key
- **THEN** neither appears anywhere in the answer, and neither does any tab or space label

#### Scenario: A process the platform will not describe
- **WHEN** one of a pane's processes ends or cannot be read between listing and measuring
- **THEN** it is left out and the request is still answered

### Requirement: A pane may ask for usage
A connection from inside a pane SHALL be admitted for a usage request without the runtime's key, as for opening a space, and refused every other request as before. Usage SHALL be sampled on the requesting connection's thread, holding the runtime's locks only to copy its tabs and panes. A request arriving while a window is sampling, or within five seconds after it, SHALL be given that window's answer.

#### Scenario: Typed into a workspace shell
- **WHEN** a person runs `uze usage` in a shell pane of the workspace
- **THEN** the runtime answers it

#### Scenario: Panes keep drawing while sampling
- **WHEN** a usage request is sampling its window
- **THEN** pane output still reaches attached clients

#### Scenario: Requests back to back
- **WHEN** several usage requests arrive at once, or one after another within five seconds
- **THEN** the runtime takes one sample for all of them

#### Scenario: Still nothing else from a pane
- **WHEN** a process inside a pane sends `Attach` without the key
- **THEN** it is refused as before
