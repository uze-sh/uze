## Context

See proposal.md for why. What the change builds on:

- One terminal server per user owns every pane. Each `PaneRuntime` (in
  the server's `panes` map, a lock apart from the session) holds the
  pane program's pid and an optional `pane::Group`. On Unix
  `Group::members()` is the leader's process group alone, empty on macOS;
  `belongs` judges by the leader's session and ancestry; strays are found
  by the pane's `Mark` (`UZE_PANE_UNIT`). On Windows the Job Object is the
  whole answer.
- `Tab::env` echoes a launch: `UZE_AGENT` marks an agent tab, and
  `Tab::agent` names the agent tab a shell was born of.
- ADR-056: a connection from inside a pane may open a space and nothing
  else; `refusal` lets any other first frame through to its own handling.
- An older server cannot decode a variant it does not know and hangs up
  without answering.
- `uze-application` walks checkouts for the checkout list (`measure`,
  `allocated`, `checkout::account`).

## Goals / Non-Goals

**Goals:** one honest figure for everything UZE runs, short-lived build
processes included; no cost while nobody asks.

**Non-Goals:** per-project, per-space or per-agent breakdowns; limits or
alerts; processes UZE did not start; GPU, network or I/O.

## Decisions

### D1. Measured in the server, from one census per reading

The server knows each pane's leader, group and mark. One census per
reading lists this user's processes once (`pane::census`): on Unix each
pid's parent, session and mark value; on Windows nothing beyond what a
job lists. A pane's processes are then found in memory: on Unix the
leader, its session, its descendants and its mark's carriers; on Windows
the job's members. One environment read per process per reading, not
per pane. The client could scan for `UZE_AGENT` with no protocol change,
but shells born of an agent carry none and Windows would mean reading
every process's PEB.

### D2. Two readings one second apart, with reaped children

On the requesting connection's thread. Under the locks only the tabs and
the pane runtimes are copied.

- Unix: per process, own CPU plus reaped children's (`cutime + cstime`
  on Linux, `ri_child_*` on macOS), so a compiler that lived half a
  second is carried by the build tool that waited for it. A pane's
  window CPU is the delta between readings of processes present in both,
  matched by pid and start stamp.
- A process only in the second reading counts in full when its start
  stamp is newer than every stamp of the first reading, which is how a
  process started inside the window looks; otherwise it was missed, and
  is left out rather than counted as a lifetime in one second.
- Windows: the job's `JobObjectBasicAccountingInformation` totals count
  every process the job ever held, so a pane's CPU is their delta.
- A request arriving while another samples waits for it and takes its
  answer: a loop in a pane cannot keep the server scanning.

The share is of the whole machine (cores × window), 0 to 100%.

### D3. A pane may read usage; the answer names nothing

The workspace's own shells are where a person types commands. `Usage` is
a keyless first frame, like `OpenSpace`; every other request from a pane
stays refused. The answer is four totals, a window and a start time: no
label, identity, launch variable or contents. What a pane learns is how
busy the workspace is, and how long it has been up.

### D4. Uptime

The server records its start (`SystemTime`) when it takes the workspace
claim and answers it with usage. The CLI renders "up 3h 12m · since
09:14" in local time, and the JSON carries the start in seconds since the
epoch.

### D5. Disk

`Workspace::disk()` in `uze-application`: every project UZE keeps
records for (`record::roots`), the checkouts `checkout::account` names as
an agent's, a subagent's or unreadable (UZE's own), walked with the
shared walk, all in parallel; a missing root is skipped. The primary, a
person's worktree and a harness's own are not UZE's cost and are not
walked; a checkout nested in one of UZE's is left out of it. Plus every tier `UzeHome` names, measured through its
accessors. It needs no `agents.yaml`, task store or Git read beyond the
worktree list. The command is `JustifiedSlow("walks every checkout to
measure the disk it holds")`, and the walk runs while the server samples.

### D6. Platform

`uze_platform::usage`: `Machine { system, cores, memory }`, and
`of(pid) -> Option<Reading { started, cpu: CpuTime, memory: Memory }>`
with `Memory { bytes, estimated }`.

- Linux: `/proc/<pid>/stat` utime, stime, cutime, cstime, starttime over
  `_SC_CLK_TCK`; `Pss` from `smaps_rollup`, else `statm` resident
  (estimated); `MemTotal` from `/proc/meminfo`; WSL by `WSL_DISTRO_NAME`.
- macOS: `proc_pid_rusage(RUSAGE_INFO_V2)`, Mach units through
  `mach_timebase_info`; `ri_phys_footprint`; `hw.memsize`.
- Windows: `GetProcessTimes`; `PrivateWorkingSetSize` from
  `PROCESS_MEMORY_COUNTERS_EX2`; `GlobalMemoryStatusEx`; the job's CPU via
  `pane::Group::cpu_time`.
- Other Unix: no readings; the report says usage is not available.

### D7. Layering

`src/cli` asks `uze_terminal::usage()` beside `stop()` and the
application for the disk, on two threads, and renders both. Neither
`uze-application` nor `uze-terminal` learns of the other.

### D8. Another build

A hang-up with no answer to `Usage` is read as a server of another
build: the same thing every older server does with a frame it cannot
decode. A server of this build that does not know the version in the
frame answers "incompatible", read the same way.

### D9. The report

Head as `key_value` lines (machine, cpu, memory, workspace), then two
tables with `aligned_rows`, numbers right-aligned, sizes in binary units:

```
uze usage

  machine     linux (WSL) · x86_64
  cpu         4 cores
  memory      16 GiB
  workspace   up 3h 12m · since 09:14

                       CPU     MEMORY   PROCESSES
  agents (3)         32.0%    4.0 GiB          46
  shells (2)          0.1%     12 MiB           4
  terminal server     0.1%     31 MiB           1
  clients (2)         0.3%     17 MiB           2
  total              32.5%    4.1 GiB          53

                      DISK
  checkouts (7)   21.1 GB
  packages       412 MB
  ...
  total           21.6 GB
```

## Candidate ADRs

- **A pane may read the workspace's usage**: amends ADR-056's "open a
  space and nothing else" with one read that names nothing.

## Risks / Trade-offs

- [Walking checkouts takes seconds, more on WSL's 9p] → parallel walks,
  and the command says what it measures meanwhile.
- [Reaped-children time counts only children their parent waited for] →
  an orphan reaped by init is lost; build tools wait for their children.
- [A process that scrubbed its environment and left the session is lost]
  → accepted, as it is to ending the pane today.
- [Protocol bump] → another build is reported, with `uze workspace stop`.
