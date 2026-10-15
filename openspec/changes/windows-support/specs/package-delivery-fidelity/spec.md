## ADDED Requirements

### Requirement: A delivered name is valid on the host filesystem

Every file or directory UZE writes for a harness SHALL have a name that the
host filesystem accepts as an ordinary name. On Windows, a name SHALL NOT
contain `< > : " / \ | ? *` or a control character, SHALL NOT end in a dot or
a space, and SHALL NOT be a reserved device name. Writing a label such as
`plugin:capability` SHALL NEVER create an NTFS alternate data stream.

#### Scenario: A label with a colon is never a stream

- **WHEN** a capability labelled `plugin:capability` is written for a harness
  on Windows
- **THEN** every name written is an ordinary name the filesystem accepts
- **AND THEN** no alternate data stream is created

### Requirement: A loose capability on Windows is named `<plugin>-<capability>`

On Windows, a loose skill or agent SHALL be written under
`<plugin>-<capability>`. Two packages that map to the same on-disk name SHALL
fail the install, naming both, before anything is written. The label each
harness shows for that name SHALL be recorded, per harness version, as
measured evidence, and the install report SHALL show it. On-disk names on
Linux and macOS are unchanged.

#### Scenario: A namespaced skill delivered loose on Windows

- **WHEN** skill `review` of plugin `flow` is delivered as a loose skill on
  Windows
- **THEN** its directory is `flow-review`
- **AND THEN** no alternate data stream exists anywhere in the delivered
  tree

#### Scenario: Two packages map to one name

- **WHEN** plugin `a-b` with skill `c` and plugin `a` with skill `b-c` are
  installed on the same Windows machine
- **THEN** the second install fails, naming both packages, and writes
  nothing

### Requirement: A package's bytes are the same on every platform

The Store SHALL hold the same bytes under the same digest for one package on
Linux, macOS and Windows, whatever the host's Git line-ending configuration,
whatever `text`/`eol` attributes the package itself declares, and whether or
not the package's repository contains symbolic links.

#### Scenario: A package with line-ending attributes

- **WHEN** a package whose `.gitattributes` declares `* text=auto` is
  installed on Windows, where Git converts to CRLF by default
- **THEN** its digest equals the digest computed on Linux

#### Scenario: A package containing a symlink

- **WHEN** a package whose repository contains a symbolic link is installed on
  Windows
- **THEN** its digest equals the digest computed on Linux and macOS
