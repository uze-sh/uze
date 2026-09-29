## ADDED Requirements

### Requirement: A marketplace can live below its repository root

The system SHALL accept a marketplace whose `marketplace.json` sits in a
subdirectory of a Git repository, SHALL record that subdirectory with the
marketplace, and SHALL resolve the catalogue and every plugin source relative
to it for mirrored reads, linked reads, the lock and update checks.

#### Scenario: Linked monorepo marketplace

- **WHEN** `uze market add <repo>/aikit` registers a marketplace whose
  manifest is `<repo>/aikit/marketplace.json` with a plugin at
  `./flow`, and the marketplace is linked
- **THEN** `uze install -m flow@<name>` installs `<repo>/aikit/flow`

#### Scenario: Mirrored monorepo marketplace

- **WHEN** the same marketplace is read from its mirror at a commit
- **THEN** the catalogue is read from `aikit/marketplace.json` at that commit
  and plugins from `aikit/<source>`

#### Scenario: A subpath escaping the repository is refused

- **WHEN** a marketplace is added with a subpath that resolves outside its
  repository
- **THEN** registration is refused and nothing is recorded

### Requirement: Adding a marketplace says what will be read

When a marketplace is added, the system SHALL report its identity and, for a
local source, the checkout path and subdirectory that will be read and
whether reads follow the working tree (linked) or commits (mirrored).

#### Scenario: Local checkout with a remote

- **WHEN** `uze market add <repo>/aikit` runs on a checkout whose origin is a
  remote URL
- **THEN** the output names the remote as identity and `<repo>/aikit` as what
  is read
