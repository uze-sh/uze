## Context

Exploration: see proposal.md. This file holds what was decided for
Claude's native `AGENTS.md` reading while it lived in
`refresh-harness-deliveries` (rebuilding PR #154's outcome on current
`main`). It is kept as input, and is revised once the option in the
proposal is chosen.

## Carried from refresh-harness-deliveries

### C1. Claude context: rebuild #154's outcome
`ContextDelivery::Native { files, shadowed_by }` — `files` stays, because
Antigravity declares `GEMINI.md` there (`antigravity.rs:276-280`, consumed
in `context.rs:62`). Claude declares `shadowed_by: [CLAUDE.md,
.claude/CLAUDE.md, CLAUDE.local.md]`. `ContextDelivery::Bridge` and
everything only it used (bridge plan/reconcile, `BridgePlan`,
`BridgeStatus`, `ContextMechanism::Bridge`, `ResourceDelivery::Bridged`,
`HarnessContextDelivery` `BRIDGE`/`PROJECTED`) are deleted.

Shadowing is judged at the project root. A file does not shadow when it is
empty, imports `@AGENTS.md` or `@./AGENTS.md`, resolves (symlink) to the
project's own `AGENTS.md`, or holds only an earlier build's
`instruction-bridge` region. The shim drops the `@AGENTS.md` projection and
`CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD` — the vendor doc says a
`--add-dir` with that variable does not load `AGENTS.md`, so keeping it
would be wrong — and activates only when `.agents/` exists, projecting what
of `.agents/` Claude does not read (skills and agents today; `project-rules`
may add rules). Floor: 2.1.281.

`/uze:init` gets #154's whole rework, not just Flow B: Flow B's
precondition is "no AGENTS.md", so the shadowed case needs the
`PARTIALLY_PORTABLE` routing, the `PORTABLE` summary, the MUST NOT lines,
Flow B's title and the plugin description too.

How a project with vendor files reaches every harness, and how and whether
it is moved onto `AGENTS.md`, is the `resolve-instruction-files` change:
this one only reports the shadowing and keeps `/uze:init` routing to the
remedy as #154 did.

**BREAKING** surfaces: `agent context plan|reconcile --format json` lose
`bridges`; `agent context inspect --format json` changes
`HarnessContextDelivery`.

Alternatives kept from #154's review: keeping `Bridge` with no declarer
(rejected, dead code); removing earlier builds' regions (rejected, they
still deliver through the import and are the operator's file).

Probe 1.1: Lab contract leg on the current Claude with no `CLAUDE.md` and
no runtime import; `CLAUDE.local.md` shadowing; and a launch inside a slot
(`.worktrees/<id>`, nested under the primary) to confirm which
`AGENTS.md`/`CLAUDE.md` reaches the model when Claude walks ancestors.


## Candidate ADRs

- Claude reads `AGENTS.md` natively; the `CLAUDE.md` bridge is removed —
  revises ADR-014's runtime projection and retires the bridge mechanism.

## Risks / Trade-offs

- [Shadowing judged at the root only] → a `CLAUDE.md` above the root or a
  user `instructionFiles: "claude-md"` also disables the reader and UZE
  cannot see either; stated in the Claude README, as in #154. Probe 1.1
  covers the nested-slot case.
- [`agent context --format json` changes shape] → `uze agent` is an
  agent-audience ABI; the change is marked breaking.
