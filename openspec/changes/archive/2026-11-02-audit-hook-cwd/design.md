## Context

See proposal.md for the finding. A hook's context is built in three
places: the generated wrapper (POSIX and PowerShell), from a per-harness
dialect (`PayloadPaths` in `claude/`, `codex/`, `antigravity/hooks.rs`);
the OpenCode bridge, inside the harness's own runtime; and the rules
engine, a separate command hook on Claude Code and Codex and a plugin
section on OpenCode. All three took the workspace from the payload's `cwd`
(the bridge from the shared service's directory).

## Goals / Non-Goals

**Goals:** `HOOK_CWD` is the workspace on every harness, from a source the
call does not choose; a shell command's directory reaches a handler as
`HOOK_WORKDIR`; the rules engine anchors on the first and resolves paths
against the second; the Lab re-measures every source.

**Non-Goals:** recovering the directory a Claude Code command *started* in
on `post_tool_use` (it would need state carried between events); a `cd`
inside one command line (no harness reports it; the engine's word reading
stays a best effort).

## Decisions

### The workspace source is a per-harness fact

`PayloadPaths.cwd` becomes `workspace: Workspace` and `workdir:
Option<&str>`. `Workspace` is `Payload(filter)`, `Environment(name)` or
`LaunchDirectory`, the last read by the wrapper with `pwd -P` (PowerShell:
`Get-Location`) before it moves to the package root. Each source was
measured with the Lab:

| harness | `HOOK_CWD` | `HOOK_WORKDIR` | measured |
|---|---|---|---|
| Claude Code 2.1.314 | `CLAUDE_PROJECT_DIR` (constant across `cd`) | payload `cwd` | `cwd` and the hook's own directory follow the shell |
| Codex 0.166.0 | the hook's launch directory | payload `cwd` (already absolute) | two sessions in different directories under one app-server: each hook ran in its own session's directory (`experiments/codex/hook-daemon`) |
| Antigravity 1.4.12 | `workspacePaths[0]` | the workspace | a hook runs in its plugin's directory; `run_command` refuses `Cwd` |
| OpenCode v2.2.5 | `ctx.location.directory` | `input.workdir`, joined to the location when relative | `process.cwd()` is the shared service's |

The transcript's first line (`session_meta.payload.cwd`) was the
fallback for Codex; the launch directory needs no file read per call.

### `HOOK_WORKDIR` is the shell alias's, resolved against the workspace

Set in the same place as the portable fields, so a rewrite recomputes it;
bounded and exported with the rest. A relative value is joined to
`HOOK_CWD`; an empty one is `HOOK_CWD`.

### The rules engine takes the workspace from the entry

The engine is one byte-identical file. Its entry may name the variable
holding the workspace (`engine <sessions> CLAUDE_PROJECT_DIR`, Claude's
exec-form entry; a variable that is not a plain name answers nothing);
without one the workspace is the directory it was started in (Codex; the
OpenCode plugin now starts it in the location). The payload's `cwd` is the
command's directory: relative paths read against it, and when it lies
outside the checkout only absolute paths into the checkout count.

### Claude Code's post-tool directory is taken as reported

Operator decision: stateless. The Lab declares
`hooks.post-workdir-is-start` for Claude, measured each run.

## Risks / Trade-offs

- [A Claude hook started without `CLAUDE_PROJECT_DIR`] → `HOOK_CWD` is
  empty, which the contract allows; it never falls back to the shell's
  directory.
- [Codex changes where it starts hooks] → the Lab's
  `hooks-directories-*` checks fail on that version.
- [A Claude rules entry from an earlier release] → it lacks the argument
  and the engine reads the launch directory, the shell's; `uze setup`
  rewrites it with the receipt's previous value.

## Migration Plan

Regenerated wrappers and engine are written on the next `uze setup` or
install; nothing to migrate by hand.
