## MODIFIED Requirements

### Requirement: A replacement is verified before it is placed

UZE SHALL verify a downloaded release against the published checksums, SHALL
run the binary it contains and confirm it reports the release it was
downloaded as, and SHALL leave the installed file untouched when either check
fails. The replacement SHALL be a single rename on the installed file's own
filesystem, so no process can start a partially written binary.

Where the platform refuses to rename over a running executable (Windows):

1. the running image SHALL first be renamed aside, in the same directory;
2. the new file SHALL then be renamed into place;
3. the image set aside SHALL be removed by the first `uze` that starts after
   nothing runs it any more.

If the second rename fails, the first SHALL be undone.

#### Scenario: A checksum that does not match

- **WHEN** the downloaded archive's digest differs from the published one
- **THEN** the installed binary is unchanged and no notice claims an update

#### Scenario: A binary that is not the release it claims

- **WHEN** the unpacked binary does not report the release it was downloaded as
- **THEN** the installed binary is unchanged

#### Scenario: Replacing a running `uze.exe`

- **WHEN** `uze upgrade` runs on Windows while the installed `uze.exe` is the
  running image and the terminal server also runs from it
- **THEN** the installed path holds the new release, and both running
  processes are untouched
- **AND THEN** the next `uze` started once neither old process remains
  removes the image that was set aside

#### Scenario: The swap fails halfway

- **WHEN** the new file cannot be renamed into place after the running image
  was set aside
- **THEN** the image set aside is renamed back, and the installed path still
  runs the release it ran before
