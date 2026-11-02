## ADDED Requirements

### Requirement: A trace never carries a credential the operator typed
A span field, an event, a recorded error and a text log line SHALL NOT
carry the userinfo of a URL the operator supplied (a marketplace source, a
plugin spec, a Git argument) nor a token a provider minted; each SHALL show
`<redacted>` in its place and keep the rest of the value.

#### Scenario: A credentialed marketplace URL is added
- **WHEN** `uze market add https://user:token@host/org/market.git` runs
  with tracing subscribed, at any level
- **THEN** no span, event or log line contains `user:token`, and the
  spans show `https://<redacted>@host/org/market.git`

#### Scenario: Git is run with a credentialed argument
- **WHEN** Git is run with an argument carrying URL userinfo
- **THEN** the Git span records the argument with the userinfo redacted
