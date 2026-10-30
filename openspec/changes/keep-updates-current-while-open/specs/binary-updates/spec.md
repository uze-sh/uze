## ADDED Requirements

### Requirement: A release channel decides when a found release installs

UZE SHALL read `UZE_RELEASE_CHANNEL`. On `latest`, the default, the
background pass SHALL install a newer release as soon as a check finds it.
On `stable` it SHALL install a release only once it has been the latest for
a day. `uze upgrade` SHALL ignore the channel's wait.

#### Scenario: The default channel

- **WHEN** `UZE_RELEASE_CHANNEL` is unset and a check finds a newer release
- **THEN** the pass installs it in the same check

#### Scenario: The stable channel

- **WHEN** `UZE_RELEASE_CHANNEL` is `stable` and the newer release was
  published an hour ago
- **THEN** the pass does not install it yet

### Requirement: An open client checks on a short tick and asks rarely

An open workspace client SHALL wake about every 15 minutes, jittered, and
SHALL ask for the latest release only when the machine's last answer is
over an hour old and no other process is asking. A process that asks SHALL
record the attempt before it reaches the network.

#### Scenario: A client open for days

- **WHEN** the client has been open longer than an hour and the last answer
  is over an hour old
- **THEN** the next tick asks, and an installed release's notice appears
  without restarting the client

#### Scenario: Two clients

- **WHEN** two clients wake while the answer is stale
- **THEN** one asks and the other reads what it recorded

#### Scenario: Offline

- **WHEN** the request fails
- **THEN** no process asks again until the hour has passed

### Requirement: A release check downloads no page

The check for the latest release SHALL read the release host's redirect
without following it.

#### Scenario: Asking for the latest release

- **WHEN** a check asks for the latest release
- **THEN** the version is read from the redirect's location and no page
  body is downloaded
