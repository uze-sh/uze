## ADDED Requirements

### Requirement: An agent's pane cannot reach the person's endpoint
Where the kernel scopes abstract Unix sockets (Linux, Landlock ABI 6+), every
pane the runtime launches a program in SHALL start in a confinement neither
that program nor any descendant can leave, which handles no file access, and
the runtime SHALL serve attaching and stopping only at an endpoint that
confinement cannot connect to. A process's session, ancestry, environment or
the key it read SHALL NOT make it the person there.

#### Scenario: An agent leaves its pane's session and offers the key
- **WHEN** a process in an agent's pane starts a helper with `setsid --fork` and the helper offers the runtime's key to attach
- **THEN** the helper cannot connect to the endpoint that serves attaching, and the shared endpoint refuses its attach

#### Scenario: The person attaches as before
- **WHEN** the person starts `uze` from a terminal of their own
- **THEN** the client attaches through the person's endpoint and the agent panes keep working, files and network included

### Requirement: Where panes cannot be sealed, the runtime keeps its layers and says so
Where the kernel offers no such confinement, the runtime SHALL keep refusing
attach and stop to a process it places in a pane and to a client without the
key, and the design SHALL state that a process that leaves its pane's session
and reads the key is still served. The same holds for a shell pane the person
opens, which is not sealed so that `sudo` keeps working in it.

#### Scenario: An older kernel
- **WHEN** the runtime starts on a kernel without abstract socket scoping
- **THEN** it serves attach and stop on its one endpoint, refusing panes and clients without the key, as before this change

### Requirement: A client attaches only to a server no more confined than itself
A client that does not run confined SHALL NOT attach to a server that runs
confined (under `no_new_privs`): such a server was started from inside a
pane. The client SHALL retire it and start one of its own.

#### Scenario: An agent replaced the runtime from its pane
- **WHEN** a server started from inside a sealed pane holds the workspace and the person starts `uze`
- **THEN** that server is retired and the person's client attaches to a server it started

### Requirement: Spaces a pane opens are bounded and not kept until the person visits them
The runtime SHALL hold at most a fixed number of spaces opened through
`OpenSpace` that no client has selected, per requesting pane and in total,
and SHALL refuse further ones. Such a space SHALL NOT be persisted, and so
SHALL NOT be restored or relaunched, until a client selects it.

#### Scenario: A pane opens spaces in a loop
- **WHEN** a pane asks to open 25 spaces, each in a different directory
- **THEN** only the bounded number are opened, the rest are refused, and none of them is in the persisted workspace

#### Scenario: The person visits a space a pane opened
- **WHEN** a client selects a space a pane opened
- **THEN** the space is persisted like any other from then on
