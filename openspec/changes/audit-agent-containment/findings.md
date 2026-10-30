Audited commit: 98d1210f · Date: 2026-10-30 · Skill: `security-audit`

These findings share one root cause. UZE tells an agent from the operator by
signals the agent controls: its environment, its session id and process
ancestry, and files under `$UZE_HOME` that it can read or write. It never
asks the kernel. ADR-056 and ADR-055 name parts of this as residuals. The
audit shows each residual costs the agent one line.

## TERM-1 - A pane escapes its classification with `setsid --fork` and takes over the operator's terminal

- **Severity**: High. The agent gains arbitrary command execution as the
  operator outside any sandbox, can type into the operator's own shell pane,
  can read every pane and can stop the runtime.
- **Status**: CONFIRMED. The PoC ran against `uze terminal serve` with a
  hostile client.
- **Threat actor**: a prompt-injected agent in a pane (3).
- **Preconditions**: the agent can read `$UZE_HOME`. UZE does not confine
  panes.
- **Location**:
  - `crates/uze-terminal/src/runtime/server.rs:704` (`origin_of`) and `:724`
    (`refusal`).
  - `crates/uze-platform/src/process/unix.rs:399` (`belongs`) and `:416`
    (`descends_from`).
  - The key lives at `$UZE_HOME/state/terminal/workspace.key` (0600, same
    uid), written by `crates/uze-terminal/src/runtime/key.rs`.
- **Attack**: `setsid --fork attacker attach …` gives the helper a new
  session, and its parent is reparented to init, so `origin_of` returns
  `Outside`. The helper then reads the key, `Attach` is accepted, and it
  sends `CreateTab{command:["/bin/sh","-c",…]}` and keyed `Input` into
  pane 1.
- **Evidence**:
  - An in-pane `Attach` is refused (`PWNED-INPANE` absent).
  - The escaped helper creates `PWNED-ESCAPED`, which contains
    `uid=1000(...)`.
  - The keyed `Input` into the existing shell pane creates
    `PWNED-TERM-A.pane1`.
- **Root cause**: whether a peer is a pane is decided by session and
  ancestry, which any process can leave. The operator credential is a file
  every pane can read.

## TERM-2 - A pane opens unbounded spaces, which persist and relaunch on every start

- **Severity**: Medium. Resources are exhausted (PTYs, shells, fds), and the
  operator's workspace stays polluted across restarts.
- **Status**: CONFIRMED.
- **Threat actor**: an agent in a pane (3). It needs no key and no escape.
- **Location**:
  - `crates/uze-terminal/src/runtime/server.rs:294`: a pane-origin
    `OpenSpace` is served.
  - `ensure_space` (`:777`) and `crates/uze-terminal/src/state.rs:337`
    (`open_space`) persist each space to `workspace.json`.
- **Attack**: an in-pane loop sends `OpenSpace` 25 times, with a distinct
  project path each time.
- **Evidence**: 25 spaces (`sp1` to `sp25`) persist in `workspace.json`, and
  33 processes run in the box.
- **Root cause**: a verb that panes may use has no bound. What a pane opens
  is persisted like what the operator opens.

## WKS-4 - An agent obtains the project-command approval

- **Severity**: Medium. ADR-055 says it stops "an agent from approving
  through the surfaces UZE offers it". Once approved, `setup` and `gate` run
  outside the harness sandbox.
- **Status**: CONFIRMED.
- **Threat actor**: an agent (3).
- **Location**:
  - `src/asker.rs:11-20` checks only the `UZE_AGENT` and `UZE_AGENT_KEY`
    environment variables.
  - `src/prompt.rs:37-40` treats `isatty` as proof that a person is present.
  - `crates/uze-workspace/src/approval.rs:79-90,192-205` stores an unkeyed
    SHA-256 in a JSON file the agent can write.
- **Attack**: two ways.
  - Run `printf y | env -u UZE_AGENT -u UZE_AGENT_KEY script -qec 'uze workspace allow' /dev/null`.
  - Or write `{"root":…,"digest":"sha256:<computed>"}` to
    `state/projects/<id>/approved-commands.json`.
- **Evidence**: run directly, `uze workspace allow` answers `error: an agent
  cannot approve…`. The env-stripped run under `script` prints `✓ approved
  2 commands`, and after the forged record it answers `nothing to approve`.
- **Root cause**: "who is asking" is read from the environment, and the
  record proves nothing about who wrote it.

## WKS-5 - Subagent records carry no launch key, and a key-less record admits any key

- **Severity**: Low. Any process can act as another agent's subagent:
  rename its branch, split checkouts (which runs the approved `setup`), and
  join commits that the parent later delivers.
- **Status**: CONFIRMED.
- **Location**:
  - `crates/uze-application/src/application/services/work.rs:99-133`
    creates the child with `launch_key: None`.
  - `crates/uze-workspace/src/task.rs:394-401` (`is_none_or`) admits any
    secret.
- **Evidence**: with `UZE_AGENT=zvtwtn UZE_AGENT_KEY=guessed-anything`,
  `work list` exits 0 and `work split grandchild` creates
  `.worktrees/b9s0ol`.
- **Root cause**: absent proof is treated as valid proof.

## Defenses that held

- The socket directory is 0700, the socket 0600 and the key file 0600, all
  written atomically.
- `SO_PEERCRED` refuses another uid on accept and on connect.
- A non-escaped pane's `Attach` and `Stop` are refused, and so is a wrong
  key.
- Frames are bounded (`MAX_FRAME` 64 MiB, handshake frame 64 KiB) before
  allocation, and the handshake has a whole-exchange deadline.
- Pane dimensions are clamped at every `Attach` and `Resize`.
- The pane reply sink drops OSC 52 and title, DECRQSS and other report
  responses. The client renders cell state, not raw bytes.
- Socket-path squatting forces a fallback to an unpredictable directory. A
  symlinked or foreign-owned directory is refused.
- The approval digest framing (step plus u64 length) has no collisions.
  The hasher and the runner use one parsed policy, and the approval is keyed
  on the canonical root.
- `uze workspace allow` refuses when the agent stamp is set or no tty is
  present (but see WKS-4).
