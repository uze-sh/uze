---
name: rules
description: Writing a project's rules, the instructions that apply only when they are relevant, under `.agents/rules/`. Covers deciding whether something is a rule, `AGENTS.md` or a Skill, choosing the trigger, writing patterns in the supported glob dialect, wording a rule as a convention, deciding when a rule belongs in a shared plugin and moving it there, and proving it with `uze agent rules check`. Use when asked to create, add or write a rule, when an instruction "only applies to src/…" or to one kind of file, when `AGENTS.md` has grown sections that only matter under one path and should be split, or when a rule does not seem to fire.
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

A subfolder of `.agents/rules/` is read only once `.agents/rules.json`
registers it, the way Antigravity reads it and so UZE and the engine too:

```json
{"entries": [{"path": ".agents/rules/team"}]}
```

`path` is relative to the checkout root, not to the manifest. A folder
entry reads the `*.md` directly in it, a file entry that one file;
`include_only` and `exclude` list paths relative to the entry. A manifest
that does not parse registers nothing, and `inherits` reaches Antigravity
alone. Keep the project's own rules flat unless a folder genuinely helps,
and never add or edit an entry naming a plugin's folder: those are UZE's.

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

It validates every file under `.agents/rules/` and every file
`.agents/rules.json` registers, offline, needs no workspace, and exits
non-zero on a manifest that does not parse or naming each file that is not
a deliverable rule and why
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

## A rule several projects share belongs in a plugin

A rule in `.agents/rules/` belongs to this project. When the same
convention holds for every repository of a team or a stack, and someone is
about to copy the file into a second project, it belongs in a plugin
instead: written once in the plugin's `rules/`, and copied into every
project that declares the plugin. Keep a rule in the project when it names
this project's own layout, or when only this project follows it.

A plugin's rules arrive in a folder of the plugin's own,
`.agents/rules/<plugin>@<market>/<rule>.md`, registered in
`.agents/rules.json` by an entry UZE writes. Those files are the
plugin's, judged against the revision `agents.lock` pins. Never edit one
in place: `uze status` reports the edit as drift, and no install or update
overwrites it, so the project silently stops following the plugin for that
rule. Either change the rule in the plugin and update, or take ownership
of the change: move the file out of the plugin's folder into
`.agents/rules/` so it becomes the project's own.

To move a project rule into a plugin:

1. Pick the plugin, or create one with the `uze:author` Skill
   (`uze agent plugin create <name> --market <market> --rules`).
2. Move the file to the plugin's `rules/<rule>.md`, unchanged. Its
   patterns stay relative to the root of the project that receives it.
3. `uze agent plugin check <plugin directory>`, then publish the
   marketplace as the `uze:author` Skill describes.
4. In each project, delete the project's own copy and run
   `uze <plugin>@<market>`: the plugin's copy takes its place in the
   plugin's folder. Commit both changes together, with `agents.lock` and
   `.agents/rules.json`.
5. `uze agent rules check` in the project, and `uze status`, which counts
   the plugin's rules apart from the project's own.

Propose the move and wait for agreement before deleting the project's
file: which projects should follow the convention is the person's call.
