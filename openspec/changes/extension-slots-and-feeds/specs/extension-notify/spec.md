## Purpose

What an extension may say outside its surfaces: a notification or a sound
an answer asks for, attributed to it and rate-limited by the host.

## ADDED Requirements

### Requirement: An answer may ask for a notification or a sound
An answer's envelope MAY carry `notify` with a `title` and a `body`, and
`sound` with one of `done`, `attention` or `error`. The host SHALL apply
them only for an extension holding `notify`, attributed to it, and SHALL
drop either without the grant, naming the grant in the surface's notice.

#### Scenario: A request without its grant is dropped and named
- **WHEN** an answer asks for `notify` and the extension does not hold `notify`
- **THEN** nothing is notified, the rest of the answer applies, and the notice names the missing grant

#### Scenario: A notification says whose it is
- **WHEN** an extension holding `notify` answers `notify: {title: "Done", body: "Phase 12"}`
- **THEN** the notification's title begins with the extension's attribution

### Requirement: Notifications are rate-limited per extension
The host SHALL send at most one notification per extension every 10
seconds and 30 per hour, dropping the excess and counting it in the
surface's notice.

#### Scenario: A burst is cut down
- **WHEN** an extension asks for five notifications within one second
- **THEN** one is sent and the notice says four were dropped
