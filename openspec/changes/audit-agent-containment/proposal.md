## Why

UZE tells an agent from the operator by signals the agent can shed or
forge (findings.md). TERM-1 breaks "a pane is never the person": a
`setsid --fork` leaves the pane's session and the key file makes the
escapee the operator. TERM-2 breaks "a pane may open a space and nothing
else" by opening unbounded spaces that persist and relaunch. WKS-4 breaks
"an agent cannot approve its project's commands" through the environment
and a terminal emulator. WKS-5 breaks "an agent's identity is bound to its
launch": a record with no key admits any key.

The property this change establishes: whether a caller is an agent is
decided by something the agent cannot shed (the kernel), not by its
environment, session, ancestry or a file it can read; where the kernel
cannot say, the residual is stated, not hidden.

## What Changes

- On Linux with Landlock ABI 6 or later, every pane UZE launches a program
  in (an agent's pane) starts sealed: a Landlock domain that handles no
  filesystem access and only scopes abstract Unix sockets, which neither the
  program nor any descendant can leave.
- The terminal runtime serves the person's verbs (attach, stop) at an
  operator endpoint a sealed pane cannot reach: an abstract socket of a
  random name. The existing endpoint keeps the verbs a pane may use
  (open a space, read usage) and refuses the person's verbs while the
  operator endpoint stands. Pane classification and the key stay as further
  layers. **BREAKING** (wire): the protocol version moves, so a client and a
  server of different builds replace each other as for every bump.
- A client does not attach to a server that runs confined (no_new_privs)
  when the client itself does not: such a server was started from inside a
  pane, and it is retired and replaced.
- Spaces opened by `OpenSpace` are bounded (per requesting pane and in
  total) and are not persisted, so they never relaunch, until the operator
  visits them.
- "Who is asking" also reads the kernel: a process running under
  `no_new_privs` (every sealed pane, and every common harness sandbox) is
  refused the operator's answers (`uze workspace allow`, extension consent).
- An agent record without a launch-key digest admits no claim; a subagent's
  record is issued a key at split that no process holds.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-runtime`: the operator endpoint, sealed agent panes, the client's
  refusal of a confined server, and bounded, unpersisted pane-opened spaces.
- `worktree-policy`: who may answer for the operator is read from the
  kernel as well as the environment; an agent's claim needs a key.
- `checkout-accounting`: a subagent's record carries a key and a claim
  naming it without that key is refused.

## Impact

- `crates/uze-platform`: a new `seal` concept: the sealed spawn, the sealed
  (abstract, Linux) endpoint, and whether a process runs confined.
- `crates/uze-terminal`: server doors, pane spawn, client connection,
  `OpenSpace` bounds and persistence; protocol version bump.
- `crates/uze-workspace`: `Agent::admits_launch`.
- `crates/uze-application`: subagent split issues a key.
- `src/asker.rs`: the kernel reading.
- Amends ADR-056 (the residual it named) and ADR-055 (the asker it relies
  on); see design.md.
