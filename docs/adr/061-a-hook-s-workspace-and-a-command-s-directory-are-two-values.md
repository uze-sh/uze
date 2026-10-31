# A hook's workspace and a command's directory are two values

Status: Accepted

## Context

The portable handler contract (ADR-040) gives a handler `HOOK_CWD`, the
workspace directory, and every wrapper read it from the payload's `cwd`.
That field means something different on each harness, and it moves:
Codex 0.166 reports there the directory a shell call runs in, which the
call itself names with `workdir`; Claude Code reports the shell's own
directory, which a `cd` moves. A guard that confines a call to the
workspace, and the rules engine finding the checkout it answers for, were
reading a value the call chooses.

## Decision

**`HOOK_CWD` is the session's workspace, read on each harness from a
source the call does not choose; `HOOK_WORKDIR`, given to a `shell` call
only, is the absolute directory its command runs in.**

- The source of each is a per-harness fact in its hook dialect, measured
  in the Lab rather than read from documentation: Claude Code's
  `CLAUDE_PROJECT_DIR`; the directory Codex starts a hook in, which is its
  session's under a shared app-server too; Antigravity's
  `workspacePaths[0]`; the location OpenCode set the plugin up for.
- `HOOK_WORKDIR` is the directory the harness reports for the call,
  relative to the workspace when it is not absolute, and the workspace
  where the harness reports none or its command tool cannot name one
  (Antigravity). It is empty for every other tool and on `stop` and
  `session_start`.
- The rules engine finds the checkout from the workspace and reads a
  command's relative paths against its directory; a command run outside
  the checkout contributes only the absolute paths that lead into it.
- What a harness reports after a call is taken as it is, without state
  carried from before the call: on Claude Code a `post_tool_use` handler is
  told where the shell was left, so a `cd` in the command moves it. The
  Lab declares that as a measured limitation.

The rejected alternative was redefining `HOOK_CWD` as the command's
directory: it would have changed the meaning of a variable handlers
already confine by, on every harness, to follow one vendor's change.

## Consequences

A handler written against the contract keeps confining to the workspace
whatever directory a call names, and one that resolves paths has the
directory to resolve them in. Each harness's sources are re-measured by the
hooks contract (`conformance/contract/hook_directories.py`), so a vendor
moving one again fails a check instead of moving the workspace. A new
harness names both sources in its dialect, and one with no source a call
cannot steer has no workspace to give.

Source change: openspec/changes/audit-hook-cwd/
