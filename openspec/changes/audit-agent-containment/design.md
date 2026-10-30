## Context

See proposal.md - Why. ADR-056 classified a terminal peer by its session and
ancestry and made a key file the operator's credential; ADR-055 relied on the
environment stamp to say who asks. Both named the escape as a residual; the
audit showed each costs one line. Landlock ABI 6 (Linux 6.12+) adds
`LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET`: a process in a scoped domain, and
every descendant whatever it does with `setsid`, cannot connect to an
abstract Unix socket created outside that domain. The `landlock` crate
0.4.7 already in `uze-platform` speaks it. Landlock needs `no_new_privs`
(the process is not privileged), which the kernel then reports in
`/proc/<pid>/status` and which nothing can unset.

## Goals / Non-Goals

**Goals:** kernel-backed separation of an agent pane from the person's verbs
on Linux; bounded, unpersisted pane-opened spaces; kernel-backed refusal of
the operator's answers to a confined process; no key-less claim admitted.

**Non-Goals:** confining what an agent may read or write (that is the
harness's sandbox); defending against a same-uid process outside every
sandbox (it owns the account); macOS and Windows kernel separation (no
equivalent primitive was found cheap enough; they keep ADR-056's model).

## Decisions

### TERM-1: two doors, and agent panes sealed from the person's

- **Sealed pane.** `uze_platform::seal::sealed` runs a closure on a
  thread of its own that first sets `no_new_privs` and restricts itself with
  a Landlock ruleset handling no file or network access and scoping only
  abstract Unix sockets. Landlock and `no_new_privs` are per thread, so the
  server stays unconfined and only what that thread forks inherits the
  domain. The terminal spawns a pane's program from that thread. No extra
  `exec`, no helper binary, no change to the pane's leader pid.
- **Which panes.** Every pane whose launch is a program (an agent the
  workspace placed, a program relaunched on restore). A shell pane the
  person opens is not sealed: `no_new_privs` disables set-uid, and `sudo` in
  the person's own shell is not ours to take. A shell an `OpenSpace` opened
  is given no input by anyone but the person, and stays a shell (it is
  bounded and unpersisted, see TERM-2). Rejected: sealing every pane (breaks
  `sudo`, `su`, `pkexec`, set-uid `fusermount` in the person's shells);
  sealing through a `host-pane` exec (an extra process image per pane, and
  the hook exists only where a host is configured).
- **Signals are not scoped.** `Scope::Signal` would also stop an agent from
  killing the server, but it stops it from ending any process another pane
  or the person started, which harnesses do (`pkill` a dev server). The
  takeover it would block is closed by the client check below instead.
- **Operator endpoint.** When sealing works here (probed once at server
  start by sealing a throwaway thread), the server binds an abstract socket
  named `uze-operator-<identity>-<random>` before its pathname socket, and
  writes that name beside the key (`workspace.operator`, 0600). Attach and
  Stop are served there; on the pathname socket they are refused while the
  operator endpoint stands. The pathname socket keeps `OpenSpace` and
  `Usage`, which a `uze` typed into a pane needs. Peer uid is still checked
  on both, the key is still required, and a peer the kernel places inside a
  pane is still refused on both. Rejected: a predictable abstract name (a
  process that outlives the server could bind it first); serving everything
  on the abstract socket (a sealed `uze` could not open a space at all).
- **Client.** Attach, the build probe and stop (and the public
  `uze_terminal::connect`) connect to the name in `workspace.operator` when
  it is there and answers, else to the pathname socket, and refuse a peer
  that runs confined while the client does not (a sealed process standing
  in for the server). The server writes the file before binding the pathname socket, so
  a client that sees the socket sees the current name; the endpoint watch
  puts it back like the key.
- **A server started inside a pane is not the person's.** A sealed agent
  can kill the server (signals are not scoped) and start `uze terminal
  serve` itself, inside its own domain, where it can reach that server's
  abstract socket. Such a server inherits `no_new_privs`. A client that does
  not itself run under `no_new_privs` retires a listener that does, and
  starts its own (`uze_platform::seal::runs_confined(pid)`). A client in
  a sandboxed terminal (Flatpak) runs confined too and keeps attaching.
- **Protocol version 24.** The wire is unchanged; the meaning of an attach
  on the pathname socket is not, and an older client must replace a newer
  server rather than be refused by it forever.

### TERM-2: bounded, unpersisted pane-opened spaces

ADR-056 lets a pane open a space because a `uze` typed into a pane needs it.
That need is one space at a time, seen by the person. So a space created by
`OpenSpace` is held as *unvisited*: at most 3 per requesting pane and 8 in
total, the rest refused with `Refused`; it is left out of `workspace.json`
until a client selects it (a `SelectSpace`, or an attach seated in it), when
it becomes the person's like any other. A restart therefore never relaunches
what a pane opened and nobody looked at. The bound applies to every
`OpenSpace`, whatever its origin, so an escaped helper is bounded too.
Rejected: a rate limit (a slow loop still fills the workspace); refusing
`OpenSpace` from panes (breaks ADR-056's nested launch).

### WKS-4: the kernel says who asks

`src/asker.rs` keeps the env stamp and adds
`uze_platform::seal::this_process_runs_confined()`: on Linux,
`prctl(PR_GET_NO_NEW_PRIVS)`. Every sealed pane has it, and so do Codex's
sandbox, bubblewrap (Claude Code, Flatpak) and Docker's
`no-new-privileges`; it is inherited and never cleared, so neither
`env -u` nor `script` sheds it. Refused processes are told to answer from an
unsandboxed terminal of their own. Rejected: probing the operator endpoint
(only answers while a server runs, and an agent can kill it first); an
HMAC on the approval record (the key would sit in the same writable home -
no disk-only scheme fixes a same-uid unconfined writer).

The approval record: UZE names an agent's checkout and
`runtime/projects/<id>/…` as writable to a harness (`--add-dir`), never
`state/`. A test pins that. An agent whose harness confines writes cannot
forge `approved-commands.json`; one with no write confinement can, which is
ADR-055's stated boundary.

### WKS-5: no proof is no proof

`Agent::admits_launch` refuses a record with no digest. A subagent is issued
a key at split whose secret is dropped: subagents never call the agent
verbs as themselves (they run under the parent's stamp and are refused
inside a child's checkout), and a key on the record also protects it from an
older build on the same machine that still admits key-less records. No shape
change: the field is the same, only its reading changes, so no ladder rung.
A pre-ADR-056 record (no key) is refused until its next placement issues
one; its persisted tab then starts its harness fresh instead of resuming.

## Candidate ADRs

- **An agent pane is sealed from the person's endpoint by the kernel** -
  amends ADR-056: the two-door endpoint and Landlock scoping replace session
  and ancestry as the boundary where the kernel offers it.
- **Who asks is read from the kernel** - amends ADR-055's "refuses inside an
  agent UZE launched": `no_new_privs` also refuses.

## Risks / Trade-offs

- [`no_new_privs` in agent panes] set-uid programs (`sudo`, set-uid
  `bwrap` on distributions without unprivileged user namespaces) fail
  inside an agent's pane → stated; unprivileged-userns `bwrap`, Codex's
  Landlock and seccomp sandboxes all run under it (verified with `bwrap`
  here).
- [abstract sockets] a program in an agent's pane cannot reach an abstract
  socket made outside it: X11's `@/tmp/.X11-unix/X0` (Xlib falls back to the
  pathname socket), D-Bus addresses of `unix:abstract=` form (systemd's bus
  is a path) → stated.
- [Asker false positive] a person in a sandboxed terminal (Flatpak) cannot
  approve or consent there → told why, and answers from another terminal.
- [Residual: shell panes] an agent typed into the person's own shell pane is
  unsealed and keeps ADR-056's residual.
- [Residual: ways around the seal] found while fixing it, not closed here →
  tracked privately in GHSA-w6jq-59jg-h4pg until fixed.
- [Residual: same-uid writer] an unsandboxed agent can write
  `$UZE_HOME/state` (approvals, task records) and replace endpoint files;
  only a harness sandbox closes that.

## Migration Plan

Protocol bump: the first client of this build retires a running server of
the previous one, which restores its panes. Key-less agent records are
refused until re-placed. Rollback is a revert; nothing persisted changes
shape.
