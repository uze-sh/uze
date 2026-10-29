## Context

The platform's design is in `extension-platform/design.md`. This change
carries out change 5 of its *Proposed split* and cites decisions there by
number rather than restating them:

- D4: the program contract.
- D5: consent.
- D6: confinement, the pinned `exec:` and the checkout.
- D18: the store.
- D27: scripts first, the texts of the consent steps.
- D28: the budget.

The review findings it closes are S1, S3, S6–S10, S12, S13, S17, S18 and
R-S13 (*Triage*).

What exists after change 4:

- **`uze-contract`**: `SourceKind::Run(Vec<String>)`, decoded and refused by
  the check.
- **`uze-workspace`**: the consent record already carries `exec_paths` and
  `enforced_at_accept` as empty fields.
- **`uze-application`**: `ExtensionRunner` answers fixtures and scripts on a
  2-lane pool, with a session cache. `ExtensionCheckout` checks the checkout
  on every question.
- **`uze-core`**: `machine::subprocess` has `read_bounded`,
  `wait_with_timeout` and process-group trees.
- **`uze-platform`**: the `process` module spawns into a group.
- **The machine this is built on**: WSL2, kernel 6.18 with Landlock ABI 7
  and seccomp filters, so the Linux confinement is tested on a real kernel,
  not mocked.

## Goals / Non-Goals

**Goals:**

- `gpu-meter`: an extension that runs the real `nvidia-smi` through
  `exec:nvidia-smi`, confined, and draws a meter and a sparkline. The
  sparkline comes from history the extension keeps in its store.
- Every test of task 5.6 of the parent plan passes on this kernel.

**Non-Goals:**

- `net`, `fetch` and `secret` (change 8). With no `net` grant in this
  change, every program runs with no network.
- Windows confinement (`extension-confinement-windows`). Windows is
  disclosed as "runs as you".
- The `view`, `cells`, `image` and `sprite` output modes (changes 6 and 7).
- The agent bridge (change 6).

## Decisions

### P1. Order of the work

The work runs in this order:

1. The manifest and the check.
2. The confinement concept in `uze-platform` and the `uze __confine`
   helper, built in parallel with step 3 because they share no file.
3. The program runner in `uze-core`.
4. Pinning and the consent steps.
5. The runner answering programs: sources, actions and `lines`.
6. The store.
7. `run_program`.
8. The operator's surfaces.
9. The proof and the docs.

No program runs until step 4 exists. Until then the runner keeps refusing
`run`.

### P2. Who knows what

- **`uze-platform::confine`** knows how a platform confines and nothing
  about extensions. It holds:
  - `Confinement`: read paths, write paths, execute paths, `network: bool`
    and the rlimits;
  - `availability()`: what this machine can enforce, right by right;
  - `enter(&Confinement, argv)`: applies the confinement and replaces the
    process. It returns only an error.
- **`uze-core::capability::extension::program`** runs a program, given:
  - a resolved argv;
  - the working directory;
  - an environment allowlist;
  - stdin bytes;
  - bounds;
  - an optional `Confinement`;
  - the helper's path.

  It spawns, bounds, kills and maps exit codes, and knows neither
  `UzeHome` nor consent.
- **`uze-application`** builds the `Confinement` from what only it knows:
  - the checkout and its Git common directory;
  - the extension's Store directory;
  - the scratch directory under `UzeHome`;
  - the pinned paths.

  It also decides whether a run may happen at all, from the consent and
  from `enforced_at_accept`.
- **`src/cli`**: the hidden `__confine` command parses its argument and
  calls `enter`. It is the only caller of `enter`, and the runner is the
  only code that spawns it.

### P3. The helper gets its rules as one JSON argument

The command line is
`uze __confine '<confinement as JSON>' -- <program> <args…>`. An argument
is visible in `/proc/<pid>/cmdline` to the operator's own processes, and it
holds paths only, never a secret. Passing it on a pipe would cost a file
descriptor protocol for nothing.

### P4. seccomp is a deny filter with argument checks, written over `libc`

The filter allows everything except the following:

- `socket` for every family. With no `net` in this change, a program has no
  sockets.
- `socketpair` other than `AF_UNIX`.
- The network calls, `io_uring_*`, `ptrace`, `process_vm_*`, `setsid` and
  `setpgid`. Refusing the last two is an addition to D6: a descendant that
  cannot leave the process group is always reached by the group kill, so a
  fork bomb is contained without a subreaper the helper would lose at
  `execve`.
- Below Landlock ABI 6, `kill`, `tkill`, `tgkill` and `pidfd_send_signal` to
  anything but the program's own group. The helper's pid is the program's
  pid and its group, because it leads the group the runner spawned it into.

The prologue checks the audit architecture and kills the process on a
mismatch. The filter is built per run, in about 40 instructions.

### P5. Landlock through the `landlock` crate, at ABI 5 with the hard requirements consent promised

- **Read and execute**:
  - the system prefixes;
  - the directory of each pinned path;
  - the extension's Store directory.
- **Read only**:
  - the checkout;
  - its Git common directory;
  - `/proc/self`;
  - the named devices;
  - each `reads` path an `exec:` declared.
- **Write**: the scratch directory and `/dev/null`.

With ABI ≥ 4, TCP bind and connect are handled and none is allowed. With
ABI ≥ 6, scoping covers abstract sockets and signals.

Whether a right was enforced is reported per right: `truncate`, TCP,
`ioctl` and scoping. If the ruleset is not `FullyEnforced` for a right that
consent promised, the helper exits with its own code, 125.

### P6. Scratch directories live in the runtime tier

A run's scratch directory is `$UZE_HOME/runtime/extensions/scratch/<run>`.
It is created before the run and removed after it, whatever the outcome.
Losing it costs nothing, and naming it in `UzeHome` keeps it out of the
operator's tmp, where `exec:` resolution refuses to look.

### P7. Windows is unavailable, not a Job Object

D6 planned a Job Object for Windows in this change. It cannot be tested on
this machine, and a Job Object without a restricted token does not confine
reach, which is what the consent screen talks about. Windows therefore
reports "unavailable" and the "runs as you" disclosure applies. The Job
Object moves to `extension-confinement-windows`.

### P8. A grant names a program: `Grant` stops being `Copy`

`Grant::Exec(ProgramName)` joins `ReadCheckout`, `Clipboard`, `OpenPath`,
`Store` and `StoreGlobal`. A device a program needs is declared beside its
grant, as `exec "nvidia-smi" { reads "/dev/nvidiactl" "/dev/dxg" }`, and the
consent screen lists it.

### P9. Only the time limit can be lowered

A `run` node takes `timeout="5s"`: 10 s by default, 10 s at most. The
output ceilings (stdout 1 MiB, stderr 64 KiB) are the host's and fixed.
Lowering them buys an author nothing that `timeout` does not.

### P10. Time a script spends in `run_program` does not count against its 100 ms

The script's deadline is paused for the length of each `run_program` call,
which has its own time limit. Otherwise a 300 ms `nvidia-smi` could never
be read from a script.

### P11. The input a program receives

A program's stdin carries:

```json
{ "protocol": 1,
  "context": {"checkout", "extension", "source" | "action"},
  "input": {…},
  "store": {…} }
```

`store` is present only when the declaration names it. `branch`, `work`,
`surface`, `slot`, `size` and `agent` from D4's list need a `Question` that
carries them, so they arrive with change 6, which needs them for `agents`.

### P12. Resolution refuses what an extension could plant

`exec:<name>` resolves on `harness_search_path()`, then through
`realpath`. It is refused if it lands:

- under the project it is enabled in;
- under `$UZE_HOME`;
- under the system temporary directory;
- under `/mnt/*`.

The general runners are a fixed list in `uze-core` (D6). A harness's own
name is refused by `uze-application`, which asks the integration registry.

## Risks / Trade-offs

- [A kernel without Landlock] → `availability()` says so, and the "runs as
  you" step applies. The tests that need Landlock skip, naming what they
  skipped.
- [`nvidia-smi` needs sockets or devices this filter refuses] → the proof
  finds out on a real GPU. Any device it needs is declared in `reads` on the
  `exec:` grant, never by widening the filter for everyone.
- [macOS cannot be tested here] → the profile is generated text with unit
  tests over it, and the macOS CI job runs the confinement tests where
  `sandbox-exec` exists.

## Migration Plan

Records written by change 4 have no `exec_paths`, and their extensions want
no `exec:`, so nothing changes for them. An update that adds `exec:` widens
the grants and is held, as change 4's rule already says.

## Open Questions

None. What this change would otherwise ask, it decides and records in
`decisions.md`.
