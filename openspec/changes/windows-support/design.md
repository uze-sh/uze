## Context

See proposal.md for why. This section covers the state of the code.

**What does not compile.** Every crate except `uze-document`, `uze-theme` and
`uze-extensions` either fails to compile for `*-pc-windows-msvc` or compiles
into placeholder behaviour:

- `uze-terminal` imports `std::os::unix::net` crate-wide. Its public
  `attach()` returns a `UnixStream`. Panes are tracked by `libc::pid_t` and
  ended with `kill(-pgid)`.
- Ungated Unix imports remain in `uze-core` (`package/authoring.rs:351`),
  `src/self_update.rs` and `uze-testkit`.

**What compiles but only pretends.** These `cfg(not(unix))` branches are
placeholders:

| Code | Non-Unix behaviour today |
|---|---|
| `persistence::try_lock_exclusive` | `Ok(())`. It backs `MutationLock`, `AgentsMdGuard` and the task store |
| `uze-git` `lock::try_lock` | `Ok(())` |
| `process_is_alive` | `true` |
| `create_symlink` | always errors |
| `uze-git` `run_within` | loses its deadline |
| `kill_reaped_process_group` | runs `taskkill` on a pid that may be reused |
| `run_shell_bounded` | spawns `sh` |
| `UzeHome::from_env` | requires `HOME` |
| `resolve_real_executable` | ignores `PATHEXT` |

**What is already portable:**

- the terminal wire protocol (length-prefixed bincode over any `Read`/`Write`);
- `portable-pty` (ConPTY);
- `crossterm` and `ratatui`;
- `shared/path.rs`;
- the CRLF-aware text regions.

**Build and release:** no dependency compiles C on the shipped path.
`onig_sys` is gone from `Cargo.lock`, and the `release.yml` musl workaround is
stale. `windows-sys` 0.61 is already in the tree, transitively.

**Constraints the dependency graph imposes:**

- `uze-core` depends on `uze-git`. `uze-terminal` depends on nothing in the
  workspace but `uze-document`. Neither can reach a process primitive that
  lives in `uze-core`.
- `src/` may not name `uze_core::`.

**Vendor facts, checked 2026-10-03:**

- All four harnesses run natively on Windows. Claude Code does not need Git
  Bash.
- Each harness spawns a hook command differently:
  - Claude: Git Bash when present, else PowerShell. There is also an exec
    form, `command` plus `args`, with no shell.
  - Codex: always PowerShell, with a `commandWindows` field.
  - Antigravity: `cmd /C`, with broken quote escaping.
- On Windows, Codex does not fire `PreToolUse` for shell commands
  (openai/codex#24453).
- OpenCode has no PowerShell installer. Its routes are scoop, choco and npm.
- Claude's sandbox is not supported on native Windows.

## Goals / Non-Goals

**Goals:**

- One code path per decision. Platform differences live behind boundaries,
  never at call sites:
  - the new `uze-platform` leaf crate;
  - `persistence`;
  - `UzeHome`;
  - the terminal transport.
- The full `cargo test --workspace` suite runs on Windows, under PowerShell,
  with no POSIX tool reachable. Unix-only tests are gated one by one, each
  with its reason.
- Bytes and digests are identical across the three platforms.
- No new third-party crate beyond `windows-sys`. CI-only Python packages for
  the journey runner are allowed.

**Non-Goals:**

- Windows builds older than Windows 10 22H2 (build 19045).
- The legacy console host without VT processing.
- Authenticode signing. The consequences are documented (SmartScreen,
  Smart App Control) and detected.
- The Conformance Lab on Windows.
- A native clipboard fallback, since Windows Terminal honours OSC 52.
- MSYS/Cygwin builds.
- Translating POSIX command lines.
- Changing on-disk names on Linux or macOS.

## Decisions

### D1 — Compile first, and guard it from the first commit

The Linux `lint` job runs, from the first PR of the change:

- `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc`;
- `cargo clippy --all-targets -D warnings` for both
  `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`.

This needs no MSVC linker: `check` never links, and `windows-sys` ships import
libraries.

The order is fixed:

1. The transport port (D4) is extracted first, as a Linux-proven refactor.
2. Group 1 then makes the build green.
3. The Windows test row joins CI as non-gating.
4. That row becomes gating once the suite is green (D19).

### D2 — Win32 through `windows-sys`, nothing else

`windows-sys` is published by Microsoft from `microsoft/windows-rs`, is
already in `Cargo.lock`, and compiles no C. It becomes a
`[target.'cfg(windows)'.dependencies]` entry, with only the features each
crate names, in these crates:

- `uze-platform`
- `uze-core`
- `uze-git`
- `uze-terminal`
- the binary crate

*Alternatives considered:*

- `interprocess`: single maintainer, and it hides exactly the pipe security
  and overlapped-I/O decisions D5 has to make.
- `uds_windows`: weaker peer identity, and it contradicts ADR-038.
- `dunce` and `home`: each is one function.

### D3 — A `uze-platform` leaf crate owns what differs per platform

The dependency graph forbids putting process primitives in `uze-core`, since
`uze-git` and `uze-terminal` both need them. A new leaf crate, `uze-platform`,
depends on nothing in the workspace and names no domain, no path and no
harness. Every module in it is one concept with one API, and an
implementation per platform selected by `cfg` at the module's boundary, so
no call site in the workspace branches on the operating system. Besides the
shell (D10, D11), home and paths (D15), names Windows can hold (D16),
filesystem privacy and links, the running image's replacement (D14) and
stdio, it holds four process concepts:

| Item | What it is | Unix | Windows |
|---|---|---|---|
| `ProcessTree` | spawn a child and its descendants as one unit, with an optional deadline | today's process group + `setsid` + `kill(-pgid)` | a Job Object with `KILL_ON_JOB_CLOSE` |
| `probe` | the per-platform kernel-facts boundary: peer pid, executable image, working directory, one environment value, liveness, and a process-table walk | moved out of `uze-terminal/process_probe.rs` and `uze-core/machine/process_cwd.rs` | see D8 |
| `interrupt` | the Ctrl+C watch | `sigaction` | `SetConsoleCtrlHandler` |
| `which` | executable lookup | `PATH` | `PATH` × `PATHEXT`, case-insensitive, skipping a directory by file identity |

How the Windows `ProcessTree` spawns:

1. `CommandExt::creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)`.
2. `AssignProcessToJobObject`.
3. Resume the single primary thread, found with
   `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD)` and resumed with
   `OpenThread` + `ResumeThread`, both documented APIs. If the assignment
   fails, the process is terminated and never resumed. `NtResumeProcess` is
   not used: it is undocumented.

Who uses it:

- `uze-core::machine::subprocess`, `uze-git::run_within`/`run_bounded` and
  `uze-terminal` all use `ProcessTree`.
- `kill_reaped_process_group`'s `taskkill` is deleted. A pid that has been
  reaped is never named again.

This changes the layering: `uze-terminal`'s rule becomes "depends on nothing
in the workspace but `uze-document` and `uze-platform`". `crate-layering.mmd`
and AGENTS.md are updated in this change.

**The boundary is enforced, both ways.** Production code outside
`uze-platform` names no platform: no `cfg(unix)`, `cfg(windows)` or
`target_os`, at a call site or at a module boundary. A crate that needs to
decide something per platform is missing a concept here, and the concept is
added, as a value when the decision is data (the shell family a hook wrapper
is written for) and as a function when it is behaviour. Tests may be gated,
because some behaviour exists on one platform only (a mode bit, a symlink
without privilege, a FIFO, a Job Object), but a gate states why in a comment
beside it, and a test of behaviour both platforms have runs on both through
these same concepts. `tests/architecture/layering.rs` fails the build over
either: a platform `cfg` in production code outside this crate, or a test
gate with no reason.

### D4 — The transport port ADR-038 promised

`uze-terminal` gains `runtime/transport/`:

- `Endpoint`
- `Listener::{bind, accept}`
- `Connection::{connect(deadline), split() -> (ReadHalf, WriteHalf),
  peer_pid()}`

The Unix implementation is today's code, moved behind the port. These callers
name only the port types:

- `attach()`, `Handshake`, `forward_events`, `connect_waiting`,
  `serves_this_build`, `open_space`, `listening_peer`;
- the client's `Attach` (`src/ui/orchestrator/session.rs:69`,
  `orchestrator.rs:441`).

The Unix-only endpoint watch, the `/tmp` rebind, `private_directory` and
`MAX_SOCKET_PATH` move behind the Unix transport. This lands and is proven on
Linux and macOS before any Windows code.

### D5 — The Windows transport: an overlapped named pipe private to its user

**Name.** `\\.\pipe\uze-<hex(sha256(canonical UZE_HOME, token user SID))[..24]>`.
The name is global across logon sessions. Two sessions of the same user with
the same `UZE_HOME` therefore share one server; this is intended, and the spec
says so.

**Server side:**

- The first instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`,
  `PIPE_REJECT_REMOTE_CLIENTS` and a DACL granting the **token user SID**
  only. The owner SID is not used: it is `Administrators` when the server is
  elevated.
- If `FIRST_PIPE_INSTANCE` fails, the server reports that the name is held by
  another process and does not start.
- The server always keeps one instance pending, so connects do not race the
  accept loop.

**Client side:**

- Opens with `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`, so a squatter
  cannot impersonate it.
- Loops on `WaitNamedPipeW` within the deadline for `ERROR_PIPE_BUSY` and
  `ERROR_FILE_NOT_FOUND`.
- Verifies the server before attaching:
  1. the pipe's owner, via `GetSecurityInfo`;
  2. `GetNamedPipeServerProcessId`, checked together with the process start
     time, against pid reuse;
  3. the image is a `uze` executable, through `probe`.

  `OpenProcess` access denied counts as a refusal.

**I/O:**

- Handles are `FILE_FLAG_OVERLAPPED`, and each half issues its own operation.
  A synchronous handle shared by a reader thread and a writer thread
  serializes its I/O and deadlocks.
- A deadline is a wait timeout followed by `CancelIoEx`, and always then
  `GetOverlappedResult(..., TRUE)` before the buffer is freed. Data completed
  during cancellation is kept.
- These invariants are written at the top of the module and held by a stress
  test.

**Testing access control.** ADR-038's "another OS user is refused" becomes a
Windows test that runs a second local account. On Unix, the client gains the
symmetric check that the server's uid is its own.

### D6 — The server: a hidden console, its own group, out of the launcher's job

**Spawning.** `uze terminal serve` is spawned with
`CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB`:

- No `DETACHED_PROCESS`: combined with it, `CREATE_NO_WINDOW` is ignored, and
  every console child would open a visible window.
- If breakaway is refused, the spawn is retried without it, and the workspace
  warns once that the server will end with its host.
- The working directory is the user's home.
- `server_executable`/`which_uze` use `EXE_SUFFIX`.

**Stopping an old server.** `retire()` exists for a server that does not speak
this build's protocol, so a protocol `Stop` request cannot work. Each server
waits instead on a named event, `Local\uze-stop-<pipe hash>`, whose shape never
changes:

1. Signalling the event makes the server persist its state and exit.
2. After the grace period, `TerminateProcess` follows, used only on a process
   `probe` confirms is a `uze` image.

A server that runs from an image an upgrade set aside (`uze.exe.old-*`) is
still recognised as `uze`.

### D7 — A pane is a Job Object the pane joins itself

**The trampoline.** A pane's argv is prefixed with a hidden
`uze.exe __pane-host <job-name> -- <argv>`, classified `JustifiedSlow` in
`command_performance.rs`. It:

1. creates the named job `Local\uze-pane-<id>`, with an owner-only DACL and
   `KILL_ON_JOB_CLOSE`;
2. joins it;
3. calls `SetConsoleCtrlHandler(NULL, FALSE)`. Ctrl+C is otherwise disabled
   for everything under the server's `CREATE_NEW_PROCESS_GROUP`;
4. spawns the real program, which inherits the job.

This closes the race without `CREATE_SUSPENDED`, which `portable-pty` cannot
pass. The server opens the job by name.

**Ending a pane.** Closing a pane, `end_leftovers` and server stop all call
`TerminateJobObject`. `ClosePseudoConsole` is never called on the reader
thread: output keeps draining until it returns, because before Windows 11
24H2 it waits forever on an undrained pipe.

**Foreground status.** ConPTY keeps no foreground group, so it is found the
way the console was passed: among the job's processes
(`JobObjectBasicProcessIdList`, parents from `InheritedFromUniqueProcessId`),
from the one the pane started, down through each shell running a command and
the launcher running a harness, to the first that keeps the console. What
that process starts stays its work, as it stays in its group on Unix. The
newest leaf, the first answer, was an agent's own `git` or language server
as often as the agent, and the pane came and went from the sidebar.

**Default interactive shell.** `UZE_SHELL`, else `pwsh.exe` if it is on
`PATH`, else `powershell.exe`. `COMSPEC` is not consulted. This is a person's
interactive shell, which is a preference. Authored commands are different and
always run in 5.1 (D11).

### D8 — `probe` on Windows

| Fact | Win32 source |
|---|---|
| peer pid | `GetNamedPipe{Client,Server}ProcessId` |
| executable | `QueryFullProcessImageNameW` |
| working directory, one environment value | the target's PEB: `NtQueryInformationProcess`, then `RTL_USER_PROCESS_PARAMETERS` via `ReadProcessMemory`, with `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ` |
| liveness | `OpenProcess` + `GetExitCodeProcess`; *unknown* when it cannot answer |
| process table | `CreateToolhelp32Snapshot`, filtered by token user SID |

PEB layouts:

- Native for 64-bit targets, including x64 emulated on ARM64.
- The WOW64 PEB (`ProcessWow64Information`, `IsWow64Process2`) for x86
  targets.
- Every failure is `None`, meaning *unknown*.

Each field read is covered by a Windows test, so a Windows build that moves a
reserved field turns a test red instead of silently losing the signal.

`process_cwd`, which tells the checkout pool whether a checkout is in use,
walks the table through `probe`. That walk has a stated budget and runs off
the draw thread.

### D9 — Locks: the pid stays in the file, the lock covers a byte range

`claim_holder()` reads the holder's pid from inside the workspace lock file.
That pid is how a new build finds and retires an old server that answers
nowhere, so it stays where it is.

- **Unix:** unchanged (`flock`).
- **Windows:** `LockFileEx` locks a sentinel byte range at a high offset
  (`u64::MAX - 1`, length 1). The pid bytes at offset 0 stay readable by other
  processes. Windows locks are mandatory, and `std`'s `File::try_lock` locks
  the whole file, which would make the pid unreadable.

This applies to every `try_lock_exclusive` caller: `MutationLock`,
`AgentsMdGuard` (`project_context.rs:123`), the task store (`task.rs:713`),
`uze-git`'s repository lock, and the terminal workspace lock. The `Ok(())`
fallbacks are deleted. Matches on `WouldBlock` (`CONTENTION_GRACE`) map
`ERROR_LOCK_VIOLATION`.

### D10 — A command line belongs to a platform

`agents.yaml` setup steps, gates and hook handlers are command lines written
by authors in one shell's language. UZE does not translate them or guess.

**Spelling.** A command is a string, or a map with keys `posix` and `windows`:

- A string means `posix`.
- A map may hold either key alone.
- The `posix` spelling runs through `/bin/sh -c` on Linux and macOS.
  Differences between those two are the author's business, the same as today
  (`uname` inside the script).
- On Windows, only `windows` runs (D11).
- A missing spelling is Unsupported on the platform that needs it, in both
  directions.
- Importers map a vendor's `commandWindows` (Codex) to `windows`.

**Where a missing spelling surfaces:**

| Command kind | Missing spelling |
|---|---|
| Hook group with effect `deny` or `ask` | `uze install` / `uze plugin install` **fails**, exit non-zero, naming the group. Nothing of that package is attached. A guard never installs half. |
| Observational hook group | Unsupported, with a warning in the install report; the rest is delivered |
| Setup step | skipped. The checkout is placed and reported *not ready* by the existing readiness signal, naming the step |
| Gate | reported when an agent is placed and when the workspace opens, not only at delivery. Delivery fails closed on it |
| Any project command | listed by `uze status`. `uze doctor` stays machine-scoped (ADR-019): it reports `git.exe`, `ssh.exe`, the PowerShell execution policy and language mode, VT support, the OS build and `Path` |

**Compatibility.** The map form is additive for this build's readers, but an
older uze rejects it (`deny_unknown_fields`). `uze agent plugin check` notes
the minimum uze version a manifest using the map form requires.

**Trust.** Trust shows and compares both spellings, so changing only the
Windows spelling re-prompts. A `hooks.json` that fails to parse fails trust
closed, never `unwrap_or_default`.

**Parsing changes:**

- `CommandHook.command: Command`.
- `WorktreePolicy.setup` and `gate` become `Vec<Command>`. `one_or_many`
  gains `visit_map`, where a bare map is one command.
- Views `DeliveryPolicyView.gate` and `readiness` carry the resolved
  spelling.

**Scaffold.** `uze agent plugin create --hook` writes `scripts/guard` and
`scripts/guard.ps1` and declares both spellings. `plugin check` warns about a
handler with no `windows` spelling. The `uze:author` and `uze:worktree`
skills, and the region UZE projects into `AGENTS.md`, tell agents that on
Windows these lines are PowerShell.

*Alternatives considered:*

- Git for Windows' `sh`: makes Git Bash a hidden runtime of the product.
- `cmd /C`: legacy, and its quoting is unreliable.
- Translating POSIX: guesswork.
- A `default` key: reads as a fallback, which this rule forbids.

### D11 — How authored commands run on Windows

**Shell.** `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass
-Command`, Windows PowerShell 5.1, the one shell every supported Windows
carries. A person who wants pwsh writes `pwsh -File …`.

**Exit status.** The line is wrapped so its status is that of its last
command and a failure stops it:

```
$ErrorActionPreference='Stop'; <line>; exit $LASTEXITCODE
```

Without the wrapping, `-Command` turns a native exit 3 into 1, and `a; b`
succeeds when `a` fails.

**Encoding.** `[Console]::InputEncoding` and `OutputEncoding` are set to UTF-8
without BOM.

**Unusable machines.** A Group Policy execution policy (MachinePolicy or
UserPolicy) overrides `Bypass`, and Constrained Language Mode breaks the
wrapper. On such a machine, authored commands and hooks are **Unsupported**
with the reason, read once into the detection cache. They are never attached
to fail at call time.

**Not used:** `-EncodedCommand`, which EDR products quarantine.

### D12 — The PowerShell hook wrapper, and how each harness calls it

**The wrapper.** `exec.ps1` is a second template beside the `sh` one,
compiled from the same constants: order, timeouts, effect per group, deny
code, reason bound, and each handler's Windows spelling. It is written as
UTF-8 with BOM, because 5.1 reads a BOM-less file as ANSI.

**Running handlers:**

- Each handler runs as a `powershell.exe -File` process of its own, from a
  script file holding the author's line as written, under its deadline;
  past it, the process and everything it started end (`taskkill /T /F`).
  A runspace in the wrapper's own process would save the second
  PowerShell start, but the reason a guard denies with is its stderr, and
  a native program a handler starts writes to the process's stderr handle,
  which a runspace cannot capture: the reason would be lost. Correctness of
  the reason is kept over the latency.
- The handler writes UTF-8 and the wrapper reads it as such; a console's
  code page would garble any reason past ASCII.
- A fault of the wrapper itself goes to `Fail`, which follows the group's
  effect: any other exit reads to Claude Code and Codex as a non-blocking
  error, which would let a guard's tool through.

**The payload:**

- Read whole by `JavaScriptSerializer` with no length limit, which
  `ConvertFrom-Json` lacks above about 2 MB in 5.1; fields are looked up
  with `ContainsKey`, the method its dictionary exposes.

**The entry each harness gets** is a fact of its dialect, per platform. It is
measured on Windows and recorded with the harness version (task 6.2):

| Harness | Windows entry |
|---|---|
| Claude Code | exec form: `command: "powershell.exe"`, `args: [-NoProfile, -NonInteractive, -ExecutionPolicy, Bypass, -File, <path>, <group>]`, no shell. Measured on 2.1.289: the entry fires, and the shell tool is `PowerShell` (its `tool_input.command`), so `shell` matches `Bash|PowerShell`; a deny group blocks the command |
| Codex | the `commandWindows` field, in PowerShell syntax: `& '<path>' <group>` via `powershell.exe -File` |
| Antigravity | a command line sealed against `cmd /c`: `powershell.exe … -EncodedCommand <base64>`, the call to the wrapper encoded, since agy 1.2.16 runs an entry as `cmd /c "<command>"`, ignores `args`, and escapes quotes as `cmd` does not read. 8.3 short paths were dropped: a volume can have them off, as Windows Sandbox's does. A test runs a sealed line through `cmd /c` the way agy does |
| OpenCode | the bridge spawns `powershell.exe` with the Windows spelling. The choice is made at generation time, never `/bin/sh` |

Paths in entries use forward slashes, which every Windows shell accepts.

**Codex shell commands.** Codex's `PreToolUse` does not fire for shell
commands on Windows, so a group matching shell on Codex/Windows is reported
**Unsupported** with the issue link until a measured version fires it.
Measured on 0.160.0: no payload reaches an entry under `command` or
`commandWindows` while Codex runs `powershell.exe -Command`.

**Cost.** A `PreToolUse` costs two PowerShell starts, the wrapper's and the
handler's. Measured on a Windows 11 host: p50 658 ms with one handler, 375 ms
of it the wrapper alone. The 400 ms budget written for the POSIX wrapper is
not reachable by a PowerShell wrapper at all, since its own start takes most
of it, so on Windows the budget is what two starts cost: p50 ≤ 700 ms for a
group of one handler. Each handler is a start of its own, about 280 ms more
(658 − 375), so a group of `n` is held to 375 + 325·`n` ms; the budget is per
handler because the cost is. Running
the handler in a runspace of the wrapper's own process was measured and
refused: a handler's `exit 3` does not reach the caller of a runspace, and
that exit code is the contract, while creating the runspace still costs about
200 ms. Lowering the cost further needs a wrapper that is not a script, which
is a decision of its own.

**Shared fixtures.** The `sh` goldens become a shared fixture set, gaining a
Windows spelling per fixture, a large-payload fixture and a non-ASCII fixture.
`exec.ps1` must answer every fixture identically on the Windows row: it does,
held by decision (`hooks/wrapper_parity_tests.rs`), the shell's own words
aside.

### D13 — Shims: copies of `uze.exe`, swapped like the binary

**Placement and refresh:**

- `runtime_shim` places `<harness>.exe` as a byte copy of the running
  `uze.exe`.
- A shim whose content differs is replaced by renaming it aside to
  `<harness>.exe.old-<pid>` and placing the copy. A running shim is never
  overwritten.
- The `.old-*` sweep covers the shims directory. Both patterns are named in
  `UzeHome` beside `install_receipt_path`.
- Refreshing compares sizes and modification times first, hashing only on a
  mismatch, and runs off the draw thread.
- `EXE_SUFFIX` is added at `tasks.rs:140` and `doctor.rs:756`.

**Running the harness.** `shim::detect` matches on `file_stem`,
case-insensitively. On Windows, `run_replacing_process`:

1. puts the shim in a `KILL_ON_JOB_CLOSE` `ProcessTree`;
2. spawns the harness;
3. swallows Ctrl+C for itself;
4. exits with the harness's code.

`UZE_SHIM_PID` plus D7's foreground walk identify the harness.

**PATH order.** Shims are first on `PATH` inside the workspace's panes. Outside
the workspace, the installer does not put the shims directory ahead of the
system `Path`, and the spec says shims apply inside the workspace on Windows.

### D14 — Upgrade: rename aside, then in

**Asset and tools:**

- The asset is `uze-<arch>-windows.zip`.
- `%SystemRoot%\System32\tar.exe` and `curl.exe` are called **by absolute
  path**. Under 5.1, `curl` is an alias, and with Git's `usr\bin` on `PATH`,
  `tar` is GNU tar, which cannot read a zip.
- `NUL` replaces `/dev/null`.

**`replace()`:**

1. Verify the download.
2. Rename `uze.exe` to `uze.exe.old-<pid>`.
3. Rename the new file into place, going through the sharing-violation retry
   helper, since Defender scans new executables.
4. If step 3 fails, undo step 2.
5. Refresh the shims (D13).

The startup sweep removes images that are no longer running. On Unix, the
requirement still reads "a single rename".

`hand_off_check` uses D6's flags.

### D15 — Home, paths, bytes

**Home.**

- `UzeHome::from_env` uses `std::env::home_dir()`, which reads `USERPROFILE`
  on Windows (fixed in 1.85).
- `uze-terminal` calls it locally, as its sanctioned exception.
- Every integration takes home from `UzeHome`.
- `run_captured` sets both `HOME` and `USERPROFILE`.

**Paths.** One helper, re-exported through `uze-application` for `src/`, does
three things:

- `strip_verbatim`;
- `same_path`: case-insensitive on Windows using ordinal upper-casing, and
  separator-insensitive;
- `display`.

`project_id_for` normalizes once, internally, so the id never depends on which
caller stripped a prefix. Locators accept `C:\`, `.\` and `~\`, and a drive
letter is never read as an scp `host:`.

**Git acquisition:**

- keeps `SystemRoot`, `windir`, `TEMP`, `TMP`, `USERPROFILE`, `ComSpec` and
  `PATHEXT` through `env_clear()`;
- `pushed_config` adds `core.autocrlf=false`, `core.eol=lf`,
  `core.fileMode=false`, `core.symlinks=false` and `core.longpaths=true`;
- `NUL` for the null paths;
- symlinks are read from tree entries (mode `120000`) and recorded as links,
  by `digest.rs`'s rule (D16).

`core.longpaths=true` is also set on `git worktree add`.

**The repository** gains a root `.gitattributes` with `* text=auto eol=lf`.
`include_str!` templates, goldens and the embedded marketplace then digest
identically on Windows checkouts. A cross-platform digest test uses a
`text=auto` fixture and a symlink fixture.

**The binary manifest.** `uze.exe` embeds an application manifest through
`cargo:rustc-link-arg-bins=/MANIFEST:EMBED /MANIFESTINPUT:…` from the root
`build.rs`. No crate is needed. It declares:

- `longPathAware`, because `Command::current_dir` and child tools do not get
  `std`'s verbatim conversion;
- `activeCodePage` UTF-8.

The client also sets the console code pages to 65001.

**Private directories.** Where Unix creates them 0700, Windows sets an
owner-only DACL.

### D16 — Names Windows can hold

**The constraint.** NTFS cannot hold `:`, and `plugin:capability` silently
creates an alternate data stream. Agent Skills requires
`^[a-z0-9]+(-[a-z0-9]+)*$` and requires the name to equal the directory, and
OpenCode enforces both.

**The decision is per harness, from its dialect:**

- Where a harness takes the label from frontmatter (Claude, Codex), the
  directory name is free.
- Where the label is the directory (OpenCode), the label must already be a
  legal name.
- On Windows, the on-disk name of a loose skill or agent is
  `<plugin>-<capability>`. Before writing, the install detects a collision
  between two packages that map to the same name and fails, naming both.
- Reserved device names (`CON`, `NUL`, `COM1`, …), `< > " / \ | ? *`,
  control characters and a trailing dot or space are refused.
- Native plugin delivery (Claude, Codex) is preferred wherever it exists, so
  that the `plugin:skill` label comes from the harness.
- Linux and macOS on-disk names are unchanged by this change.

**What is tested.** The mapping is testable in CI: name in, on-disk name out,
collision refused. The label each harness shows is recorded evidence for it,
measured by hand on Windows per harness version.

### D17 — MCP servers on Windows

A plugin's MCP `command` such as `npx` resolves to `npx.cmd` on Windows. When
projecting MCP configuration on Windows, a command that resolves to a
`.cmd`/`.bat` launcher is written as `cmd /c <command> <args>`, the form the
vendors document. The integration owns the spelling, and it is measured per
harness alongside the hook entry.

### D18 — The suite runs on Windows, in PowerShell

**Fakes.** `uze-testkit`'s `FakeHarness`, `scripted_agent` and `FakeSsh`, and
the inline shebang fakes in 32 test files, dispatch through the existing
`uze-fake-harness` binary, configured by an environment file. On Windows the
fake is a `<name>.exe` copy; on Unix, a symlink.

**Gating.** A genuinely Unix test is `#[cfg(unix)]` with its reason. Each one
has a Windows counterpart where the behaviour exists there: pipe DACL, job
teardown, the `LockFileEx` range.

**References corrected by the review:**

- `runtime/persist.rs:124-131`
- `tabs.rs:437`
- `root_picker.rs:360`
- `orchestrator.rs:361`
- `generate.rs` and `application/tests.rs` are test-gated, not
  product-gated.

### D19 — CI on Windows answers the user's question

**`ci.yml`.** The Windows test rows run under `shell: pwsh`, with:

- `HOME` unset;
- `PATH` reduced to System32, `Git\cmd`, cargo and rustup;
- a first step asserting that `sh`, `bash`, `jq` and GNU `tar` do not resolve.

Rows and triggers:

| Row | Runs on |
|---|---|
| x64 (`windows-2025`) | every PR touching `src/`, `crates/`, `tests/`, `journeys/` or `install.ps1` |
| Arm64 (`windows-11-arm`) | the platform filter, `main` and nightly |

The rows start non-gating and become gating at task 8.4.

Other CI changes:

- Rust caches save only from the nightly, with `CARGO_INCREMENTAL=0`, because
  the repository cache is at 9.9 of 10 GB.
- Defender excludes the target directory.
- Python comes from `actions/setup-python`.
- The proof key treats an empty `ImageOS`/`ImageVersion` as unknown, never
  reusable.

**`installer-windows` job:**

- PSScriptAnalyzer and Pester 5, pinned, against a fake release served over
  `python -m http.server`;
- both 5.1 and pwsh 7;
- the refusal gate is a mockable function, not an environment bypass.

**`release.yml`:**

- New targets: `x86_64-pc-windows-msvc` on `windows-2025`, and
  `aarch64-pc-windows-msvc` on `windows-11-arm`, native, so it can be
  smoke-run. Both are Tier 1.
- `+crt-static` goes in `.cargo/config.toml` under
  `[target.'cfg(all(windows, target_env = "msvc"))']`, so tests run what ships.
- The zip is built with `tar.exe -a -cf`, followed by a `uze.exe --version`
  smoke step.
- The SBOM uses `--target all`.
- The `publish` globs, `SHASUMS256.txt` and attestation subjects take `*.zip`.
- The Windows rows are gated by a repository variable until task 13.3.

**Elsewhere:**

- `cliff.release.toml` gains PowerShell install and verify lines, plus the
  SmartScreen and Smart App Control notes.
- `web/vercel.json` serves `/i` to both shells, one address and each shell's
  own command (`curl -fsSL https://uze.sh/i | sh`, `irm https://uze.sh/i |
  iex`): a request whose `User-Agent` names PowerShell (`WindowsPowerShell/5.1`,
  `PowerShell/7`) gets `install.ps1`, any other `install.sh`, as before. The
  answer varies by `User-Agent` and is not cached. `/i.ps1` stays, for an
  address already written down.

### D20 — The journey runner gains a Windows backend

`journey.py` splits two backends:

| Backend | Unix | Windows |
|---|---|---|
| `Screen` | tmux | `pywinpty` (ConPTY) feeding a `pyte` screen. DSR is answered through `write_process_input`; clicks map through the same cell grid |
| `Machine` | `/proc` / `ps` | `psutil` |

- `fcntl` becomes `msvcrt.locking`.
- The packages are pinned in `journeys/requirements-windows.txt`, installed
  with `--only-binary :all:`. `win_arm64` wheels are checked.

**What changes in the specs.** The earlier claim that they would not change was
wrong:

- **World setup.** `shell:` steps are runner tooling, and on Windows they run
  under the runner's own Git Bash by absolute path. The environment of the
  `uze` under test is kept free of it. World stand-ins become
  `uze-fake-harness` copies.
- **Provisioning checks.** `curl.log` checks gain their PowerShell-route
  counterpart.
- **`07-upgrade`.** Declared unsupported on Windows, with the reason, until a
  Windows release exists to upgrade from.
- **`tap` and OSC 52.** These have a ConPTY answer, or the check stops the run.

**Effort.** The port is estimated at 3–5 engineer-weeks, and the task group is
sized accordingly. If pyte proves flaky on click coordinates (ConPTY
re-renders wide glyphs), the fallback is wezterm's mux CLI as the screen.

### D21 — The TUI and CLI on a Windows console

- **Keys.** Key handling drops `Release` and `Repeat` before notice dismissal
  (`session/keys.rs:92-101`, `session/manage.rs:97-106`). Ctrl+Alt plus a
  printable character is text only when the keymap binds nothing to it.
- **Colour** comes from `is_terminal()` plus enabling VT. The opener is
  `cmd /c start "" <url>`. `$BROWSER` is split with `split_paths`.
- **Output.** `--quiet` redirects the stdout handle to `NUL`. A stdout
  `BrokenPipe` ends quietly.
- **Retry.** Atomic replaces and extension saves go through the one
  sharing-violation retry helper.
- **Local time.** Prompt history's local day uses `GetTimeZoneInformation`.
- **SSH.** `uze doctor` checks `ssh.exe` when any marketplace is reached over
  SSH, because MinGit has none.

### D22 — Installing, uninstalling, and the floor

**`install.ps1`** mirrors `install.sh` and adds:

- The whole script body is a scriptblock, so a truncated download runs
  nothing.
- Errors are `throw`n. Under `iex`, `exit` would close the user's window.
- **Download:** `$ProgressPreference='SilentlyContinue'`, `-UseBasicParsing`,
  and TLS 1.2 OR-ed into the allowed protocols rather than replacing them.
- **Architecture:** read from the registry's native `PROCESSOR_ARCHITECTURE`,
  which is reliable under x64 emulation.
- **Floor:** the OS build must be ≥ 19045.
- **Prerequisite:** `git.exe` must be present.
- **Checksum:** `Get-FileHash`, compared case-insensitively.
- **Location:** `%LOCALAPPDATA%\Programs\uze\bin`, or `UZE_BIN_DIR`.
- **`Path`:**
  - read with `DoNotExpandEnvironmentNames`;
  - written back as `REG_EXPAND_SZ`;
  - `WM_SETTINGCHANGE` broadcast;
  - `$env:Path` updated for the current session;
  - entries compared case- and trailing-`\`-insensitively.
- **Receipt:** written as UTF-8 **without BOM**, with a canonical path that
  `is_same_file` matches.
- **Output:** ASCII glyphs on a non-UTF-8 console.
- **Reputation warnings:** a warning when Smart App Control is on (registry
  `VerifiedAndReputablePolicyState`), because an unsigned `uze.exe` may be
  blocked.

**`install.ps1 -Uninstall`** removes:

- the binary;
- the `Path` entry;
- the shims;

and stops the server. `~\.uze` is removed only with `-Purge`.

**Scoop.** A Scoop manifest is generated from the release's zip and SHASUMS.
Winget is deferred.

### Diagrams

`crate-layering.mmd` gains `uze-platform` (process trees and kernel facts),
with edges from `machine`, `git` and `terminal`. The terminal crate's arrow to
`document` stays. `containers.mmd` is unchanged: the terminal server still
hosts over a PTY on all three platforms. AGENTS.md's workspace-layout entry
for `uze-terminal` and the new crate is updated. The layering tests learn the
new leaf.

## Candidate ADRs

- **`uze-platform`, a leaf crate for processes and kernel facts (D3):** a
  boundary moved out of `uze-terminal` and `uze-core`.
- **Win32 through `windows-sys` and no wrapper crate (D2).**
- **A named-pipe transport private to its user (D4/D5):** ADR-038's backend
  made concrete, with the stop event whose shape never changes (D6).
- **A command line belongs to a platform (D10/D11):** `posix`/`windows`
  spellings, Windows PowerShell 5.1, and deny groups without a spelling
  failing the install.
- **Shims as binary copies swapped by rename (D13):** an addendum to ADR-014.
- **Windows is a supported platform, unsigned, from 22H2:** an addendum to
  ADR-034.
- **The PowerShell hook wrapper and per-harness Windows entries (D12):** an
  addendum to ADR-040.

## Risks / Trade-offs

- **Unsigned binaries meet Smart App Control and EDR.**
  → Detected and warned by the installer and `uze doctor`, and stated on the
  installation page. This is a documented, conscious choice.
- **PEB reads break on a future build.**
  → They return `None`, and per-field Windows tests make it red.
- **Job breakaway is refused by some hosts.**
  → Retry without breakaway, and the workspace names the host once.
- **Overlapped pipe I/O deadlocks or loses data.**
  → One module, written invariants, a stress test, and the full terminal
  suite on Windows.
- **Hook latency on Windows.**
  → One PowerShell start per call, runspaces for handlers, a measured
  budget.
- **Existing POSIX-only plugins on Windows.**
  → Deny/ask guards refuse to install, which is loud. Observational ones are
  reported, and the scaffold and `plugin check` steer authors.
- **Vendor hook behaviour drifts (shell choice, the Codex bug).**
  → Per-harness Windows dialect facts carry the measured version. A drift is
  a re-measure, not a guess.
- **The journey port is 3–5 weeks.**
  → It is sized as its own task group, and was green on both hosted Windows
  runners before Windows shipped.
- **The size of the change.**
  → Kept as one change by decision. Its groups merge in order, and Linux and
  macOS stay green at every merge.

## Migration Plan

Nothing changes shape on Linux or macOS:

- the lock file is unchanged;
- on-disk names are unchanged;
- a plain string command reads as before.

The attachments ledger gains a receipt kind for a copied artifact (the
Windows answer to `SymlinkReference`). That is a `Shaped` record change with
one rung.

**Rollback:** drop the Windows rows from `release.yml` and make `install.ps1`
refuse Windows again. Existing Windows installs keep working, and
`uze upgrade` stops finding newer Windows assets.

**Shipped as experimental, not gated.** Windows was to stay behind a
repository variable and an installer refusal until a release candidate was
installed by hand on x64 and Arm. It ships instead by default, marked
experimental in the README, the installation page and the release notes:
every gate that guarded it passes on both hosted runners, the release path
is built and installed by `install.ps1` on every change to it, and the one
known limit, an unsigned `uze.exe` that Smart App Control can refuse, is a
fact of the machine that a gate would not have changed. Signing is the
follow-up that lifts it.

## Open Questions

- **Antigravity's binary location.** `%LOCALAPPDATA%\Antigravity\agy.exe` per
  a third-party guide. Confirm it on the first Windows run of task 4.4.
