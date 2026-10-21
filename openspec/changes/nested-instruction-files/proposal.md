## Why

A repository can carry an `AGENTS.md` in a subdirectory (`web/AGENTS.md`)
with instructions that only apply there. Whether an agent receives it
depends on the harness today. All four read every `AGENTS.md` from the launch
directory up to the root. Only Claude Code and OpenCode also read the one
in a subdirectory below the launch directory when the agent works on a file
there. Codex and Antigravity never do. UZE promises one project context
read the same way by every harness, so an agent's instructions
must not depend on which harness it runs in.

Measured on 2026-10-20 through the UZE shims, with a root `AGENTS.md` and
`sub/AGENTS.md`:

| Harness | Launched in `sub/` | Launched at the root, reads `sub/notes.txt` |
|---|---|---|
| Claude Code 2.1.305 | root + nested | root + nested (injected on read) |
| OpenCode 2.0.32 | root + nested | root + nested (injected on read) |
| Codex 0.163.0 | root + nested | root only |
| Antigravity 1.4.3 | root + nested | root only |

## What Changes

- An `AGENTS.md` in a directory below the launch directory reaches the
  agent, once, the first time the agent reads, edits or writes a file under
  that directory through a file tool. This holds in all four harnesses, or
  a harness says why it cannot. Shell commands are best effort.
- Where the harness does this itself (Claude Code, OpenCode), UZE does
  nothing. Where it does not (Codex, Antigravity), the rules engine hands
  the file over through the same path-matched hook that delivers glob
  rules, so it arrives the way a nested `AGENTS.md` arrives in Claude.
- Each integration declares how it receives nested instruction files
  (native, rules engine, or unsupported with a reason). `uze status`, the
  harness drawer and `uze agent context inspect` report it next to
  `AGENTS.md`.
- The Lab proves the outcome per harness with a nonce in a nested
  `AGENTS.md` and nothing in the prompt pointing at it.
- UZE writes nothing into the project: no generated rule, no import line,
  no copy of the nested file.

Shadowing (a `CLAUDE.md` that makes Claude skip every `AGENTS.md`) stays
in `resolve-instruction-files`, which owns which file a harness reads in
place of `AGENTS.md`. That change's draft is updated with the measurement
that any `CLAUDE.md` on the walk, not only at the root, shadows all of
them.

## Capabilities

### New Capabilities
- `nested-instruction-files`: how an `AGENTS.md` below the launch directory
  reaches each harness, what UZE delivers where the harness does not read
  it, how it is reported, and the Lab proof.

### Modified Capabilities

None. `project-context-delivery` is still a draft inside
`resolve-instruction-files`, and its shadowing requirement is corrected
there.

## Impact

- `uze-core`: the integration port gains the nested-instruction
  declaration.
- `uze-integrations`: each integration declares it. The rules engine
  (`rules/engine.rs`) walks a touched path up to the launch directory and
  answers with each nested `AGENTS.md`, before its rules gate; its hook
  entries gain an argument (extends ADR-040 D8). Antigravity gets the
  engine for nested files only if a measurement shows its hooks can
  deliver; otherwise it reports the reason.
- `uze-application`, `src/cli/status.rs`, `src/ui/view/harnesses.rs`: one
  more context row. **BREAKING** for `uze agent context inspect --format
  json`, which gains a field.
- `docs/architecture/invariants.md`: the engine's "no rules, one directory
  check" invariant changes, and Antigravity's "given nothing" one if it
  gets the engine.
- `conformance/`: a contract check for every harness.
- No change to what UZE writes into a project.
