# Decisions made during implementation

Choices the design left open, recorded where they were made so review can
weigh them. One entry each: Context, Decision, Alternative.

## Frontmatter is read line by line, not as YAML

- **Context.** Antigravity's own rules leave patterns unquoted
  (`globs: **/*.ts`). A YAML parser reads a leading `*` as an alias and
  refuses the file, and the `sh` engine reads the same lines with `sed`.
- **Decision.** `uze-core::rules` reads top-level `key: value` lines: the
  value trimmed, an unquoted ` #` comment cut, one pair of quotes removed.
  Only `trigger`, `globs` and `description` are read; any other key, and
  any block beneath it, is ignored. One of those three written as a block
  (`globs:` followed by `- item`, `description: >`, or a value continued on
  an indented line), given twice, or a
  frontmatter line with no `:` makes the file not a rule, with that reason.
- **Alternative.** `noyalib` with `serde_yaml` compatibility, as memory
  facts use. Rejected: it refuses the canonical unquoted form, and the
  engine could not reproduce its reading.

## A duplicated field is refused, not resolved

- **Context.** YAML refuses a repeated key; a line reader would have to
  pick the first or the last, and the engine would have to pick the same.
- **Decision.** A repeated `trigger`, `globs` or `description` makes the
  file not a rule (`DuplicateField`).
- **Alternative.** First-wins or last-wins, which is one more rule both
  readers must agree on for no author benefit.

## `globs` splits on the commas outside `{…}`

- **Context.** `globs: src/**/*.{ts,tsx}` must be reported naming that
  pattern (spec scenario), but a plain comma split would report `src/**/*.{ts`
  and `tsx}`.
- **Decision.** The split ignores commas inside braces. Braces are outside
  the dialect anyway, so this only changes how a refused pattern is named.
- **Alternative.** A plain split, which reports two fragments the author
  never wrote.

## The dialect refuses what task 1.1 has not measured

- **Context.** Task 1.1 (Antigravity's dialect) is not measured yet, and the
  spec says a pattern outside the dialect is not delivered.
- **Decision.** Refused until measured: a leading `/`, whitespace or a
  control character inside a pattern, `{}`, `[]`, `!`, `(`, `)`, `|`, `\`,
  an empty segment (including a trailing `/`), a `.` or `..` segment, and
  `**` that is not a whole segment. Matching: a pattern without `/` matches
  only at the root (`*.md` does not match `docs/a.md`), `*` matches a
  leading dot, `src/ui/**` also matches `src/ui` itself. All of it is rows
  in `tests/_fixtures/rules/glob-dialect.tsv`, so 1.1 widens or corrects the
  table, not prose.
- **Alternative.** Gitignore-style basename matching for slash-less
  patterns. Rejected until measured: it would deliver a rule Antigravity
  might not.

## The checkout root is the nearest `.git`, then the project root

- **Context.** The engine resolves the checkout with
  `git rev-parse --show-toplevel`; Rust has no Git call on this path
  (`status` is Budgeted) and `uze agent rules check` must work offline.
- **Decision.** `rules::checkout_root` walks up to the nearest directory
  holding `.git` (a worktree's `.git` file counts, so a worktree is its own
  root). Outside every repository it falls back to the project root a
  manifest or `AGENTS.md` marks, then to the directory itself.
- **Alternative.** Spawning `git rev-parse`, which costs a process on every
  `status` and answers the same for every checkout layout UZE makes.

## The rules engine's records are a `runtime/` tenant

- **Context.** `prune_projections` sweeps anything directly under
  `runtime/` that is not a named tenant. Sweeping the per-session record
  would make every running session receive every glob rule again.
- **Decision.** `UzeHome::runtime_rules_dir()` (`runtime/rules`) joins
  `runtime_tenants()`; `rules_sessions_dir()` is `runtime/rules/sessions`.
  The seven-day sweep inside it (task 5.2) stays setup's.
- **Alternative.** Only `rules_sessions_dir()` with no tenant, which the
  sweep would delete on the next `doctor`.

## A file that is not a rule makes `status` say the project needs attention

- **Context.** `status` is where a person asks whether the project is
  ready, and a rule that silently does not reach any agent is a project
  that is not.
- **Decision.** The headline becomes `N files in .agents/rules not
  delivered`, after the existing plugin, drift and command checks and
  before portability. No next step is printed: the fix is editing the file
  the section names.
- **Alternative.** Listing the files without changing the headline, which
  leaves "ready" above a reason the project is not.

## D-dialect. Directory patterns open with `**/`; a bare name matches at any depth

**Context.** Task 1.1 measured that `agy` matches a pattern against the
touched file's absolute path. A root-relative `src/ui/**` never fires
there, and a pattern with no `/` matches the name at any depth. The
dialect first implemented here took both the other way, so a valid rule
would have reached three harnesses and not the native one.

**Decision.** The dialect is the measured intersection. A pattern either
opens with `**/` or holds no `/`, in which case it is matched as if it
opened with `**/`. `[…]` sets and ranges and spaces join the dialect. A
`/`-holding pattern that does not open with `**/` is refused as
`Unanchored`, and its reason names the `**/` form to write. Negated sets
(`[!…]`, `[^…]`) stay out, because readers disagree on them. The spec, the
shared table, the `uze:rules` Skill and the docs page moved to the new form.

**Alternative.** Keep root-relative patterns and translate them for
Antigravity. That needs a copy of every rule, which design D1 rules out.
