## ADDED Requirements

### Requirement: The installer installs only the release it resolved
The installers SHALL resolve `latest` to a concrete release before
downloading and SHALL download from that release's own path. Before placing
the binary they SHALL run it and confirm it reports that release, or the
pinned one; otherwise they SHALL refuse, leave any installed binary as it
was, and name both versions. The receipt SHALL record the verified release.

#### Scenario: A signed older build served under a newer release
- **WHEN** the latest release resolves to `v2` and its signed archive holds
  a binary reporting `1`
- **THEN** the installer refuses, names both versions, and installs nothing

#### Scenario: A pinned release that is not what it says
- **WHEN** `UZE_VERSION` names a release whose binary reports another
  version
- **THEN** the installer refuses and installs nothing

#### Scenario: Latest cannot be resolved
- **WHEN** the release host does not answer which release is the latest
- **THEN** the installer refuses before downloading anything
