## Why

A project's instructions reach an agent all at once: `AGENTS.md` is loaded
whole into every session, whether the agent is touching `src/ui/` or a
migration script. Two harnesses already solve this natively with
path-scoped rules (Claude Code's `.claude/rules/` with `paths`, Antigravity's
`.agents/rules/` with `trigger`/`globs`), each in its own directory and its
own format; Codex and OpenCode have no equivalent, and their open requests
for one (openai/codex#34002, anomalyco/opencode#52873, PRs #10090 and #18903
unmerged) show no sign of landing. A project that wants rules today must
write them twice and still leave two harnesses without them.

A proof of concept on 2026-10-06 showed the gap can be closed without a
second copy: one set of rules in Antigravity's format, read natively by
Antigravity and injected by a generated hook into Claude Code, Codex and
OpenCode, changed the code all four agents wrote, while the same task
without the hook ignored every rule.

## What Changes

- **New canonical capability: the project Rule.** A Markdown file under
  `.agents/rules/` in the checkout, with Antigravity's frontmatter as the
  canonical form: `trigger` (`always_on` | `glob` | `model_decision` |
  `manual`), `globs` (for `glob`), `description` (for `model_decision`).
  The project authors it and versions it; UZE never copies, translates or
  rewrites it.
- **Antigravity is Native** and needs nothing from UZE: it reads
  `.agents/rules/` itself.
- **Claude Code, Codex and OpenCode are Adapted** through one rules engine
  UZE generates once per harness at machine setup:
  - Claude Code and Codex: a command hook on `SessionStart` (always-on
    rules and the index of on-demand rules) and `PostToolUse` (a `glob`
    rule whose pattern matches a path the tool touched), answering with
    the harness's own `additionalContext`.
  - OpenCode: a generated V2 plugin using the session `context` hook
    (always-on and index, every request) and `execute.after` (the matched
    rule appended to the tool result the model reads).
- **The engine reads the session's own checkout** at run time: no index is
  compiled, so a branch or a worktree with different rules is answered
  with its own. The only state is a per-session record of which rules were
  already delivered, in the generated tier.
- **Claude Code is Adapted by decision, not by limitation**: its native
  route needs a `.claude/rules/` in the project (measured: rules with
  `paths` reached through `--add-dir` never fire), which this change refuses
  in order to keep one canonical directory. The exit condition is recorded.
- **`uze status` reports the project's rules**: how many of each trigger,
  any file that does not parse or uses a pattern outside the supported
  dialect, and the route each harness delivers them by.
- **Authoring is covered, the way diagrams are**:
  - `uze agent rules check [<path>]` validates a project's rules offline,
    with no workspace, and exits non-zero naming each file that is not a
    deliverable rule and why. It shares its validator with `uze status`.
  - A new `uze:rules` Skill in the official plugin guides an agent through
    writing a rule:
    - whether it belongs in a rule at all, or in `AGENTS.md` or a Skill;
    - which trigger to use;
    - patterns in the supported dialect;
    - wording a rule as a convention;
    - checking it.
  - `uze:init` points to it when a project carries rules in a
    harness-specific directory (`.claude/rules/`, `.cursor/rules/`,
    `.windsurf/rules/`, `.agent/rules/`) or path-specific sections in
    `AGENTS.md`, and proposes moving them to `.agents/rules/`.
- **Out of scope**: rules shipped inside plugins (Antigravity installs
  plugins machine-wide, which would fire a project's rule in every project)
  and personal machine-wide rules. Both are named as follow-ups.

## Capabilities

### New Capabilities
- `project-rules`: the canonical Rule (location, frontmatter, triggers, glob
  dialect), its delivery route per harness, the generated rules engine and
  its contract (what it reads, what it answers, deduplication, failure
  behavior), and how it is reported.

### Modified Capabilities
<!-- None: machine setup, the persisted-state tiers and the portable hook
     contract keep their requirements; the engine is a new artifact setup
     delivers, the deduplication record sits in the existing generated tier,
     and the engine is not a portable hook (it answers with context, which
     the exit-code ABI has no channel for). -->

## Impact

- `uze-core`: the Rule model (frontmatter parser, trigger set, glob
  dialect and its validation), discovery in a checkout, and the
  deduplication directory named in `UzeHome`. Names no harness.
- `uze-integrations`: per-harness rules route (Native for Antigravity,
  generated engine for the other three), the `sh` engine template and the
  OpenCode V2 plugin, receipt-owned entries in `~/.claude/settings.json`,
  `~/.codex/hooks.json` and OpenCode's global plugin directory.
- `uze-application` / CLI:
  - `uze setup` delivers the engine;
  - `uze status` reports rules;
  - `uze doctor` reports a missing engine dependency (`jq`);
  - `uze agent rules check` is a new agent-audience leaf command, classified
    in `command_performance.rs`.
- `plugins/uze`: a new `rules` Skill, and `init` extended to detect and
  migrate harness-specific rules. The `uze agent` region UZE projects into
  `AGENTS.md` names the new check.
- Conformance Lab: one contract scenario per harness proving a `glob` rule
  changes what the agent writes, and the controls proving it does not leak.
- Docs: the rules page for authors; ADR at archive time.
- No new dependency.
