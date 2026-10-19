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

# Decisions made while implementing project-rules

Choices the design left open, recorded for review. Entries prefixed `E` are
the engine and delivery's (tasks 3, 4, 5.1, 5.2, 5.5).

## E1: The engine's receipts live in the attachment ledger under a reserved owner

- **Context:** every managed artifact is receipt-owned, but the engine
  belongs to no package, and the ledger's orphan sweep (`maintain`) detaches
  any receipt whose package is not in the Store.
- **Decision:** the receipts are ordinary `AttachmentReceipt`s whose
  `package_id` is `state::SETUP_OWNER` (`"setup"`), which no package id can
  equal (every one is `name@market`). The orphan sweep passes them by. Each
  artifact is `IntegrationOwned { kind: "rules-engine" }`, so no generic
  inspection, and no hook-wrapper prune, ever mistakes it for something else.
- **Alternative:** a record of its own under `state/` (a new shaped
  document and ladder for two or three entries per harness), or no receipt
  at all, identifying UZE's entries by the engine path alone.

## E2: The engine is one byte-identical file; the sessions directory is its argument

- **Context:** ADR-040 D8 forbids the delivered artifact naming the
  packager, and `$UZE_HOME` normally contains `.uze`.
- **Decision:** the `sh` template is a constant; the entry passes
  `runtime/rules/sessions` as `$1`. The engine is written per harness at
  `runtime/attachments/<harness>/rules/engine`, beside that harness's hook
  wrapper, so one harness's removal never takes another's engine.
- **Alternative:** one shared engine for all three harnesses, which needs a
  reference count across integrations to know when it may go.

## E3: Path recovery trusts patch headers, and requires every other candidate to exist

- **Context:** "every string field of `tool_input`" includes `description`,
  `old_string` and file contents; shell tokens include `1,40p`.
- **Decision:** `*** Add/Update/Delete File:` and `*** Move to:` lines are
  taken as they are (a deleted or new file need not exist). A one-line
  string field and each shell token count only when the checkout-relative
  path exists. Multi-line non-shell fields (file contents) are ignored, and
  a patch's body lines are never read for paths.
- **Alternative:** taking every candidate unchecked, which turns a `**`
  rule into "every tool call" and a description into a match.

## E4: The `model_decision` index is a sentence and one `<rule>` line per rule

- **Context:** the spec asks for one line naming the path and the
  description; D5 asks every rule to arrive wrapped so `doctor` can
  recognise the engine's output.
- **Decision:** one sentence ("read a rule's file when its description fits
  the task") then `<rule path="…" trigger="model_decision">description</rule>`
  per rule, after the always-on bodies.
- **Alternative:** an unwrapped bullet list, which `doctor` could not tell
  from anything else in a transcript.

## E5: OpenCode's plugin file is `uze-rules.ts`, refused over a foreign file

- **Context:** the global plugin directory is shared with the operator's
  own plugins, and a bare `rules.ts` is a name an operator may well use.
- **Decision:** the file is `uze-rules.ts` (its content names no packager);
  a file at that path that neither carries the generated header nor equals
  a recorded earlier version is refused with `ManagedEntryConflict`.
- **Alternative:** `rules.ts`, as the task names the template.

## E6: OpenCode's `context` asks with `source: "context"`, which resets nothing

- **Context:** `context` runs before every request, so it cannot mean a
  compaction; truncating the session record there would re-deliver every
  glob rule on every request.
- **Decision:** only a `SessionStart` whose source is `compact` or `clear`
  truncates the record. On OpenCode a glob rule therefore arrives once per
  session even across a compaction; the spec allows, not requires, it to
  arrive again.

## E7: A package's `rules/` is withheld by every harness, with its own reason

- **Context:** 4.6. Antigravity's generated plugin already stages only
  `skills/`, `mcp_config.json`, `hooks.json` and `agents/`; Claude Code's
  envelope and both explicit marketplace mirrors copied the whole tree.
- **Decision:** `rules/` is excluded from every mirror, and
  `IntegrationPort::withheld_paths` now returns `WithheldPath { path,
  reason }` so `inspect` says "rules not delivered: rules are delivered per
  project…" instead of the generic "no portable equivalent". All four
  integrations report it.
- **Alternative:** keeping `Vec<String>` and a fixed reason in the CLI,
  which would have said something false about rules.

## E8: Doctor reports the engine as machine concerns

- **Context:** 5.5 asks for a missing `jq` and drifted entries, per harness.
- **Decision:** both are `MachineConcern { subject: "rules" }` entries,
  appended to the ones `doctor` already renders; `jq` is reported only when
  some harness holds an engine receipt.
- **Alternative:** a new `DoctorReport` field and its own rendering.

## E9: OpenCode's plugin names the checkout by `ctx.location.directory`

- **Context:** 6.1, measured on OpenCode 2.0.32
  (`experiments/opencode/rules-directory`). Neither a `context` nor an
  `execute.after` event carries a directory; `ctx.directory` is absent;
  `ctx.worktree` is the worktree API (`list`, `create`, …), not a path, so
  the first template handed the engine an object as `cwd` and no rule ever
  reached an OpenCode session. `process.cwd()` is the project only in a
  standalone launch: the shared service runs in the home directory
  whatever it serves. OpenCode sets a plugin up once per directory it
  serves, and that instance's `ctx.location.directory` is the session's
  directory in both topologies.
- **Decision:** the plugin reads `ctx.location.directory` and nothing
  else; without one it asks with an empty `cwd`, which the engine answers
  with nothing.
- **Alternative:** falling back to `process.cwd()`, which under the shared
  service is the home directory, and would hand a session the rules of
  whatever repository the home directory is.
