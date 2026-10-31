## Context

`uze-platform::confine` applies a `Confinement` to the `uze __confine`
helper and then becomes the program: Landlock plus seccomp on Linux,
`sandbox-exec` on macOS, nothing elsewhere. `Availability` reports what the
machine enforces, field by field. Only `summary()` and `is_available()`
read it outside the crate, in `uze doctor`, `uze status`, consent and the
runner. The runner already starts the helper in a Job Object of its own on
Windows (`process::spawn_tree`) and ends that job past any bound.

What follows was measured before it was decided. Windows Sandbox
(Windows 11 24H2) ran the probe as its administrator and as a standard
account. The `confinement-lab` workflow ran it on `windows-2025`,
`windows-11-arm`, `macos-15` and `macos-15-intel`. The probe's source
lived under `lab/` in this change and is removed with it.

## Goals / Non-Goals

**Goals:**

- Windows enforces every outcome Linux enforces that it has a mechanism
  for, through the same `Confinement`, with nothing asked of the operator.
- `Availability` says, on every platform, what a confined program cannot
  do, never which mechanism refused it.

**Non-Goals:**

- Device grants on Windows. Windows has no device paths to grant, so a
  confinement that lists `devices` is refused there rather than run with
  fewer rights.
- A memory bound on macOS (D7).
- Removing the ACEs a grant leaves (D3), and LPAC (D2).

## Decisions

### D1. One AppContainer profile per run

The helper creates a profile named `uze.run.<pid>.<n>`, starts the
program in it and deletes it after the program ends. Creating one took
5 to 10 ms and deleting one about 3 ms. A run's `%TEMP%` and
`%LOCALAPPDATA%` therefore belong to that run, as its scratch does on
Linux, and nothing one extension's run writes there reaches the next.
`DeriveAppContainerSidFromAppContainerName` alone does not do this:
`CreateProcess` fails with "file not found" when no profile is
registered.

A run that is killed leaves its profile behind. The next helper removes
any `uze.run.*` profile whose process no longer runs, read from the
`AppContainer\Mappings` monikers under `HKCU`.

- Rejected: one fixed profile for every run. Its folder outlives the
  run, which would give programs of different extensions a place to
  leave things for each other.

### D2. AppContainer, not LPAC

Under LPAC the probe could not start `cmd.exe` and Winsock failed to
initialise until `registryRead` was added. PowerShell still failed with
`registryRead`, and hung with `lpacInstrumentation`. Plain AppContainer
ran `cmd`, PowerShell, git and the probe itself, and refused every reach
LPAC refused except binding a socket that nothing can then reach. A
listener's port takes no inbound connection, and without network no
connect succeeds (`WSAEACCES`), DNS included. The outcome matches Linux.

### D3. A grant is a capability SID with an inheritable ACE

Each `(right, path)` derives a capability SID with
`DeriveCapabilitySidsFromName("uze.<right>:<path>")`. The path gets an
inheritable allow ACE for that SID, and the run's token carries exactly
the capabilities of its own confinement. The rights are:

- `read`: `FILE_GENERIC_READ`;
- `read_execute`: adds `FILE_GENERIC_EXECUTE`;
- `write`: read plus `FILE_GENERIC_WRITE`, `DELETE` and
  `FILE_DELETE_CHILD`, and never execute.

The ACE stays. A process holds such a capability only when the user's
own process creates it with one, so the ACE widens nothing for anybody
else. Propagating it over 5,000 files took about 380 ms. A path whose
DACL already carries the ACE is skipped without propagating. Files Git
writes later inherit it. A path the user cannot rewrite the DACL of
(`Program Files`, `C:\Windows`) needs no ACE when it already grants
`ALL APPLICATION PACKAGES` the right; otherwise the run is refused.

- Rejected: granting and revoking on every run. Propagation would be
  paid twice per run, and a run that is killed would leave the ACE
  anyway.
- Rejected: deny ACEs to carve out exceptions. Neither a capability SID
  nor the container SID made a deny ACE take effect.

### D4. The Job Object bounds the run

The helper starts the program suspended in a new job. The job has
`KILL_ON_JOB_CLOSE`, `PROCESS_MEMORY` set to `address_space`,
`PER_PROCESS_USER_TIME` set to `cpu_seconds`, and `DIE_ON_UNHANDLED_EXCEPTION`.
The helper then resumes the program and waits for it, and exits with the
program's code. The job nests inside the runner's own, so ending the
runner's job ends this one too. A commit of 512 MiB under a 256 MiB limit
was refused. Windows has no limit on file size or on open handles, which
`Availability` says.

### D5. The standard handles are the only ones inherited

`PROC_THREAD_ATTRIBUTE_HANDLE_LIST` names the three standard handles, so
nothing else the helper holds reaches the program. The program inherits
the helper's environment as it is. Windows itself points `TEMP`,
`TMP` and `LOCALAPPDATA` into the run's profile.

### D6. `Availability` names outcomes

The fields become `truncate`, `network`, `listening`, `devices`,
`processes`, `tree`, `memory`, `cpu`, `file_size` and `open_files`, each saying
what a program cannot do or which resource is bounded. On Linux,
`listening` is the seccomp filter refusing `listen` at every ABI,
`devices` is ABI 5 and `processes` is ABI 6. `tree` is the filter
refusing `setsid` and `setpgid` on Linux and the job on Windows; Seatbelt
filters no system call, so on macOS a program can leave its run's group. The ABI 6 rule stays
conservative: below it the filter checks signal targets only as far as an
argument check can. `summary()` names the mechanism once, then what a
program can still reach and which limits are missing, in plain words
(`confined by AppContainer, without a file size or open files limit`).

### D7. macOS keeps no memory bound

On `macos-15` a process starts with about 392 GiB of address space
already mapped. `setrlimit(RLIMIT_AS)` accepts only values above that,
and then refuses only `mmap`: a 3 GiB `malloc` succeeded under a limit
of the current size plus 2 GiB. `RLIMIT_DATA` is refused, and
`memorystatus_control` answers `EPERM`. None of these bounds memory, so
`memory` stays `false` on macOS and the summary says so.

### D8. Part of HKCU stays readable, and the security page says so

Every app container can read `HKCU\Environment`, and parts of
`HKCU\Software\Microsoft`, through `ALL APPLICATION PACKAGES`. A key the
user creates is not readable. A deny ACE does not apply (D3), so this
cannot be closed. The security page says that a variable saved with
`setx` is readable by a confined program on Windows.

## Risks / Trade-offs

- [Propagating an ACE over a large checkout (`node_modules`) on the
  first run] → it is paid once per path and later runs skip it. A time
  limit that ends the first run reports a timeout, and the next run
  starts with the grant in place.
- [A user who cannot change a path's DACL] → the run is refused, with
  the path named, and never runs with less than it was granted.
- [A program that needs a path outside its grants, such as a tool
  installed under the profile] → it fails, as on Linux. Its directory is
  pinned at consent and granted `read_execute`, as `program.rs` already
  does for a pinned program's directory.
- [A profile left behind when a helper is killed] → the next helper
  removes it (D1).
- [A directory granted for reading must still be entered] → its own entry
  carries `FILE_TRAVERSE` and only its files get the entry without it,
  since on a file that bit means execute. The lab found this when Git
  could not `chdir` into a checkout it was granted.
- [`NUL` on Windows Server] → Server 2025 does not let an app container
  open it, while Windows 11 does. A program that opens `NUL` itself fails
  there. The security page says so.
- [`cmd`'s `dir` and `cd /d`] → these read the attributes of every parent
  directory, which a container is not granted, so they fail where
  PowerShell, Git and Python succeed. Granting metadata on every parent
  would rewrite the DACLs of directories the operator never granted, so
  it is not done.

## Candidate ADRs

- Windows confinement by a per-run AppContainer with persistent
  capability ACEs. It leaves durable ACEs on the operator's directories,
  which is hard to undo once released.
