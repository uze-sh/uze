## MODIFIED Requirements

### Requirement: Attachment is a persistent, UZE-managed, user-scope reference
A transparent attachment SHALL be an artifact UZE materializes from the
package in the UZE store into the harness's own user-scope discovery
location. A file UZE renders (a `SKILL.md`, an agent definition, a manifest,
an MCP configuration, a policy sidecar) SHALL be written as a regular file,
and a file UZE carries unchanged SHALL be copied as a regular file; neither
SHALL be a link, and nothing a harness reads SHALL resolve into the store.
UZE SHALL own the artifact's lifecycle through a receipt that proves its
content: it SHALL rebuild the artifact when the store package changes through
UZE and SHALL remove it on `uze remove`/uninstall. The store SHALL remain the
single source of truth; a materialized artifact SHALL be reproducible from
the store and the engine alone.

#### Scenario: Store update is reflected without a rewrite
- **WHEN** a UZE-managed attachment was materialized from a package in the
  UZE store
- **AND WHEN** that store package's content changes through UZE
- **THEN** the harness resolves the updated content without the operator
  rewriting or recreating anything: UZE rebuilds the attachment itself
- **AND THEN** the receipt proves the new content

#### Scenario: Removing a package removes its attachment
- **WHEN** a package with an active transparent attachment is removed from
  the UZE store
- **THEN** UZE removes the managed artifact for every harness that had it
- **AND THEN** nothing is left in any harness's discovery location

#### Scenario: A loose skill keeps its supporting files visible
- **WHEN** a skill with `references/` and `scripts/` is delivered to OpenCode
  through its own global root, `~/.config/opencode/skills`
- **THEN** `~/.config/opencode/skills/<label>` is a regular directory holding the
  rendered `SKILL.md` and copies of those files
- **AND THEN** OpenCode lists them in the skill's `<skill_files>`

#### Scenario: An edited delivered skill is drift, not a silent source
- **WHEN** the operator edits a file inside a delivered skill directory
- **THEN** `uze doctor` reports the attachment Drifted
- **AND THEN** UZE does not remove or overwrite it without the operator's
  action

## ADDED Requirements

### Requirement: A generated directory is replaced whole
When UZE rebuilds a directory a harness reads (a generated plugin, a skill
directory), it SHALL build the new tree beside the destination and replace
the destination in one rename, so a reader observes either the previous tree
or the new one and never a partially written one. A build that fails SHALL
leave the destination as it was and SHALL remove what it staged.

#### Scenario: A session open during install reads a whole plugin
- **WHEN** Claude Code reads a UZE-generated plugin from its directory
  marketplace while `uze install` rebuilds that plugin
- **THEN** every file Claude reads belongs to the same build

#### Scenario: A failed rebuild keeps the delivered tree
- **WHEN** a rebuild fails because a supporting file links outside its package
- **THEN** the previously delivered directory is unchanged
- **AND THEN** no staging directory is left beside it

### Requirement: A receipt from an earlier shape is retired before the current one is attached
When a capability already has a receipt for an integration whose artifact is
a different kind than the one this build plans for it, UZE SHALL inspect the
earlier artifact before attaching: a matched one SHALL be detached and its
receipt forgotten, a missing one SHALL be forgotten, and anything else SHALL
be left in place with the capability reported as held back and the reason.

#### Scenario: A beta.3 skill link becomes a directory
- **WHEN** a skill was attached by an earlier build as a symlink at
  `~/.agents/skills/flow:review`
- **AND WHEN** this build installs the same package
- **THEN** the symlink is removed and a regular directory takes its place
- **AND THEN** the ledger holds only the new receipt

#### Scenario: A changed earlier artifact is not overwritten
- **WHEN** the earlier symlink was repointed by the operator
- **THEN** it is left in place
- **AND THEN** the install reports the skill held back for that harness
