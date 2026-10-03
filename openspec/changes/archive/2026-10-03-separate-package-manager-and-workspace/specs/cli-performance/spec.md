## MODIFIED Requirements

### Requirement: A harness executable is looked for on this machine's own filesystems
When resolving a harness executable — for detection, for its cached
fingerprint, and for the runtime shim's own resolution — UZE SHALL skip
`PATH` entries that live on a filesystem reached over a network protocol
(`9p`, the mount a Windows drive appears as inside WSL). A harness UZE
integrates keeps its state under `$HOME` on this machine; an executable on
such a mount is not it. Whether a harness started through the workspace's
shim is decided from the process that runs, never by walking the operator's
`PATH`.

#### Scenario: A Windows drive on PATH costs nothing
- **WHEN** `PATH` carries entries under a `9p` mount and a harness is not
  installed
- **THEN** resolving that harness's name performs no filesystem access
  under those entries

#### Scenario: The shim check stops at the shims directory
- **WHEN** the shims directory is on `PATH`, or is not, and `uze status` or
  `uze doctor` reports how project context reaches a harness
- **THEN** no `PATH` entry is probed to decide whether a shim is active
