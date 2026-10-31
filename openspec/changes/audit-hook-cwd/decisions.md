## D1 — Codex's workspace is the hook's launch directory, not the transcript

- status:   pending
- task:     2.2
- commit:   squashed on delivery
- trigger:  the operator asked to measure whether a Codex hook's process directory follows its session under a shared app-server, and to fall back to the transcript's first line otherwise.
- decided:  measured two live interactive sessions (/work/project, /work/project2) served by one app-server started from /work/project; every hook ran in its own session's directory, and matched `session_meta.payload.cwd`. `Workspace::LaunchDirectory`.
- rejected: the transcript head (a file read and a `jq` per call, and a path the payload hands over).
- blast:    a Codex release that starts hooks elsewhere fails the Lab's directory checks.

## D2 — Claude's workspace has no fallback to the payload

- status:   pending
- task:     2.2
- commit:   squashed on delivery
- trigger:  `CLAUDE_PROJECT_DIR` could be missing in some launch the Lab does not exercise.
- decided:  an empty `HOOK_CWD` (the contract's "may be empty") rather than the shell's directory.
- rejected: `${CLAUDE_PROJECT_DIR:-<payload cwd>}`, which would bring back a value the call moves.
- blast:    a handler sees an empty workspace in such a launch and decides as it would for any harness that reports none.

## D3 — The rules engine names its workspace by an entry argument

- status:   pending
- task:     3.1
- commit:   squashed on delivery
- trigger:  the engine is one shared file; Claude's entry is exec form, which expands no variable.
- decided:  an optional second argument naming the variable (`CLAUDE_PROJECT_DIR` on Claude), the launch directory otherwise; the OpenCode plugin spawns the engine in the location and passes a call's `workdir` as `cwd`.
- rejected: reading `CLAUDE_PROJECT_DIR` unconditionally inside the engine (a Codex launched from a Claude shell would inherit it).
- blast:    the Claude rules entry changes once; the previous one is replaced by the receipt.

## D4 — Claude Code is the one harness whose shell is scripted with no named directory

- status:   pending
- task:     4.1
- commit:   squashed on delivery
- trigger:  the Lab cell scripts a call naming a directory wherever the command tool can carry one.
- decided:  Claude's `Bash` declares no directory parameter (declared tools, 2.1.314), so its cell scripts only the `cd` cases; Antigravity's `Cwd` is scripted and its refusal declared, since its schema once carried one.
- rejected: scripting an undeclared `workdir` on Claude, which the harness rejects before any hook and would only measure its validator.
- blast:    none; a Claude release that adds the parameter needs one `shell_in` change.
