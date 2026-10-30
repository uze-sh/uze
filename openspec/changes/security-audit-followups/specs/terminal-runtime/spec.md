## ADDED Requirements

### Requirement: A client retires only a server that started inside a pane

A server SHALL record, when it starts, whether it was started from inside a pane the runtime confined, and a client SHALL retire a running server only when that record says so. A server started by a person in a sandboxed terminal SHALL NOT be retired by the same person's client from another terminal.

#### Scenario: A sandboxed person's server survives an ordinary terminal
- **WHEN** a person starts uze in a sandboxed terminal, places agents, and then opens uze from an ordinary terminal
- **THEN** the second client attaches to the running server and every agent keeps running

#### Scenario: A server an agent started is still retired
- **WHEN** a server was started from inside a confined pane and the person's unconfined client arrives
- **THEN** the client retires that server and starts one of its own
