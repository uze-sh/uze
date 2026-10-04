## MODIFIED Requirements

### Requirement: Portable runtime boundary
The terminal runtime SHALL define transport and PTY boundaries independently of
the host operating system, and SHALL support Linux, macOS and Windows behind
them. On Windows the transport SHALL be a named pipe and the pseudoterminal a
ConPTY. A platform SHALL NOT change workspace, tab, pane, or client/server
lifecycle semantics.

#### Scenario: Initial supported platform starts a session
- **WHEN** a user on Linux or macOS attaches to a terminal workspace
- **THEN** the system SHALL use the platform's local transport and PTY backend
- **AND THEN** the workspace behavior SHALL conform to this specification

#### Scenario: A Windows user starts a session
- **WHEN** a user on Windows attaches to a terminal workspace
- **THEN** the system SHALL use a named pipe and ConPTY
- **AND THEN** the workspace behavior SHALL conform to this specification

### Requirement: The endpoint lives beside the workspace it serves

The endpoint SHALL be named from the workspace's own location, so that
every terminal of one machine computes the same endpoint for one
`UZE_HOME` whatever their session environment says, and no directory a
system cleaner owns can take it while the workspace itself survives. Where
that path cannot hold a socket, UZE SHALL fall back to the session's
runtime directory and the system temporary directories in turn. On Windows
the endpoint SHALL be a named pipe whose name is derived from the
workspace's location and the current user, so no filesystem fallback
applies.

#### Scenario: Two terminals with different session environments

- **WHEN** two terminals of one machine have different values for the
  session's runtime directory, or one has none
- **THEN** both SHALL compute the same endpoint for the same `UZE_HOME`
- **AND THEN** the second SHALL attach to the server the first started

#### Scenario: A home too long for a socket path

- **WHEN** the workspace's own directory would exceed the length a socket
  path allows
- **THEN** UZE SHALL use the first fallback directory that can hold one
- **AND THEN** the endpoint SHALL still be one per `UZE_HOME`

#### Scenario: Two Windows terminals compute one pipe

- **WHEN** Windows Terminal and a VS Code terminal of the same user start
  `uze` for the same `UZE_HOME`
- **THEN** both SHALL compute the same pipe name and attach to one server

## ADDED Requirements

### Requirement: The endpoint admits only its own user

On every platform, the endpoint SHALL accept a connection only from the OS
user who owns the server, and a client SHALL attach only to a server running
as its own user. On Windows the pipe SHALL refuse remote clients, and SHALL
be created as the first instance of its name, so another user cannot hold the
name before the server does.

#### Scenario: Another user connects

- **WHEN** a process running as a different OS user connects to the endpoint
- **THEN** the connection SHALL be refused before any request is read

#### Scenario: A squatted pipe name

- **WHEN** a pipe of the endpoint's name already exists and is not held by a
  `uze` server of the same user
- **THEN** the client SHALL NOT attach to it, and SHALL report who holds it

### Requirement: The server outlives the console that started it

The terminal server SHALL keep running, and keep every pane's process
running, when the terminal window, tab or console session that started it
closes. On Windows it SHALL run detached from that console and outside any
job object that would end it with its launcher.

#### Scenario: Closing the Windows Terminal tab that started the server

- **WHEN** a user on Windows closes the tab whose `uze` started the server
- **AND WHEN** they run `uze` again in another tab
- **THEN** the new client SHALL attach to the same server with every pane's
  process still running

### Requirement: A pane ends with everything it started

Closing a pane, or stopping the server, SHALL end the pane's process and
every process it started, on every platform, so that no descendant keeps the
pseudoterminal open after the pane is gone.

#### Scenario: A pane whose agent spawned helpers

- **WHEN** a pane running an agent that started child processes is closed on
  Windows
- **THEN** the agent and every descendant SHALL be ended
- **AND THEN** the pane's resources SHALL be released
