## ADDED Requirements

### Requirement: Windows is a supported platform

UZE SHALL support Windows 10 22H2 (build 19045) and Windows 11, on x86_64
and aarch64, under the same proof every supported platform carries: its
automated checks SHALL build, run, lint, test and perform the product journeys
on Windows hardware of each architecture shipped. Every CLI command and the
terminal workspace SHALL be available there. UZE SHALL require only `git.exe`,
from any distribution, and a terminal that understands VT sequences. No POSIX
shell, POSIX tool, WSL or compatibility layer SHALL be on any path UZE itself
runs.

#### Scenario: The same journeys run on Windows

- **WHEN** the product journeys run on a Windows runner
- **THEN** they perform the same suite as Linux and macOS, from the same
  definitions
- **AND THEN** a journey that cannot run on Windows is declared unsupported
  there with its reason, and is never silently skipped

#### Scenario: A stock Windows account starts uze

- **WHEN** `uze` runs for a user whose environment has no `HOME` variable
- **THEN** the user's home is resolved from the platform, and `$UZE_HOME`
  defaults to `.uze` under it

#### Scenario: No POSIX tool is reachable

- **WHEN** the Windows suite runs with no `sh`, `bash`, `jq` or GNU `tar` on
  `PATH`
- **THEN** every command and the workspace behave as specified

### Requirement: A platform guarantee holds or fails closed

A guarantee UZE states for every platform SHALL hold on every platform. A
guarantee the platform cannot provide SHALL be reported as an error or as
*unknown*, never as success. This applies at least to:

- exclusion between `uze` processes that mutate one `UZE_HOME`;
- naming the process that holds such a lock;
- whether a process is still alive;
- ending a process together with everything it started;
- a deadline on a child process.

#### Scenario: Two processes mutate one home on Windows

- **WHEN** two `uze` processes on Windows try to mutate the same `UZE_HOME`
  at once
- **THEN** one waits for, or refuses on, the other's lock exactly as on
  Linux, and the holder can still be named

#### Scenario: A liveness check that cannot answer

- **WHEN** the platform cannot tell whether a recorded process is alive
- **THEN** the answer is *unknown*, and nothing that depends on the process
  being dead or alive is decided from it

#### Scenario: A network Git operation past its deadline

- **WHEN** a Git network operation on Windows exceeds its deadline
- **THEN** it is ended together with every process it started, and is
  reported as timed out

### Requirement: Paths are compared as the platform compares them

UZE SHALL compare paths by the platform's own rules, case-insensitively on
Windows. A path SHALL compare equal to itself whichever separator it is
spelled with, and whether or not it carries a verbatim (`\\?\`) prefix. A path
UZE shows to a person SHALL use the platform's separator and SHALL NOT carry a
verbatim prefix.

#### Scenario: A checkout reported by Git in another spelling

- **WHEN** Git reports a worktree as `C:/Users/a/proj/.worktrees/x`, and UZE
  recorded it as `c:\users\a\proj\.worktrees\x`
- **THEN** UZE treats them as the same checkout

#### Scenario: A drive-letter path names a local marketplace

- **WHEN** `uze market add C:\plugins\acme` runs on Windows
- **THEN** the argument is read as a local path, not as an SSH `host:path`

### Requirement: The PowerShell installer resolves an asset from the host and fails closed

UZE SHALL publish `install.ps1`, run as `irm https://uze.sh/i.ps1 | iex`. It
SHALL meet the POSIX installer's requirements:

- **Asset resolution:** derive the asset from the host's *native*
  architecture, including under x64 emulation on Arm.
- **Verification:** verify the archive's SHA-256 against `SHASUMS256.txt`
  before placing anything.
- **Refusal before download:** refuse, before downloading, an unsupported
  architecture, a Windows build below 19045, or a host without `git.exe`.
- **Configuration:** honour `UZE_VERSION`, `UZE_BASE_URL` and `UZE_BIN_DIR`.
- **Receipt:** write the same `state/install.json` receipt that
  `uze upgrade` reads.

The installer SHALL use only what Windows PowerShell 5.1 and PowerShell 7
carry. A truncated download SHALL run nothing. A failure SHALL be raised as a
terminating error that places nothing, and SHALL NOT close the user's
session. The installer SHALL add its install directory to the user's `Path`
without rewriting other entries' expansion, and the current session SHALL see
it. It SHALL warn when the host's reputation policy may block an unsigned
binary. It SHALL offer an uninstall that removes the binary, its `Path`
entry and the shims, and stops the workspace server.

#### Scenario: A checksum that does not match

- **WHEN** the downloaded archive's digest differs from its entry in
  `SHASUMS256.txt`
- **THEN** the installer raises an error naming the asset, and nothing is
  placed

#### Scenario: An Arm64 host running x64 PowerShell

- **WHEN** the installer runs in an x64-emulated PowerShell on Windows on Arm
- **THEN** it installs the `aarch64-windows` asset

#### Scenario: An unsupported Windows build

- **WHEN** the installer runs on a build older than 19045
- **THEN** it raises an error naming the build and the minimum, and
  downloads nothing

#### Scenario: The receipt names what was placed

- **WHEN** the installer completes
- **THEN** `state/install.json` names the placed `uze.exe` and the version it
  reported, so that `uze upgrade` recognises it as the installer's binary
