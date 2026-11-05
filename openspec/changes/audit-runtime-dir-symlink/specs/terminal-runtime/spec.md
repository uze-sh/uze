## ADDED Requirements

### Requirement: A runtime directory is never reached through a link

Where UZE makes or adopts a private directory for an endpoint or for sockets, it
SHALL ask about the entry itself before creating or changing anything. An entry
that is a link SHALL be refused, and what the link reaches SHALL keep its mode
and its contents.

#### Scenario: A link stands at a shared runtime name

- **WHEN** a link to a directory stands at every shared name for a socket directory
- **THEN** no socket directory SHALL be answered
- **AND THEN** the directory the link reaches SHALL keep its mode

#### Scenario: The endpoint's own directory is a link

- **WHEN** the directory an endpoint is addressed to is a link
- **THEN** the endpoint SHALL be placed somewhere else
- **AND THEN** the directory the link reaches SHALL keep its mode

#### Scenario: The only place to remember a fallback is a link

- **WHEN** every shared name is taken and the address's directory is a link
- **THEN** the endpoint SHALL be refused
- **AND THEN** nothing SHALL be written through the link, and what it reaches SHALL
  keep its mode

#### Scenario: A directory of this user's is open to others

- **WHEN** a real directory this user owns stands at a runtime name with a mode
  that lets others in
- **THEN** it SHALL be narrowed to this user and used
