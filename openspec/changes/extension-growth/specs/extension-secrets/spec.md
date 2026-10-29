## Purpose

Secrets an extension uses without ever seeing them drawn, logged or sent.

## ADDED Requirements

### Requirement: A secret is kept in the keychain and only used in a request
A `secret` preference SHALL be stored in the operating system's keychain
(or a `0600` file where there is none), SHALL never be redisplayed,
logged, piped or sent to an agent, SHALL not be read by `{pref.*}`, and
SHALL reach a request only through `fetch`'s `secret` option, never in argv
or the environment; an answer echoing it SHALL be scrubbed before drawing.

#### Scenario: The token never shows
- **WHEN** a forge answers with the token in its body
- **THEN** the drawn answer shows it replaced, and no log line holds it
