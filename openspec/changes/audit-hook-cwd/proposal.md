## Why

- **HOOKCWD-1**: `HOOK_CWD`, which the portable contract promises is the
  workspace, is read from a payload field that Codex 0.166 fills with the
  directory a shell call runs in, a directory the call names itself. The
  same read on Claude Code follows the shell's `cd`. A handler confining to
  the workspace, and the rules engine finding its checkout, trust it.

The property this change establishes: a hook's workspace comes from a
source the call cannot choose, on every harness, and where a command runs
reaches a handler as a value of its own.

## What Changes

- `HOOK_CWD` is read per harness from the session: Claude Code's
  `CLAUDE_PROJECT_DIR`, the directory Codex starts its hook in,
  Antigravity's `workspacePaths[0]` (unchanged), and the location OpenCode
  set the plugin up for (instead of the shared service's own directory).
- `HOOK_WORKDIR` is added for the `shell` alias: the absolute directory the
  command runs in, `HOOK_CWD` where the call names none; empty for every
  other tool and on `stop` and `session_start`.
- The rules engine finds the checkout from the workspace (an argument
  naming Claude Code's variable, the directory it is started in elsewhere)
  and reads a command's relative paths against the directory it ran in.
- The Lab's hooks contract asserts both values per harness, with the two
  limitations it measures declared.

## Capabilities

### New Capabilities

### Modified Capabilities
- `portable-hooks`: the handler context names the workspace and a shell
  command's directory apart, each from the source the harness keeps it in.

## Impact

`crates/uze-integrations` (the hook dialects of Claude, Codex and
Antigravity, the POSIX and PowerShell wrappers, the OpenCode bridge, the
rules engine and its OpenCode plugin), their goldens and tests;
`docs/capabilities/portable-hooks.md`; ADR-061; the Lab's hooks contract
and registry. A Claude rules entry gains one argument, rewritten on the
next `uze setup`.
