## Context

See proposal.md for the measured matrix. What the code has today:

- **Ancestors are already uniform.** Every harness reads each `AGENTS.md`
  from the launch directory upward on its own. In a slot under
  `.worktrees/`, Claude and Antigravity keep walking past the checkout root
  into the primary (`antigravity/agent_context.rs`, and probe 1.1 of
  `resolve-instruction-files`). That belongs to the slot's own story, not
  to this change; the spec only covers files *below* the launch directory.
- **The rules engine** (`crates/uze-integrations/src/rules/engine.rs`, one
  POSIX `sh` file, ADR-040) is attached to Claude Code and Codex as a
  `SessionStart` + `PostToolUse` command hook and asked by OpenCode's
  generated plugin. On `PostToolUse` it recovers touched paths (exact for
  file tools and patches, best effort for shell words), matches glob rules,
  answers `hookSpecificOutput.additionalContext`, and records per
  `session_id` what it handed, forgetting on a compact or clear start.
  It exits early when the checkout has neither `.agents/rules` nor
  `.agents/rules.json` (line ~318), and `invariants.md` promises that a
  checkout without rules costs one directory check.
- **Setup attaches the engine** only where `rules_route().needs_engine()`
  (`uze-application/src/application/rules_engine.rs`). Antigravity's route
  is native, so it gets nothing, and `invariants.md` plus
  `rules/tests.rs::antigravity_is_left_to_its_own_rules` say so.
- **Antigravity hooks** (`antigravity/hooks.rs`, `agent_context.rs`): the
  measured context channel is `injectSteps[].systemMessage` on
  `PreInvocation`; its `PostToolUse` answers with the empty object today.
  Its `SessionStart` only has `startup`, and `/compact` is gated, so there
  is no compaction to reset on. Hooks run only while signed in.
- Claude Code and OpenCode inject a nested `AGENTS.md` on their file-read
  tool (measured). Edit, write and shell reads are not measured yet.

## Goals / Non-Goals

**Goals:**
- One answer to "which instruction files does this agent have", whatever
  the harness.
- Reuse the engine's path recovery, session record and compaction reset.

**Non-Goals:**
- Vendor files and shadowing (`resolve-instruction-files`). This change
  only reports the shadowing that change detects on its own row.
- `AGENTS.override.md` or per-vendor file names.
- Nested `.agents/skills` or `.agents/rules`.
- Delivering a nested file to a Claude subagent's parent: the engine keys
  its record on the `session_id` the harness sends, which a subagent
  shares. Same gap as glob rules; stated in the README.

## Decisions

### D1. The engine answers a touched path with its nested `AGENTS.md`

On `PostToolUse`, for each touched path, the engine walks from the path's
directory up to the session's launch directory (the payload's `cwd`,
excluded, since those files arrived at session start) and emits each
`AGENTS.md` it finds, outermost first, as
`<instructions path="sub/AGENTS.md">body</instructions>`. It records each
one per session like a glob rule.

The nested walk runs **before** the rules gate: the early exit becomes
"no rules, no manifest, and no `AGENTS.md` below the launch directory on
the touched paths". The cost for a checkout without either is one
directory check plus one `test -f` per directory between each touched
path and the launch directory. `invariants.md` and
`a_checkout_without_rules_or_outside_git_pays_nothing` are rewritten to
that.

Alternatives: a synthetic glob rule per nested file (`sub/**`) generated
into the project or into `~/.uze`. Rejected: writing into the project is
excluded by the spec, and a machine-side copy goes stale.

### D2. Each integration declares how it gets nested files

A new port declaration, `nested_instructions()`: `Native`, `RulesEngine`,
or `Unsupported { reason }`. Claude Code and OpenCode declare `Native`,
Codex `RulesEngine`. The hook payload does not say which harness is
calling, so the engine learns it from a second argument written into each
harness's hook entry from its `RulesTarget` (`--nested` or nothing). This
extends ADR-040 D8 ("the sessions directory arrives as its only
argument"); every engine receipt changes and setup rewrites the entries.

Alternative: always answer and let the harness dedupe. Rejected: Claude
dedupes files it loaded itself, not text a hook added.

Each declaration gets a `HarnessFact` in the integration's `facts()` with
`proven_by` naming the Lab check.

### D3. Antigravity: measure first, then one of two outcomes

Delivering to Antigravity needs four things the engine does not have:
setup attaching it to a harness whose rules route is native, an output in
Antigravity's dialect (`injectSteps`), a post-tool hook that can inject
at all, and a launch directory (`workspacePaths[0]` may be the git root,
which would double-deliver `sub/AGENTS.md` when launched in `sub/`).
Task 1.1 measures the last two on the current version.

- **If the post-tool hook can inject and the launch directory is known:**
  setup attaches the engine to Antigravity for nested files only
  (`deliver_rules_engine` gains the nested condition, as do
  `remove_rules_engine` and `doctor`); the engine gains an Antigravity
  output mode selected by the D2 argument; `invariants.md` and
  `antigravity_is_left_to_its_own_rules` change to "rules native, nested
  files through the engine"; `core-components.mmd` gains the edge. No
  compaction reset (Antigravity has none).
- **Otherwise:** Antigravity declares `Unsupported` with the measured
  reason. The spec allows it, and the report says it.

### D4. Reported as one more context row

`HarnessContextSupport` gains `nested_instructions`, rendered by the same
`context_row` as `AGENTS.md`. When `resolve-instruction-files` reports
Claude's context shadowed, the nested row takes the same state, since a
shadowing `CLAUDE.md` turns off every `AGENTS.md`, nested ones included.
`agent context inspect --format json` gains a field: **BREAKING** for that
shape, like the sibling change.

## Candidate ADRs

- Nested instruction files are delivered by the rules engine, not by
  generated files. Recorded as an extension of ADR-040 (the engine's
  argument contract and its early exit), not a new number.

## Risks / Trade-offs

- [Shell paths are best effort] → Same gap glob rules have; stated in the
  reason (spec: "best effort" requirement).
- [Native harnesses diverge on edit or shell reads] → Task 1.2 measures it.
  Where Claude or OpenCode skip a tool the engine would catch, the
  difference is stated in the reason rather than patched with a second
  delivery that could duplicate.
- [A `sub/CLAUDE.md` below the launch directory may shadow `sub/AGENTS.md`
  in Claude] → Task 1.3 measures it. If it does, it is a shadowing case for
  `resolve-instruction-files` to report.
- [Size] → Codex `project_doc_max_bytes` (32 KiB) and Antigravity's rule
  file limit truncate their native walks, while the engine delivers the
  whole file. Noted in each README's row.
- [Claude or OpenCode stop injecting nested files] → their Lab check fails
  and the declaration moves to `RulesEngine`.
