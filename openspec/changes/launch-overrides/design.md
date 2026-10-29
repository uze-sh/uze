## Context

See proposal.md for why. What the code does today, and what constrains
the approach:

- **The new agent's popup composes no argv.** An `AgentOption`'s command is
  `[launch]`: the harness's shim when setup placed one, its bare binary
  otherwise (`crates/uze-application/src/application/services.rs`,
  `src/ui/orchestrator.rs`). The tab is created with `UZE_AGENT` and
  `UZE_AGENT_KEY` in its launch environment, which the terminal runtime
  caps at 16 entries and 4096 bytes (`uze-terminal/src/launch.rs`).
- **The shim turns continuity off for any argv.** `src/shim.rs` plans
  continuity and the agent context only when `original_args.is_empty()`
  and the launch owns its identity; that is how `The operator's own
  invocation is never rewritten` is kept. Its argv is: session args, the
  contribution's args, the trailing first turn, `memory_off`'s args, then
  the caller's argv. Its environment is the inherited one, then
  `UZE_SHIM_*`, then `extra_env`. Under `UZE_BYPASS` it is a plain
  passthrough.
- **What UZE composes, per harness** (measured on each integration's
  `HarnessFact::measured_on`):

  | harness | flags UZE composes | environment UZE sets or reads |
  |---|---|---|
  | Claude Code | `--session-id`, `--resume`, `--add-dir` (runtime, context), `--settings` (reporter), `-- <turn>` | sets `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD`; reads `CLAUDE_CONFIG_DIR` |
  | Codex | `resume <id>` subcommand, `-c agents=`, `-c hooks.SessionStart=`, `-c hooks.state=`, `-c notify=` (when the operator's config leaves it free), `-c features.memories=false` (when `[memory] harness = "off"`), positional turn | reads `CODEX_HOME`, `HOME` |
  | OpenCode | `--session`, `--prompt <turn>` | reads `XDG_CONFIG_HOME`, `XDG_DATA_HOME` |
  | Antigravity CLI | `--conversation`, `--add-dir` (context), `-i <turn>` | none |

  Every harness also sees `UZE_AGENT`, `UZE_AGENT_KEY`, `UZE_PANE`,
  `UZE_PANE_UNIT`, `UZE_SHIM_NAME`, `UZE_SHIM_PID`, `PATH` with the shims
  first and, with telemetry, `TRACEPARENT`/`TRACESTATE`.
- `owner_of` (`uze-workspace/src/conversation.rs`) loads the agent store to
  verify a claim, and keeps only root, id and isolation; `LaunchPlan` has no
  room for anything else.
- `AgentStore` is saved by `write_atomic` with the umask's mode, so
  `agents.json` is not private. `prompt_history` is the precedent for a
  per-project, owner-only, bounded history under `state/projects/<id>/`.
- Nothing today describes a harness's CLI flags. The closest precedent is
  `HarnessFact` (a fact with `measured_on` and `proven_by`).

## Goals / Non-Goals

**Goals:**
- Overrides reach the harness without turning off continuity, the agent
  context, the reporter or memory-off.
- One place, the integration, says which flags and variables are UZE's.
- A set used before is one gesture away.

**Non-Goals:**
- Project-declared defaults (a `workspace:` key in `agents.yaml` naming
  overrides every agent gets). It is a declaration with an approval story
  of its own; a later change.
- Overrides for a harness started by hand in a shell: that invocation is
  already the operator's, untouched.
- Discovering flags by parsing `--help` at runtime.
- Editing the overrides of a running agent. A change takes a relaunch,
  which the agent's menu can offer later.
- Masking values on screen.

## Decisions

### 1. Overrides travel in a private record the shim reads, not in argv

The popup records the agent's overrides before its tab is created; the
shim, on an owned launch, gets them back from `continuity::plan_with` as
`LaunchPlan.overrides` (args, env, notes), filled from the same store load
that verifies the claim. The pane's command stays `[launch]`.

When `launch` is the bare binary (no shim placed), the client appends the
override flags to the argv and the variables to the launch environment
itself: there is no continuity to lose there. Which of the two applies
follows the command the tab runs, read when the client attached, never a
fresh look for the launcher at placement: a launcher set up in between
would otherwise have the overrides recorded for a launch that never reads
them. Under `UZE_BYPASS` the shim
launches without them and says so once. A claim `owner_of` refuses
launches without them and says so.

Alternatives:
- *Append to the pane's argv always.* Rejected: the empty-argv rule would
  read every override as the operator taking the launch over and drop
  resume, context and reporter.
- *Carry them in the tab's launch environment.* Rejected: the cap is sized
  for identity, and a respawn would need the terminal runtime to persist
  them too.

### 2. One global owner-only document, not a field on `Agent`

`UzeHome::launch_overrides_path()` → `state/launch-overrides.json`,
beside its siblings `provisioning.json` and `hosts.json`, and named after `theme-overrides.json` at the root, mode
`0600`. One `Shaped` document (shape 1):

```
{ agents: { <project id>: { <agent id>: { <integration>: [tokens] } } },
  recent: { <integration>: [ { tokens, withheld: n, project, last_used_unix } ] } }
```

Read-modify-write under a lock beside it. Tier: *record*, since losing it
costs the operator what only they typed.

Global rather than per project because most overrides are habits of the
operator (`--model`, `--permission-mode`, `--verbose`), not of a project;
a per-project history would have them typed again in every repository.
The few that are a project's (`--add-dir ../lib`, `DATABASE_URL=…`) are
handled by order, not by scope: each set remembers the project it was
last used in, and the list shows this project's sets first, then the
others.

Rejected: a field on `Agent`. `agents.json` is world-readable, and a shape
bump on `AgentStore` makes it unreadable to an older installed binary,
whose shim would then find no owner and drop every agent's continuity in
the project. Rejected: one file per project
(`state/projects/<id>/…`), for the reason above.

An agent's overrides are kept per agent *and* harness, so `Continue in`
launches the other harness with none and switching back finds the first
harness's set again. An agent's entry goes when the agent's record does.

### 3. Final argv and environment

```
<session args> <contribution args> <override flags> <trailing turn> <memory_off args> <caller argv (empty)>   turn behind a flag or `--`
<session args> <contribution args> <trailing turn> <override flags> <memory_off args> <caller argv (empty)>   bare positional turn
```

The session args lead (Codex's `resume <id>` must come first, so every
Codex catalog flag must also be accepted by `codex resume`); the trailing
turn stays where its harness reads it. A turn carried by a flag or after
`--` (Claude's `-- <turn>` ends option parsing, OpenCode's `--prompt`,
Antigravity's `-i`) has overrides precede it; a bare positional turn
(Codex) has them follow it, since a variadic flag before it
(`-i, --image <FILE>...`) would take the turn as one of its values. Variables are set before
`UZE_SHIM_*` and `extra_env`. That ordering protects only what UZE
*sets*; variables UZE only *reads* are protected by refusal alone.

Flag values are visible to every user in `/proc/<pid>/cmdline`; the step's
detail line says that secrets belong in variables.

### 4. The catalog is data the integration declares

`uze-core` gains plain types and two `IntegrationPort` methods with empty
defaults:

- `flag_catalog() -> FlagCatalog`: `spellings` (`--model`, `-m`),
  `value` (`None` / `Free(hint)` / `OneOf(&[..])` / `Optional(hint)`, a
  value only ever attached — `--debug=mcp`, `-dmcp` — and offered as a
  switch), `repeatable`,
  `summary`, and `owner`:
  - `Operator`: offered, passed.
  - `Weakens(what)`: offered, passed, marked warning (`--dangerously-skip-permissions`).
    A `OneOf` flag of the operator's may name some of its values as
    weakening (`weakening_values`): `--permission-mode bypassPermissions`,
    Codex `--sandbox danger-full-access` and `--ask-for-approval never`
    give up what the switch would, and are marked the same way. A
    `Keyed` flag names `key=value` settings the same way, compared with
    the value's quotes and spaces dropped: Codex `-c
    sandbox_mode="danger-full-access"` and `-c approval_policy=never`.
  - `Degrades(what)`: offered, passed, marked warning naming what UZE
    delivers that stops loading (`--strict-mcp-config`, `--bare`,
    `--setting-sources`, `--disable-slash-commands`; Codex `--disable`).
  - `Uze`: never offered, refused (session, resume, continue, fork, print,
    first-prompt flags; `--settings` until the Lab proves it merges).
  - `Keyed { owned: &[..] }`: a configuration flag (Codex `-c`/`--config`)
    whose keys are checked one by one.
- `managed_environment() -> &'static [&'static str]`: variables it sets
  or reads to deliver and resume.

The catalog is a `FlagCatalog`: the specs and the `FlagGrammar` the
harness's parser reads a single-dash word in, since the same word is a
different flag to each. `Posix` (clap, commander): one dash starts a
cluster of short switches and a short flag that takes a value takes the
rest of its word, so `-model` is `-m odel`. `GoFlag` (Go's `flag`): one
dash names a long flag as two do, and no word is a cluster. `EffectCli` (Effect's CLI): `-name=value`
names a flag, and a single-dash word without `=` is a cluster.

The integration reuses its `session.rs` constants, so `--resume` is
spelled once. The rule every harness shares (`UZE_*`, `PATH`, `HOME`,
`TRACEPARENT`, `TRACESTATE`) lives in `uze-workspace`, not repeated per
integration.

Codex keys: `agents`, `hooks.SessionStart`, `hooks.state` always owned;
`notify` and `features.memories` owned *when the plan composes them*.
Matching covers `-c k=v`, `-ck=v`, `--config k=v`, and a parent table
(`-c hooks={..}`, `-c features={..}` collide with an owned child).
`--enable <feature>` and `--disable <feature>` are `-c
features.<feature>=true|false` by Codex's own `--help`, so their spec names
the table their value is a key in (`sets_key_in`), and that key is judged
as `-c`'s is; `--disable` stays `Degrades` on top.

What a catalog holds is the whole interactive `--help` of the release:
every flag its snapshot defines is catalogued, unless it ends the launch
(help, version, completions, a wizard) or acts only on non-interactive
output (Claude Code's `--output-format`, `--json-schema`, `--max-budget-usd`
and the rest the help says work only with `--print`; agy's print-mode
flags), and each one left out is named, per harness, with its reason. A
flag that collides with UZE is not left out to be passed unseen: it is
catalogued and classified, `Uze` when it moves the session out of its pane,
its checkout or this machine (Claude Code `--bg`, `--desktop`, `--cloud`,
`--environment`, `--worktree`, `--from-pr`, `--teleport`; Codex `--cd`,
`--worktree`, `--remote`; OpenCode `--server`), `Weakens` or `Degrades`
when it gives up safety or switches off a delivery. The `Uze` reason is
what a refused entry says, so the operator learns why rather than meeting
an agent that left its pane.

Rejected: a TOML catalog under `conformance/` or the Store. Only
`uze-integrations` names a harness, and the catalog is the same kind of
fact as `session.rs`'s constants.

### 5. One check, two strictnesses

`uze_workspace::overrides::check(tokens, catalog, managed, plan: Option<&LaunchPlan>)`
returns one verdict per entry: `Accepted`, `Unknown`, `Weakens`,
`Degrades`, `Refused(reason)`. The popup calls it with no plan, pure and
on each keystroke, treating conditionally owned keys as `Degrades`, whose
entry reads `-c turns off uze's own value for this key`. The shim calls it with the real plan, where a
conditionally owned key the plan composes is left out with a note. Both
refuse positionals and `--`.

### 6. Input is one typed line, entries one per Enter

The step's line takes `DEBUG=1 --model opus`, split with single and double
quotes and backslash escapes, no expansion: a small splitter in
`uze-workspace`, not a dependency (neither `shlex` nor `shell-words` is a
direct dependency today, and the grammar is a dozen lines). A
`NAME=value` with a valid variable name is a variable; anything else is a
flag or the value its catalog entry takes. Enter turns the line into
entries; a refused token is not added and stays in the line in the danger
hue. Backspace on an empty line removes the last entry.

### 7. History

Up to 20 sets per harness, this project's first and then the rest, each
group most recent first, identified by their normalized tokens so the
same set moves to the top and takes the project it was last used in. A variable whose name
reads as a secret (`TOKEN`, `SECRET`, `PASS`, `PWD`, `CRED`, `KEY`,
`AUTH` or `DSN` anywhere in it, upper-cased, or `PAT` as a `_`-separated
part; withholding `AUTHOR` too is the price of never keeping `GH_PAT`),
or whose value holds a URL with credentials before its host
(`DATABASE_URL=postgres://u:pw@h/db`, `https://key@host`), is not kept:
the set counts it in `withheld` and its row says `1 variable not kept`. Spans carry
`overrides = <count>`, never a token.

### 8. Proving the catalog

One committed `--help` snapshot per harness (and `codex resume --help`)
under `crates/uze-integrations/src/<harness>/`. A Rust unit test per
harness asserts, both ways, that the flags its snapshot defines are exactly
the catalog's spellings and the ones it names as left out: nothing
unclassified, nothing catalogued or left out that the snapshot does not
define. The Codex catalog is also held to `codex resume --help`, since
overrides follow `resume <id>`. The Lab diffs each snapshot against the
live binary, so the snapshot cannot drift from the release unseen; a flag
that appears live fails the Lab, the refreshed snapshot then fails the
unit test until the flag is classified, and `harness-watch` refreshes
both. `-c` keys are not in `--help` and are
proven by the existing Codex conformance instead.

### 9. Removing Profiles first, and what it leaves on disk

Profiles is removed before the catalog is built, in its own commits, so
nothing new is written against `PreferencePort` and the two harness
vocabularies (a profile's axes, a launch's flags) never coexist. The
model and permission axes were the reason Profiles existed; a launch
override expresses them per agent without touching the harness's global
configuration, which every session on the machine, UZE's or not, reads.

What it leaves:

- `state/profiles.json` exists on every machine (`snapshot.machine`
  listed profiles unconditionally, and listing wrote a default). It is a
  record nothing reads any more, so a record tier that no longer claims
  its cost: removed once, at the point the application already retires
  what earlier releases left behind.
- `layout.json`'s `profile_columns_width` and a `route: "profiles"` read
  as unknown and fall back; nothing to do.
- Keys `Apply` wrote into harness configuration (Claude's
  `permissions.defaultMode`, `sandbox.enabled`, `model`; Codex's
  `approval_policy`, `sandbox_mode`; OpenCode's `permission.*`;
  Antigravity's `agentMode`, `toolPermission`, …) stay. No receipt
  records them, so UZE cannot tell them from the operator's own edits,
  and removing an operator's setting is worse than leaving one UZE wrote
  at their request. The CHANGELOG says so.

Rejected: keeping `PreferencePort` as the catalog's home. It models a
persistent machine-wide setting with a plan and an apply; the catalog is
static data about a binary's argv. Sharing the trait would keep the
write path alive for nothing.

`Feature` keeps the mechanism with `Overview` as its one entry; its tests
move to `Overview`.

## UI/UX

Against the operator's mockup and the design-system rules:

- **Overrides is a step, not an expanding section.** The mockup grows the
  popup downward; "a modal keeps its height" and "dropdowns never push
  content down". The step takes the list's place, headed `‹ overrides` (a
  target), esc `back`, esc again closes. The popup's height is fixed when
  it opens as the larger of what the two steps need, so switching never
  resizes it.
- **Two views, named on the top border:** `new agent · overrides` stand
  where the title would be, the first one standing for it, parted by the
  hint bar's separator, the one in front in the accent and the other held
  back (colour only: no ground and no hover; their own hit target, so
  pointing at one never lights the foot's `ctrl+f` hint). Under the border
  the harness row, `‹ Claude Code ›` unlabelled against its end, and a
  rule, then the view in front:
  `new agent` is the filter and the listing, `overrides` the line, the
  entries and the list, in the same rows, each line typed into in the
  same look. At the body's foot, with air over and under it, one muted
  line counts the overrides once there are any (`3 flags · 2 vars`, in
  the warning hue when one gives something up); with none it says
  nothing. A click on `overrides`, or `ctrl+f`, opens that view; a click
  on `new agent`, or Esc, goes back. On `overrides` the arrow keys move the
  caret, but the harness keeps its arrows, drawn alike, and a click on
  one changes the harness there: the entries, the catalog and the sets
  used lately follow it, each kept per harness. The popup is
  52 columns with 8 to 12 listing rows; `esc close` gives way to `ctrl+f
  overrides` in the foot.
- **Not `ctrl+o`.** It is the Global `SwitchMode` and reaches Global even
  while a filter is typed into; `tab` is the picker's `Expand`, `←→` the
  harness. The new action's default chord is the operator's call.
- **A draft per harness inside the popup**, so going back and moving the
  harness shows that harness's entries, never another harness's flags.
- **Not prefilled with the last set.** The harness is prefilled because a
  wrong harness is visible and harmless; a sticky
  `--dangerously-skip-permissions` is neither. The last set heads the
  history instead.
- **The step, top to bottom:** the typed line (underline, caret, no raised
  band, which the mockup has); the entries added, one row each with an `x`
  in `mark::close_tone`; then a list: `recent` sets for this harness while
  the line is empty (this project's first, another project's sets naming
  it, muted, at their end), the catalog's completions while typing, a flag's
  accepted values after a `OneOf` flag and a space. Under a rule, a detail
  line of fixed height for the marked row (`Permission mode · default,
  acceptEdits, plan`) instead of the mockup's right column cut mid-word.
  Rows say at their end only what is known: `value`, `repeatable`,
  `unknown`.
- **Keys in the step:** `←→` move the caret (its own scope), `↑↓` mark in
  the list, which opens with nothing marked; Tab completes; Enter adds the
  line, takes the marked row, or, with an empty line and nothing marked,
  launches.
- **States by token:** `Weakens` and `Degrades` carry the warning hue and
  `Symbol::Warning` on the entry and on the history row, and the detail
  line names what is lost; a refused token the danger hue, the detail line
  says why (`managed by uze · conversations continue on their own`).
- **Words in English**, lowercase: the mockup's Portuguese (`adicionar`,
  `escolher`, `voltar`, the flag descriptions) becomes English.
- **Foot:** first step `enter new · ←→ harness · <key> overrides`, `esc
  close`; overrides step `enter add · tab complete · esc back`, Enter named
  by what it does there: `new` on an empty line with nothing marked,
  `take` with a row marked, `add` otherwise. Every hint
  a click target with a click == key test.
- **After launch:** the agent's overview names its overrides under its
  harness. A launch carrying a `Weakens` flag, a respawn included, says so
  on the shim's note line.

## Candidate ADRs

- **Launch inputs travel in a private per-project record read by the
  shim** — it makes that record, not the pane, the authority over what an
  owned launch carries, and later per-launch inputs (project defaults, a
  relaunch with edits) bind to it.

## Risks / Trade-offs

- [A catalog lags a vendor release] → unknown flags still pass (with
  `--f=v`); the snapshot test and the Lab's diff catch a removed flag.
- [A `Degrades` list is incomplete] → it is a warning, never a refusal,
  so a miss costs a missing mark, not a broken launch.
- [Conditional ownership differs between popup and shim] → the popup is
  conservative (warns), the shim decides with the real plan and says what
  it left out.
- [Windows: an npm harness is a `.cmd`, and std refuses arguments with
  `"` or `%`] → the shim's error stands in the pane; no rewriting.
- [A bad value breaks the harness] → the harness's own error stands in
  the pane; UZE checks values only for `OneOf` flags.

## Open Questions

- Whether a tab or caption row may carry a warning mark for an agent
  running with a `Weakens` flag. The caption rule today allows project then
  harness only; this is the operator's call and changes only drawing.
