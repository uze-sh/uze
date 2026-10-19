---
name: rules
description: Writing a project's rules, the instructions that apply only when they are relevant, under `.agents/rules/`. Covers deciding whether something is a rule, `AGENTS.md` or a Skill, choosing the trigger, writing patterns in the supported glob dialect, wording a rule as a convention, and proving it with `uze agent rules check`. Use when asked to create, add or write a rule, when an instruction "only applies to src/…" or to one kind of file, when `AGENTS.md` has grown sections that only matter under one path and should be split, or when a rule does not seem to fire.
---

# UZE: a project's rules, written once

A rule is a Markdown file directly under `.agents/rules/` in the checkout,
versioned with the project. Its frontmatter says when it applies:

```markdown
---
trigger: glob
globs: "**/src/ui/**/*.rs"
---

Colours come from the theme: name a `Token`, never an RGB value.
```

Antigravity reads `.agents/rules/` itself. Claude Code, Codex and OpenCode
receive the same files through the rules engine `uze setup` installs. Every
harness reads the one directory and the one format, so a rule is written
once.

## Is it a rule at all?

Decide this first. Most instructions are not rules.

- **Always true, whatever the agent is touching** → `AGENTS.md`. It is
  loaded whole, every session, by every harness, and nothing else is needed.
- **True only under a path, or for one kind of task** → a rule. That is the
  one thing a rule does that `AGENTS.md` cannot: arrive only when it is
  relevant.
- **A procedure with steps** ("to cut a release, run …") → a Skill. A rule
  is a convention the code should follow, never a script to execute.

When `AGENTS.md` carries a section that only matters under `src/ui/` or
`migrations/`, that section is a rule waiting to be split out.

## Choose the trigger

| trigger | arrives | use it for |
|---|---|---|
| `glob` | when the agent reads, edits or writes a file matching `globs` | conventions of a directory or a file type: the common case |
| `model_decision` | as one line of an index; the agent reads the body when the task calls for it | conventions a path cannot predict, such as "when adding a migration" |
| `always_on` | at session start, every session | sparingly: it costs every session, and most such text belongs in `AGENTS.md` |
| `manual` | only when someone points the agent at the file | reference material |

`glob` needs `globs`. `model_decision` needs a `description`, written as
*when to read it* ("Read before adding or changing a database migration"),
never as a summary of the body.

## Patterns: the supported dialect

`globs` is one pattern or a comma-separated list. Quote the value when it
opens with `*`, as YAML would otherwise read it as an alias. Each pattern is
one of two shapes:

- **A file name, with no `/`**, matched at any depth: `*.sql`, `Makefile`,
  `*.test.ts`.
- **A path opening with `**/`**: `**/src/ui/**`, `**/migrations/*.sql`,
  `**/*.rs`. Antigravity matches a pattern against the file's absolute path,
  so a pattern naming a directory has to open with `**/` to fire there;
  `src/ui/**` never would, and is reported for that reason.

Inside a pattern: `**` is any number of directories (none included), `*`
any characters within one name, `?` one character, `[ab]` or `[0-9]` one
character of a set or range, and everything else, spaces included, is
literal. Braces (`{a,b}`), negated sets (`[!a]`), a leading `/`, `./` or
`../` are outside the dialect. They are reported by the check and not
delivered, even where one harness alone would accept them, so that the four
harnesses never disagree on what matched. Use two patterns instead of
braces.

## Write it as a convention

- **One concern per file**, short. A rule that covers three things
  arrives three times as often as each of them is relevant.
- **State the project's convention**, not an order about the agent's
  reply: "Handlers return `Result<_, ApiError>`", not "You MUST always
  answer with…". A rule worded as an imperative about the reply reads like
  a prompt injection, and a careful model will treat it as one.
- Leave out where the rule lives: the engine and Antigravity both tell the
  agent which file a rule came from.

## Check it

```bash
uze agent rules check
```

It validates every file under `.agents/rules/` offline, needs no workspace,
and exits non-zero naming each file that is not a deliverable rule and why
(no frontmatter, an unknown trigger, a missing `globs` or `description`, a
pattern outside the dialect). On success it lists each rule with its
trigger and patterns. Run it after every change to a rule, and before you
say a rule is done. `uze status` reports the same findings for a person.

## Moving rules from a harness-specific directory

Rules kept in `.claude/rules/`, `.cursor/rules/` (`.mdc`), `.windsurf/rules/`
or the legacy `.agent/rules/` reach only one harness. Move each into
`.agents/rules/` with the mechanical mapping:

- `paths` or `globs` → `trigger: glob`, each pattern that names a directory
  prefixed with `**/` (`src/**` becomes `**/src/**`);
- `alwaysApply: true` → `trigger: always_on`;
- a `description` alone → `trigger: model_decision`.

A pattern outside the dialect is flagged for the person to decide, never
silently rewritten. Propose the move, wait for agreement, then run the
check.
