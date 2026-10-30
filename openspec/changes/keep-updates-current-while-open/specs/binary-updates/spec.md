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

### Requirement: The workspace restarts into an installed release

While a release is installed and not running, the workspace SHALL offer to
restart into it, first at the right end of the bar under the agent area,
showing the bar for it. Restarting SHALL restore the terminal, run the
installed binary in the client's place on Linux, macOS and Windows, and
reattach to the running terminal runtime where the operator was. It SHALL
NOT stop the runtime or any pane.

#### Scenario: Restarting from the notice

- **WHEN** the operator activates the restart button at the right end of
  the bar under the agent area, which names the version it runs
- **THEN** the client is replaced by the installed release, which attaches
  to the same runtime and shows the space and tab that were in front

#### Scenario: A pane during the restart

- **WHEN** an agent is running in a pane and the operator restarts
- **THEN** the agent keeps running, and the restarted client shows it

#### Scenario: From the keyboard

- **WHEN** a release is installed and the operator opens the action index
- **THEN** it offers the restart, and choosing it restarts as the row does

## MODIFIED Requirements

### Requirement: The workspace sidebars announce releases

Both sidebars SHALL show, above the sections at their foot, a notice for
the release now running that the updater put in place, until it is
dismissed, and for nothing else. The notice SHALL name the version and say
what is new in it can be read. Activating it SHALL open that release's own
section of the `CHANGELOG.md` in a modal; dismissing it SHALL put it away
for that release in every client and every later run.

#### Scenario: A release waiting on a restart

- **WHEN** the updater replaced the binary and this client runs the older
  release
- **THEN** neither sidebar shows a notice, and the bar offers the restart

#### Scenario: After the restart

- **WHEN** the client runs the release the updater put in place
- **THEN** the sidebars show its version and "see what's new"

#### Scenario: A release that is only available

- **WHEN** a newer release is published and has not been installed
- **THEN** neither sidebar shows a notice, and `uze upgrade` is where a
  person asks about it

#### Scenario: A release this binary will not install

- **WHEN** a newer release is published and the running binary is not the
  one a receipt names
- **THEN** neither sidebar shows a notice, and `uze upgrade` is where the
  reason is told

#### Scenario: Opening the notes

- **WHEN** the operator activates the notice
- **THEN** a modal opens on the notes of the version it names, rendered
  rather than shown as Markdown, and shows no other release

#### Scenario: Notes that cannot be read

- **WHEN** the changelog cannot be fetched, or does not carry the release
- **THEN** the modal says so and still opens the release page on request

#### Scenario: Dismissing it

- **WHEN** the operator dismisses the notice
- **THEN** it disappears from both sidebars and does not return for that
  release, and a restart the bar offers stays

