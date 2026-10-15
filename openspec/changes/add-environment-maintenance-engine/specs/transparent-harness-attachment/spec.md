## MODIFIED Requirements

### Requirement: Attachment is a persistent, UZE-managed, user-scope reference
A transparent attachment SHALL be a reference (such as a filesystem symlink)
UZE creates under the harness's own user-scope discovery location, pointing
at the package's content inside the UZE store, rather than a copy of that
content. UZE SHALL own the reference's lifecycle: it MAY refresh it when the
store package changes, SHALL recreate it during safe maintenance when its
receipt proves it is missing, and SHALL remove it on `uze remove`/uninstall.
UZE SHALL NOT duplicate the store's package content as a second permanent
installation or overwrite a divergent artifact without an explicit operation.

#### Scenario: Store update is reflected without a rewrite
- **WHEN** a UZE-managed attachment references a package already installed in
  the UZE store
- **AND WHEN** that store package's content changes through UZE
- **THEN** the harness resolves the updated content without UZE recreating
  the attachment

#### Scenario: Missing managed reference is restored
- **WHEN** a receipt-owned user-scope reference is missing during maintenance
- **THEN** UZE recreates the reference from the Store and receipt

#### Scenario: Divergent managed reference is preserved
- **WHEN** a receipt-owned user-scope reference points somewhere other than
  its recorded target
- **THEN** UZE does not overwrite it during background maintenance
- **AND THEN** it reports a human-actionable divergence

#### Scenario: Removing a package removes its attachment
- **WHEN** a package with an active transparent attachment is removed from
  the UZE store
- **THEN** UZE removes the managed reference for every harness that had it
- **AND THEN** no dangling reference is left in any harness's discovery

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
