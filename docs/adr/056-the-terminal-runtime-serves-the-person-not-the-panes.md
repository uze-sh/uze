# The terminal runtime serves the person, not the panes

Status: Accepted

## Context

ADR-038 made the terminal runtime's endpoint "local and user-private": a
socket in a directory only the user can enter, a pipe whose ACL names the
user. That keeps other accounts out and lets every process of the user in,
and the runtime treated every connection the same way. A connection could
attach and receive every pane's contents, type into any pane, create a tab
running any command with any environment (persisted in `workspace.json` and
started again on the next restore), and stop the runtime.

The processes most likely to reach that endpoint are the ones the runtime
itself starts: the agents in its panes. A harness that runs its agent in a
sandbox (bubblewrap, Landlock, Seatbelt) usually leaves the user's home
readable and a Unix socket reachable, so a sandboxed agent could ask the
runtime to run a command outside its sandbox, or type into the operator's
own shell pane. Separately, the agent identity (`UZE_AGENT` plus the
directory a process stands in) was the caller's to choose, so one agent
could rename or join another's work.

Two designs were weighed:

1. **A key only.** A per-runtime secret in a file only the user can read,
   required to attach. Every process of the user can read that file unless
   its sandbox denies it, so on its own this keeps out only sandboxes
   configured to hide the home directory.
2. **Classifying the peer only.** The kernel stamps the connecting
   process's pid on the connection; the runtime asks whether that process
   belongs to one of its panes. A process can leave a pane on purpose (a
   new session, then orphaned to init), so on its own this is defeated by a
   double fork.

## Decision

**The runtime serves the person at the terminal, and a pane is never that
person.** It uses both layers, each covering the other's escape:

- **Another user is refused** before anything is read, on the kernel's
  account of the peer (`uze_platform::endpoint::peer_is_another_user`),
  whatever the endpoint's permissions say.
- **A connection from inside a pane may open a space and nothing else.**
  `uze_platform::process::pane::belongs` answers it per platform: on Unix
  the peer is in the session the pane's program leads or descends from that
  program; on Windows it is in the pane's Job Object. The one request a pane
  is served is `OpenSpace`, answered with the space's label alone
  (`SpaceOpened`), which is what a `uze` typed into a pane needs. It is
  shown no snapshot, given no input, and cannot stop the runtime, even when
  it offers the key.
- **Attaching and stopping take the runtime's key.** Each server mints 32
  random bytes when it takes the workspace claim and writes them, readable
  by the user alone, beside the claim (`state/terminal/workspace.key`).
  Nothing puts the key in a pane's environment. `Attach` and `Stop` carry
  it; a connection without it is answered `Refused`. A refusal is its own
  event so that a client asking whether a runtime speaks its protocol never
  reads a refusal as "no" and replaces a live runtime over it.
- **Pre-handshake connections are bounded** (`MAX_UNATTACHED`), and the
  writer thread starts only once a connection has attached.
- **An agent's identity is bound to its launch.** Every placement issues a
  fresh secret, carried in the tab's environment (`UZE_AGENT_KEY`) and kept
  in the agent's record only as a SHA-256 digest. `uze agent work …`, the
  shim's continuity and the client's conversation refresh all verify it
  with the identifier and the directory; the identifier and the directory
  alone prove nothing.

The protocol version moves to 20: `Attach` and `Stop` carry the key, and
`OpenSpace`, `SpaceOpened` and `Refused` are new.

## Consequences

A sandboxed agent that reaches the socket can open a space and learn its
label; it cannot see or drive any pane or run anything outside its
sandbox. A process that both escapes pane classification (leaves the
session and is orphaned, or is started through another daemon such as a
pre-existing `tmux` server or `systemd-run`) and can read
`$UZE_HOME/state/terminal/workspace.key` is still served as the person:
that is the residual, and a harness sandbox that denies reading `$UZE_HOME`
closes it. On a platform whose kernel does not name the peer, the key is
the only layer.

A space opened from a pane starts the user's default shell in a directory
the pane chose, outside any sandbox. That shell is given no input by the
pane, but anything its startup reads from that directory (an `.envrc`, a
shell hook) runs unsandboxed; tools like `direnv` already require the
operator's approval per file, and that approval is the guard.

An agent launched by a build before this decision carries no key and its
record no digest: it is refused by `uze agent work …` until it is launched
again. An older build that rewrites the task record drops the digest with
the same effect. A newer client meeting an older runtime (or the reverse)
is told the protocol is incompatible and replaces it, as for every protocol
bump.
