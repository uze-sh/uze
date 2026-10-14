## Status

Exploration. The idea is being validated before the specs, design and
tasks are settled. Everything about project instruction files lives here:
the cross-harness resolution below, and Claude's native `AGENTS.md`
reading that replaces PR #154 (carried from `refresh-harness-deliveries`
as `specs/project-context-delivery`, design C1 and tasks section 2, to be
revised once an option is chosen). Until then Claude keeps the `CLAUDE.md`
bridge it has today.

## Why

UZE promises one project context that every harness reads. With Claude
reading `AGENTS.md` natively, a project that has an `AGENTS.md` reaches
all four harnesses. The opposite case does not: a repository cloned with
only a `CLAUDE.md` (most of them written by Claude's own `/init`) gives
Codex, OpenCode and Antigravity nothing, until the person migrates it.
The open decision is whether UZE should resolve vendor instruction files
across harnesses, or require `AGENTS.md` and help the person get there.

## Options

- **A. `AGENTS.md` only.** UZE writes and reads only `AGENTS.md`. A
  project with only vendor files is reported (`locked to one vendor`) and
  `/uze:init` migrates it. Simplest model, one source of truth; a fresh
  clone is without context in three harnesses until someone migrates.
- **B. `AGENTS.md` canonical, plus read fallback.** UZE still writes only
  `AGENTS.md`, but configures each harness, on the machine, to read a
  vendor file where a directory has no `AGENTS.md`. Reported as a
  fallback (degraded), with `/uze:init` still offered. Nothing is written
  into the project.
- **C. Full cross support.** UZE keeps every vendor file in sync with
  `AGENTS.md`. Two sources of truth: what the `CLAUDE.md` bridge was, and
  what `refresh-harness-deliveries` removes. Listed to be rejected
  explicitly.

Independent of the option: how `/uze:init` migrates (vendor file reduced
to an `@AGENTS.md` import plus vendor-only lines, both files merged with
every disagreement asked, `CLAUDE.local.md` never merged, nested files to
nested `AGENTS.md`), and whether the import line is kept or the file is
deleted.

## Measured so far

- **Codex 0.162.1** (source, `codex-rs/core/src/agents_md.rs`,
  `config/defaults.toml`): per directory it reads `AGENTS.override.md`,
  then `AGENTS.md`, then `project_doc_fallback_filenames` (default `[]`).
  `project_doc_fallback_filenames = ["CLAUDE.md"]` in `~/.codex/config.toml`
  is a native fallback that only acts where `AGENTS.md` is missing.
  `AGENTS.override.md` replaces `AGENTS.md` in its directory, so it is a
  shadowing file Codex's integration does not declare today.
- **Antigravity 1.3.4** (docs shipped in the binary, `rules.md`,
  `json_configs.md`; `strings` of the binary): reads `GEMINI.md` and
  `AGENTS.md` from every directory up to the repository root and
  deduplicates; the binary holds no `CLAUDE.md` literal, so there is no
  native fallback. Candidates, not measured: a global or project
  `rules.json` entry naming `CLAUDE.md` (additive, not a fallback), a
  `contextFileName` setting (a JSON tag exists in the binary), the
  `@[label](path)` include syntax inside `AGENTS.md`/`GEMINI.md`.
- **OpenCode V2**: V1 read `CLAUDE.md` as a native fallback; V2 is not
  measured. `instructions` in `opencode.json` is additive, so it would
  also inject a `CLAUDE.md` that only imports `AGENTS.md`.
- **Claude Code 2.1.296** (vendor docs, not yet Lab-measured on this
  version; #154 measured 2.1.285): reads `AGENTS.md` natively since
  2.1.281 unless a root `CLAUDE.md` shadows it; reads no `GEMINI.md`.

## To measure before choosing

- Antigravity: each candidate above, with a nonce per file and the
  verbatim requests (`conformance/experiments/antigravity/context-files.py`,
  `--discovery`), on 1.3.4: `CLAUDE.md` only; `GEMINI.md` only; `AGENTS.md`
  with `GEMINI.md` (additive or one wins); `rules.json` global and project;
  `contextFileName` / `context.fileName`; include from `AGENTS.md`.
- OpenCode V2: `CLAUDE.md` only, and `AGENTS.md` with `CLAUDE.md`.
- Codex: the fallback with `CLAUDE.md` only, and that it stays silent when
  `AGENTS.md` exists; `AGENTS.override.md` shadowing.
- What a vendor-only line costs another harness: a `CLAUDE.md` `@path`
  import read by Codex as text.

## Questions for the decision

- Is a fresh clone with only `CLAUDE.md` working in every harness worth a
  machine-wide setting UZE owns (B), or is the nudge to `/uze:init` (A)
  enough?
- Under B, what does UZE do when a plugin's context contribution would
  create an `AGENTS.md` in a project that only has `CLAUDE.md`, which cuts
  the fallback off for Codex?
- Does `status` describe this to a person in effect terms ("read by every
  harness", "Claude Code reads CLAUDE.md instead") rather than
  `portable`?

## Capabilities

- `project-context-delivery` (new): drafted from #154's outcome; revised
  with the option.

## Impact

To be decided with the option.
