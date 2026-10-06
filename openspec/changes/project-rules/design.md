## Context

See proposal.md for why. What the design has to work with:

- **How the two native harnesses do it.** Neither uses a hook internally,
  but both inject through the channel their hooks also write to.
  - *Claude Code* (2.1.290, transcript of a measured session): after a
    `Read`/`Write`/`Edit` returns, the harness matches the path against each
    rule's `paths` and appends a `nested_memory` attachment, the same one a
    nested `CLAUDE.md` produces. A `PostToolUse` hook's `additionalContext`
    lands in the same place in the turn.
  - *Antigravity* (`agy` 1.2.17, from the binary's symbols and embedded
    text): rules are engine "memories" with
    `CORTEX_MEMORY_TRIGGER_{ALWAYS_ON,GLOB,MODEL_DECISION,MANUAL}`. They are
    injected as `EphemeralMessage` steps, the same type a hook answering
    `{"ephemeralMessage"}` produces. They are deduplicated by resolved path
    within a turn, and `model_decision` is progressive disclosure, like a
    Skill.
- **Claude Code's native route needs the project.** Measured on 2.1.290:
  - a rule with `paths` in an `--add-dir` directory (the route UZE's runtime
    already uses for `.agents/skills`) never fires for a project file,
    whether the glob is relative, `**/`-prefixed or absolute;
  - an unconditional rule there does load;
  - a symlink `.claude/rules → ../.agents/rules` inside the project fires
    correctly, and the transcript names the canonical path.

  Claude reads only `paths` from a rule's frontmatter and ignores every
  other field.
- **The other two harnesses' hook channels:**
  - *Codex* 0.160: `PostToolUse`, `PreToolUse`, `SessionStart` and
    `UserPromptSubmit` accept
    `hookSpecificOutput.{hookEventName, additionalContext}`
    (`codex-rs/hooks/src/engine/output_parser.rs`).
  - *OpenCode* V2:
    - `tool.hook("execute.after")` exposes `event.result.content[]`, which
      the model reads and a plugin may extend;
    - `session.hook("context")` exposes `event.system[]` before every model
      request;
    - the tool input is `event.input`, with the path as `input.path`
      (measured on 2.0.15, not `filePath`).
- **The PoC (2026-10-06)** is the shape to reproduce. It used:
  - one `sh` + `jq` engine answering Claude and Codex directly and
    OpenCode through a V2 plugin;
  - three rules (`glob`, `always_on`, `model_decision`) in `.agents/rules/`.

  Results:

  | | glob | glob scope | always_on ×2 | model_decision |
  |---|---|---|---|---|
  | Claude Code (sonnet) + engine | ✅ | ✅ | ✅ ✅ | ✅ |
  | Codex (gpt-5.6-terra) + engine | ✅ | ✅ | ✅ ✅ | ✅ |
  | OpenCode 2.0.15 (free model) + engine | ✅ | ✅ | ✅ ✅ | ✅ |
  | Antigravity (Gemini 3.8 Flash), nothing from UZE | ✅ | ✅ | ✅ ✅ | ✅ |
  | Claude / Codex / OpenCode, no engine (control) | ❌ | ✅ | ❌ ❌ | ❌ |

  The checks read only the files the agent left behind. Codex's `glob` rule
  arrived from a `sed -n` it ran through its shell.
- **Constraints that hold** (AGENTS.md, ADR-013/033/040/052):
  - `uze-core` names no harness;
  - every managed artifact is receipt-owned and inspected before detach;
  - no `uze` on a delivered artifact's execution path;
  - everything UZE owns is named in `UzeHome` and sits in the tier its
    deletion cost says.

## Goals / Non-Goals

**Goals:**
- One rules directory, one format, no per-harness copy or link in the
  project.
- The same rule reaches the agent at the same moment on all four harnesses,
  with the adapted route as close to the native one as the channel allows.
- Zero per-project activation: a checkout with rules is enough.

**Non-Goals:**
- Rules shipped by plugins. Antigravity installs plugins machine-wide
  (`~/.gemini/config/plugins`), so its native `rules/` would fire one
  project's rule in every project. That needs its own activation design.
- Personal machine-wide rules. `~/.claude/CLAUDE.md` and its peers already
  carry unconditional personal text, and nobody has shown a need for a
  path-scoped personal rule.
- A `PreToolUse` gate that holds an edit until its rule is read. That is
  stricter than either native harness, so it is not parity.
- Windows. The engine is `sh`. Until a PowerShell template exists, the
  adapted route is Unsupported on Windows with that reason, as portable
  hooks are.
- Translating rules from `.cursor/rules` or `.claude/rules`.

## Decisions

### D1: Antigravity's frontmatter is the canonical form
The conflict between formats only exists if two harnesses read the same
files natively. With no `.claude/rules` in the project (D3), only
Antigravity reads `.agents/rules/` itself, so its format costs zero
translation. It is also the richest form: four triggers, and a
`description` that makes progressive disclosure possible. The Windsurf and
Cursor rule formats share its vocabulary.

Alternatives considered:
- **A UZE-owned format.** Every harness, Antigravity included, would need a
  translated copy.
- **Claude's `paths`.** It would cost Antigravity a copy and drop
  `model_decision`/`manual`.
- **Carrying both `paths` and `globs` in one file.** Duplicated patterns
  that only stay correct while something checks them, and it is only
  needed if Claude reads the file natively, which D3 rules out.

### D2: The engine reads the checkout; nothing is compiled
The engine parses `.agents/rules/*.md` from the session's own checkout
(`git rev-parse --show-toplevel` from the payload's `cwd`) on each call.
The PoC compiled an index first. That was dropped because:
- an index is keyed by a project id that the engine, without `uze`, cannot
  reproduce: `project_id_for` uses the workspace's internal digest;
- an index is stale the moment a branch changes a rule;
- an index is wrong for every worktree whose rules differ from the
  checkout it was compiled from.

Reading live makes the checkout the only source, which is what "versioned
with the project" means. The cost is one directory stat per tool call when
there are no rules, and a few small-file reads when there are. The glob
dialect (spec) stays small enough that the engine's translation to an ERE
is a few lines of `sed`/`awk`. Rust and the engine are proven against one
shared table of pattern → path → expected match.

Alternative considered: a compiled index in `runtime/` refreshed by
`uze install`. Rejected for the three reasons above, and because it would
make `uze install` a precondition for rules, which the spec forbids.

### D3: Claude Code is Adapted on purpose
Native delivery on Claude needs `.claude/rules/` inside the project, either
as a copy or as a symlink (measured: the `--add-dir` route only carries
rules without `paths`). That is a second rules directory in every project,
which is what this change exists to avoid. The adapted route uses the
channel Claude's own injection uses (`additionalContext` and
`nested_memory` both arrive as attachments in the same turn), so the loss
is limited to Claude's own `InstructionsLoaded` event not firing.

**Exit condition**, recorded so the decision is revisited rather than
forgotten: if Claude Code evaluates a rule's `paths` from an `--add-dir`
directory against the session's working directory, the runtime links
`.agents/rules` beside `.agents/skills` and Claude becomes Native with no
project write. That needs either a translated `paths` (D1 leaves Claude no
native field) or Claude reading `globs`, so the issue to open upstream asks
for both.

### D4: One engine, installed by setup, not delivered as a package
The rules engine is a UZE-owned artifact that `uze setup` delivers per
harness, alongside detection and the shims. It is not a plugin with a
`hooks.json`:
- the portable hook ABI answers with an exit code and has no channel for
  context (ADR-040 D3), and widening that ABI for one internal consumer
  would put context injection in every author's hands with none of the
  rules' semantics;
- the engine reads project files of the checkout, which no package owns.

Per harness:
- Claude Code: entries in `~/.claude/settings.json`:
  - `SessionStart`, with no matcher, so it fires for `startup`, `resume`,
    `clear` and `compact`;
  - `PostToolUse`, matched on `Read|Edit|MultiEdit|Write|Bash`.

  Exec form, absolute path to the engine.
- Codex: entries in `~/.codex/hooks.json` for `SessionStart` and
  `PostToolUse`. A user-level file is trusted, unlike a project's
  `.codex/hooks.json`, which needs hook trust: measured, the PoC needed
  `--dangerously-bypass-hook-trust`.
- OpenCode: a generated `rules.ts` V2 plugin in the global plugin
  directory, the same place the portable-hook bridge goes.
  - It must be global. Measured on 2.0.15: a plugin in a project's
    `.opencode/plugins/` is never evaluated and hangs
    `opencode run --standalone`.
  - It has no imports, and it spawns the same `sh` engine with Bun.
  - `context` takes the place of `SessionStart`. It runs before every
    request, so reinjection after compaction is inherent.
- Antigravity: nothing.

All entries and files are receipt-owned like portable hooks' entries, and
inspected by content identity before replacement or removal.

### D5: The engine's contract
- **In:** the harness's hook payload on stdin (`hook_event_name`, `cwd`,
  `session_id`, `tool_name`, `tool_input`). OpenCode's plugin builds the
  same document from its events.
- **Out:** `hookSpecificOutput.{hookEventName, additionalContext}` on
  stdout, or nothing. The OpenCode plugin unwraps it into
  `system.push` or `result.content.push`.
- **Rule text** is wrapped as
  `<rule path="…" trigger="…" matched="…">body</rule>`, so the agent can
  name the rule it is following, and `uze doctor` can recognise the
  engine's output in a transcript.
- **Path recovery** reads from, in order:
  - every string field of `tool_input`;
  - `*** (Add|Update|Delete) File:` and `*** Move to:` lines;
  - shell tokens split on whitespace, quotes and operators.

  Paths are made relative to the checkout root, and anything outside it
  is discarded. The shell case is best effort, and Codex's route says so.
- **Deduplication:** a file per session id under
  `runtime/rules/sessions/`, which is generated tier. `SessionStart` with a
  `compact` or `clear` source truncates it, matching the spec's compaction
  scenario. Setup sweeps entries older than a week.
- **Failure:** missing `jq` (the guard reuses the portable wrapper's), no
  git, a malformed rule or an unknown payload means exit 0 with no output.
  Never a denial, never a delay past the entry's timeout.
- **Neutral:** nothing in the engine names the packager, per ADR-040 D8.

### D6: Where the code lives
- `uze-core::project::rules`: parse a rule file, validate the trigger and
  the glob dialect, and discover a checkout's rules. This is what a
  project declares, so it sits in the `project` concern next to
  `context`, and it names no harness.
- `uze-core::UzeHome`: `rules_sessions_dir()` under `runtime/`.
- `uze-integrations`: a per-harness `rules_route()`. It is Native for
  Antigravity, and it is the engine template plus entries for the other
  three, beside each harness's `hooks.rs`. The `sh` engine and the OpenCode
  plugin are templates in `uze-integrations::rules`, generated byte-stable.
- `uze-application`: a rules read model for `status`, engine delivery
  inside `setup`, and a doctor check.

### D7: Authoring follows the diagrams' shape, not the plugin's
A rule is a file in the project, with no marketplace, scaffold or install,
so its authoring surface is the one `uze:architect` already set for
diagrams:
- a deterministic `uze agent rules check`, sharing `uze-core`'s validator
  with `uze status` so the two can never disagree;
- a Skill, `uze:rules`, that carries the judgment the check cannot. It
  covers:
  - whether the instruction is a rule at all. A path-independent
    instruction belongs in `AGENTS.md`, and a procedure belongs in a
    Skill;
  - which trigger to use: `glob` when a path decides relevance,
    `model_decision` when the task does, `always_on` sparingly, `manual`
    for reference material;
  - patterns in the supported dialect;
  - one concern per file, short;
  - a `description` written as *when to read*;
  - wording as a project convention rather than an imperative about the
    agent's reply. The PoC's artificial rule was flagged as a prompt
    injection by a small model;
  - ending with the check.

`uze:author` is not extended. Its trigger text matches "create a plugin",
and every step it guides (marketplace, scaffold, install, publish) is
absent from a project rule. When plugin-shipped rules exist, `author`
gains that capability flag the way it has `--hook` and `--mcp`.

`uze:init` already inventories a project's context files. It learns to
recognise harness-specific rule directories (`.claude/rules/`,
`.cursor/rules/` with `.mdc`, `.windsurf/rules/`, the legacy `.agent/rules/`)
and `AGENTS.md` sections that only apply under a path, and to hand them to
`uze:rules` for migration. The mapping is mechanical:
- `paths` / `globs` become `trigger: glob`;
- `alwaysApply: true` becomes `always_on`;
- `description` alone becomes `model_decision`.

A pattern outside the dialect is flagged for the person to decide, never
silently rewritten.

**Diagrams.** `docs/architecture/core-components.mmd` gains rules in the
`project` component's description, and `containers.mmd`'s
integrations → harness relation becomes "Projects plugins, hooks, rules,
context". Both are updated in this change.

## Candidate ADRs

- **Project rules: Antigravity's format, read from the checkout, adapted
  everywhere else.** It fixes the canonical location and format of a new
  capability, makes Claude Code Adapted for a native feature by choice,
  and puts a project-reading engine on the machine. All three are
  expensive to walk back once projects write rules.

## Risks / Trade-offs

- **Agents may take an injected rule for a prompt injection.** Haiku
  flagged an artificial test rule. → Real rules are guidance, not
  imperatives about the reply. The wrapper names the file the rule came
  from, and the docs tell authors to write rules as conventions.
- **A machine-wide engine reads every checkout's `.agents/rules/`,
  including a cloned untrusted repository's.** → This is the same trust
  class as that repository's `AGENTS.md`, which every harness already
  loads, and as Antigravity's native rules. The engine only reads and
  injects text and executes nothing from the checkout.
- **Per-tool-call cost.** → The engine exits after one stat for a checkout
  without rules. The Lab records the hook's wall time per harness, and
  `PostToolUse` is matched to file and shell tools only, not `.*`.
- **Shell path recovery is heuristic on Codex.** It can miss a path that is
  built from a variable, or match a token that only looks like a path. →
  A miss means the rule arrives at the next edit, which `apply_patch`
  names exactly. A false match only delivers a rule early. Both are
  reported in Codex's route.
- **Vendor drift:** Codex's hook output schema, OpenCode V2's plugin API
  (`result.content`, `context`), Claude's `SessionStart` sources. → Each is
  a Lab contract scenario, so drift is a red nightly, not a silent loss.
- **Antigravity's own glob dialect may be wider than ours.** → The dialect
  is the measured intersection. A pattern outside it is reported and not
  delivered by UZE, even though Antigravity alone would accept it, so the
  four harnesses never disagree on a match.
- **Codex `SessionStart` after compaction is unmeasured.** → Task 1.3
  measures it. If it does not fire, Codex's route reports that always-on
  rules may be lost after compaction until the next session. The approach
  does not change.

## Migration Plan

Nothing exists to migrate. Delivery follows the next `uze setup` (and
`uze workspace`'s automatic setup of detected harnesses). Rollback is
removing the engine's receipt-owned entries, which leaves `.agents/rules/`
to Antigravity alone.
