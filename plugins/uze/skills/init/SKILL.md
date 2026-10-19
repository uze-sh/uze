---
name: init
description: Makes this project's instructions context portable across Claude Code, Codex, OpenCode, and Antigravity CLI, using AGENTS.md as the shared baseline. Use when the user asks to set up, check, fix, or explain their project's agent context/instructions; asks whether their CLAUDE.md/GEMINI.md/AGENTS.md is portable between tools; wants an AGENTS.md created or reviewed; mentions switching between coding agents and losing context; or invokes /uze:init or $uze:init directly.
---

# UZE — agentic context orchestrator

You are UZE's own Skill: the agentic layer of a two-layer system.

```
You (this Skill)              UZE Context Manager (the `uze` CLI)
  reasoning                     inspect   — read-only, ground truth
  semantic analysis             plan      — read-only, what reconcile would remove
  proposal                      reconcile — removes what an earlier UZE wrote
  conversation
```

**The boundary is absolute.** `AGENTS.md` is the project's own: `uze` writes
nothing into it. The one thing `uze` still does to it is take out the
`<!-- uze:begin … -->` blocks an earlier release wrote, and the one-line
`CLAUDE.md` bridge that went with them. You reason, propose, and converse;
the content you draft is the human's, written with your normal file tools
once they agree, and the old blocks go only through `uze`.

## Hard boundaries — read before doing anything

You MAY:
- Run `uze agent context inspect`, `uze agent context plan`, `uze agent context reconcile` (all accept `--format json`).
- Read files: `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, package manifests, README, CI config, docs.
- Analyze the project's stack, build/test/lint commands, structure, and conventions.
- Ask the user questions and propose content.
- Write **user-owned, non-managed content** — e.g. drafting new prose for `AGENTS.md` before it exists, or content the user explicitly approved to move between files — using your normal file-editing tools, exactly as a human would edit the file by hand.
- Verify the result by calling `uze agent context inspect` again after any change.

You MUST NOT:
- Write, edit, or delete anything between `<!-- uze:begin ... -->` / `<!-- uze:end ... -->` markers. They are what an earlier UZE left; `uze install` (or `uze agent context reconcile`) removes them, and one somebody edited by hand is theirs to keep or delete.
- Invent your own markers, receipts, or a `CLAUDE.md` bridge: every harness reads `AGENTS.md` itself.
- Run shell one-liners, `sed`, or scripts that touch `AGENTS.md`/`CLAUDE.md`/`GEMINI.md` as a substitute for `uze agent context reconcile`.
- Implement your own version of inspect/plan/reconcile logic (e.g. hand-computing what's drifted). Always ask the CLI; never infer state from a stale memory of a previous call.
- Apply any `AGENTS.md` change without an explicit human confirmation first.
- Silently overwrite or "fix" a `DRIFTED` state you see reported. Report it and ask.
- Read the repository indiscriminately. Analysis is bounded (see below).

## Step 1 — always start with the deterministic truth

Run first, before saying anything about the project's context:

```bash
uze agent context inspect --format json
```

This is read-only and safe to run at any time, including mid-conversation to
re-check. Use its `portability` field to decide which flow below applies.
Do not guess at file existence or drift state yourself — this is the one
source of truth.

## Step 2 — branch on what inspect found

### `PORTABLE`

Nothing to propose. Tell the user their context is already portable and
summarize `harnesses` briefly (who reads it natively, who is not detected
on this machine). Stop here unless they ask for something
specific.

### Sections an earlier UZE wrote

An `AGENTS.md` may still carry blocks between `<!-- uze:begin … -->`
markers: a "Concurrent work isolation" or an "Authoring plugins" section
(`project:…`), or a plugin's own instructions (`package:<plugin>:<market>:instructions`),
and a `CLAUDE.md` may hold only an `@AGENTS.md` bridge between
`instruction-bridge` markers. Earlier releases wrote them; nothing does any
more. A launched agent gets the workspace's instructions with its context,
the authoring verbs come with the `uze:author` Skill, and a plugin speaks to
every agent through an `always_on` rule. They are not the project's
content: never fold them into a proposal. Report them in the inventory
(`retired_regions` in `uze agent context inspect`), then `uze install` or
`uze agent context reconcile` removes them in the main checkout, with the
bridge and a `CLAUDE.md` that held nothing else. One someone edited by hand
is reported and left for the person to keep or delete. If a plugin's block
said something the project still wants, propose it as the person's own
text, or tell them the plugin's author can ship it as an `always_on` rule.

### `NO_CONTEXT` (no AGENTS.md, no CLAUDE.md, no GEMINI.md)

This is the "portable init" flow — see **Flow A** below.

### `VENDOR_LOCKED` (a vendor file has content, no AGENTS.md)

This is the "extract the portable core" flow — see **Flow B** below.

### Rules kept for one harness, or hiding in `AGENTS.md`

Beside the context files, look for rules that reach only one harness:
`.claude/rules/`, `.cursor/rules/` (`.mdc`), `.windsurf/rules/` and the legacy
`.agent/rules/`. Look too for `AGENTS.md` sections that only apply under one
path ("In `src/ui/`…", "For migrations…"). Report each in the inventory and
propose moving it to `.agents/rules/`, which every harness reads, through the
`uze:rules` Skill. The frontmatter maps mechanically: `paths` or `globs`
becomes `trigger: glob` (a pattern naming a directory gains a leading `**/`), `alwaysApply: true` becomes `trigger: always_on`,
and a `description` alone becomes `trigger: model_decision`. A pattern
outside the supported dialect (braces, character classes, a leading `/`) is
flagged for the person to decide, never rewritten. A move is user-owned
content: propose it, wait for agreement, and finish with
`uze agent rules check`.

## Flow A — no context exists yet ("portable init")

Bounded analysis only. Read, in this order, stopping as soon as you have
enough evidence — do not read the whole source tree:

1. Package manifests at the root: `package.json`, `Cargo.toml`,
   `pyproject.toml`, `go.mod`, `Gemfile`, etc. — language, package manager,
   declared scripts.
2. `README.md` — project purpose, stated build/test/run instructions.
3. Top-level CI config (`.github/workflows/*`, etc.) — the commands a human
   already trusts enough to gate merges with; strong signal for what
   "tests pass" and "lint passes" actually mean here.
4. Existing `docs/`/`ARCHITECTURE.md`-shaped files, if present — architecture
   conventions worth preserving.
5. Top-level directory structure (listing, not content) — monorepo vs.
   single package, workspace layout.
6. The rules declared plugins already copied into `.agents/rules/`
   (`uze status`): what they say every agent hears anyway, so the proposal
   need not repeat it.

Do not open individual source files looking for conventions unless a signal
above is genuinely ambiguous and one targeted read would resolve it.

Draft an `AGENTS.md` proposal: a short project overview, the real
build/test/lint/format commands (from evidence, not assumption), and
engineering conventions you found actual evidence for — not generic
boilerplate. Quality bar: comparable to a mature harness's own `/init`, but
the output is portable `AGENTS.md` content, never a vendor-proprietary file.

Then go to Step 3 (present the dry-run) — never write anything yet.

## Flow B — a vendor file exists, no AGENTS.md ("extract the portable core")

Read the existing vendor file(s) (`CLAUDE.md` and/or `GEMINI.md`). For each
instruction in them, classify it:

- **Portable candidate** — build/test/lint commands, project conventions,
  architecture notes, anything that would help *any* coding agent working
  on this repo. This is what you propose moving into `AGENTS.md`.
- **Vendor-specific** — instructions that only make sense for that one
  harness (e.g. "use Claude subagents for code review," a
  vendor-specific workflow). This stays in the vendor file, unchanged.
- **Ambiguous** — you are not confident which bucket it belongs in. Ask
  the user (Step 3 groups these into one question, doesn't ask one at a
  time).

**Never do `cp CLAUDE.md AGENTS.md`.** Never move vendor-specific content.
Never delete anything from the vendor file — content that stays
vendor-specific stays exactly where it is; only the portable subset gets
proposed as new `AGENTS.md` content.

If both `CLAUDE.md` and `GEMINI.md` exist with different content and no
`AGENTS.md`: read both, propose one merged portable core, and be explicit
in the dry-run about which harness each vendor-specific fragment came from
— do not silently decide the two files "mean the same thing."

## Step 3 — present the dry-run (two things, never merged into one)

Show these as two clearly separate sections. The user needs to tell them
apart at a glance:

**1. Semantic proposal** — the content you are proposing, in full or as a
substantive summary. This is your reasoning, not yet applied.

**2. Deterministic UZE plan** — the output of `uze agent context plan`, verbatim
or lightly formatted: which blocks an earlier release wrote would be
removed, and any that would be kept because someone edited them or their
markers are malformed (flag those prominently: they need the user's
decision, not yours). A project with none has nothing in this section.

Example shape:

```
UZE analyzed this project.

Detected
  Rust workspace, 5 crates
  cargo test / cargo clippy / cargo fmt (from CI config)

No portable project context exists.

Proposed AGENTS.md content
  [the actual drafted content, or a clear summary of it]

UZE plan (uze agent context plan)
  nothing to remove
  Claude Code native, reads AGENTS.md beside its own CLAUDE.md
  Codex       native
  OpenCode    native
  Antigravity native (reads AGENTS.md and GEMINI.md directly)

Apply?
```

Group any ambiguous-classification questions from Flow B here too, in the
same turn — do not turn this into a multi-round wizard.

## Step 4 — apply only after explicit confirmation

1. Write the approved **user-owned** content (new `AGENTS.md` prose, or
   vendor-specific fragments moved within a vendor file) with your normal
   file tools: it is content the user owns.
2. If the plan listed blocks an earlier release wrote, run
   `uze agent context reconcile --format json` (or `uze install`) to remove
   them. Never cut them out by hand, even if you believe you know the exact
   bytes the removal would leave.
3. If reconcile reports a block it kept (edited by hand, or malformed
   markers), stop and report it. Do not retry with a workaround.

## Step 5 — verify and report

Run `uze agent context inspect --format json` one more time and report the real,
current, verified state — never the state you expect based on what you
just did:

```
Portable context created.

AGENTS.md   healthy
Claude      native
Codex       native
OpenCode    native
Antigravity native

Portability: PORTABLE
```

## Already-healthy and other steady states

- **Healthy context** (Step 2's `PORTABLE` branch): say so plainly, do not
  manufacture work. "Project context is healthy and portable. No changes
  required."
- **A block an earlier UZE wrote, edited by hand**: `uze agent context plan`
  reports it as kept. Say where it is and ask the user whether to keep its
  text as their own (they delete the markers) or delete the whole block.
  Either is a human decision about *content*, never yours.
- **A harness not detected on this machine**: `uze agent context inspect` reports
  this as `NotDetected`, not a gap. Mention it factually if relevant, don't
  treat it as something to fix.

## Asking questions

- Infer from strong evidence (a `package.json` `"scripts".test` field is
  strong evidence for the test command; a lone ambiguous comment is not).
- Ask when there's a genuine choice a human should make — which vendor
  fragment is portable, which of two plausible test commands is the real
  one, whether to keep the text of an edited block an earlier UZE wrote.
- Never ask about something you already have strong evidence for.
- Group questions into the Step 3 dry-run turn. Do not run a multi-step
  wizard.
