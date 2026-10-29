## Why

Running several agents at once is what the workspace is for, and nothing
says what UZE as a whole costs the machine: how much CPU and memory
everything it runs takes, how much disk its checkouts and `~/.uze` hold,
how long the workspace has been up. The operator finds out from `top` and
`du`, where the workspace is a scatter of pids and directories.

## What Changes

- New command `uze usage [--format json]`: one report for the
  whole of UZE on this machine, never narrowed to a project, space or
  agent.
  - A head with the machine (system, cores, memory) and the workspace's
    uptime.
  - A process table by component: agents (their panes and the shells
    born of them), shells, the terminal server, clients, and a total;
    CPU as the machine's share over one second, memory as each process's
    own share, process count.
  - A disk table: every known project's checkouts, each tier of
    `~/.uze`, and a total.
  - No workspace running, or one from another build, is reported, not
    failed.
- The terminal runtime answers a new read-only request with those four
  totals and its start time: no labels, identities or contents. It is the
  one request besides opening a space that a connection from inside a
  pane may make, because the workspace's own shells are where a person
  types this command.
- `uze-platform` gains what a process uses and what the machine has.

Out of scope: the workspace TUI, tokens and cost, a live view, history,
and any breakdown per project, space or agent.

## Capabilities

### New Capabilities
- `resource-usage`: the `uze usage` report: what each row
  counts, how CPU, memory and disk are measured, its states and its JSON.

### Modified Capabilities
- `terminal-runtime`: the runtime reports what it runs on request, and
  admits that request from inside a pane.

## Impact

- `crates/uze-platform`: a `usage` module (per-process CPU with reaped
  children, memory, the machine's capacity); a census of a pane's
  processes; Job Object CPU accounting on Windows; one more windows-sys
  feature (`Win32_System_ProcessStatus`).
- `crates/uze-terminal`: `ClientRequest::Usage` / `ClientEvent::Usage`,
  `PROTOCOL_VERSION` 22 → 23, the server's start time, clients' pids,
  sampling on the connection's thread.
- `crates/uze-application`: a disk read model (checkout walk shared with
  the checkout list, UZE home tiers).
- `src/cli`: the subcommand, its report and JSON, its performance class.
- ADR-056 is narrowed; `docs/architecture/invariants.md`, its test and the
  refusal message follow.
- `docs/architecture/containers.mmd`: the CLI's relation to the terminal
  server.
