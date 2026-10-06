## Purpose

A project writes path-scoped instructions for its agents once, as rules in
its own checkout, and every supported harness receives each rule when it
applies — natively where the harness reads rules itself, through a
generated engine where it does not — instead of loading every instruction
into every session.

## ADDED Requirements

### Requirement: A project's rules are Markdown files under `.agents/rules/` in the checkout
The system SHALL treat every `*.md` file directly under `.agents/rules/` at
the root of a checkout as one project Rule. The project authors and versions
these files; UZE SHALL NOT copy, translate, move or rewrite them, and SHALL
NOT create a second rules directory in the project for any harness. A rule's
identity SHALL be its path relative to the checkout root.

#### Scenario: Rules are found where the project keeps them
- **WHEN** a checkout contains `.agents/rules/ui.md` and `.agents/rules/testing.md`
- **THEN** UZE reports two project rules named by those paths
- **AND** no file is written under `.claude/`, `.codex/` or `.opencode/` in the checkout

#### Scenario: A worktree is answered with its own rules
- **WHEN** two checkouts of one repository are on branches whose `.agents/rules/` differ
- **THEN** an agent in each checkout receives the rules of its own checkout and never the other's

### Requirement: The canonical rule frontmatter is Antigravity's
A rule SHALL open with a YAML frontmatter block declaring `trigger`, one of
`always_on`, `glob`, `model_decision` or `manual`. A `glob` rule SHALL
declare `globs`: one or more comma-separated patterns relative to the
checkout root. A `model_decision` rule SHALL declare `description`. Any other
field SHALL be ignored. The body after the frontmatter is the instruction
delivered to the agent.

A file whose frontmatter does not parse, declares no or an unknown
`trigger`, or omits the field its trigger requires SHALL NOT be delivered
by UZE and SHALL be reported with the reason.

#### Scenario: A file without a trigger is reported, not guessed
- **WHEN** `.agents/rules/notes.md` has no frontmatter
- **THEN** `uze status` reports it as not a rule, naming the missing `trigger`
- **AND** no harness receives it from UZE

#### Scenario: A glob rule without patterns is invalid
- **WHEN** a rule declares `trigger: glob` and no `globs`
- **THEN** it is reported as invalid and is not delivered

### Requirement: Glob patterns follow one documented dialect
The system SHALL support, in `globs`, the patterns `**` (any number of path
segments, including none), `*` (any characters within one segment), `?` (one
character within a segment) and literal characters, matched against a path
relative to the checkout root with `/` as the separator. A pattern using any
other construct SHALL be reported, and the rule SHALL NOT be delivered by
UZE, so that a rule never matches differently on an adapted harness than on
a native one.

#### Scenario: Directory pattern matches nested files
- **WHEN** a rule declares `globs: src/ui/**`
- **THEN** it matches `src/ui/button.ts` and `src/ui/forms/field.ts`
- **AND** it does not match `src/core/math.ts`

#### Scenario: Several patterns in one rule
- **WHEN** a rule declares `globs: src/**/*.test.ts, tests/**`
- **THEN** it matches `src/core/math.test.ts` and `tests/e2e/login.ts`

#### Scenario: Unsupported construct is reported
- **WHEN** a rule declares `globs: src/**/*.{ts,tsx}`
- **THEN** `uze status` reports the pattern as outside the supported dialect
- **AND** UZE does not deliver the rule

### Requirement: Each trigger reaches the agent at the moment it applies
On every harness that receives rules, the system SHALL deliver:

- an `always_on` rule's body at the start of every session;
- for each `model_decision` rule, at the start of every session, one line
  naming the rule's path and its `description`, so the agent can read the
  rule when it decides the description applies;
- a `glob` rule's body after a tool call that read, edited or wrote a path
  matching one of its patterns, at most once per session;
- nothing for a `manual` rule, which the person references themselves.

#### Scenario: A glob rule arrives with the file it governs
- **WHEN** an agent reads `src/ui/button.ts` in a checkout whose `ui.md` rule declares `globs: src/ui/**`
- **THEN** the body of `ui.md` reaches the agent before its next model request
- **AND** a later read of another file under `src/ui/` in the same session does not deliver it again

#### Scenario: A glob rule does not leak
- **WHEN** the same agent only touches `src/core/math.ts`
- **THEN** the body of `ui.md` is never delivered

#### Scenario: On-demand rules are announced, not loaded
- **WHEN** a session starts in a checkout with a `model_decision` rule `testing.md`
- **THEN** the agent receives one line with `.agents/rules/testing.md` and its description
- **AND** the body of `testing.md` is not delivered by UZE

#### Scenario: Compaction does not lose an always-on rule
- **WHEN** a session's context is compacted
- **THEN** the always-on rules and the on-demand index are delivered again
- **AND** a glob rule already delivered before the compaction may be delivered again on its next match

### Requirement: Each harness receives rules by its most native route
The system SHALL deliver rules to Antigravity natively, by relying on its
own reading of `.agents/rules/`, and SHALL install nothing for it. The system
SHALL deliver rules to Claude Code, Codex and OpenCode through a generated
rules engine, reported as an adapted route. The route SHALL be reported per
harness with its reason, including, for Claude Code, that its native
mechanism needs a rules directory inside the project.

#### Scenario: Antigravity is left to itself
- **WHEN** UZE sets up Antigravity on a machine
- **THEN** no rules engine, hook entry or plugin is installed for it
- **AND** `uze status` reports Antigravity's rules route as native

#### Scenario: Claude Code reports why it is adapted
- **WHEN** `uze status` reports the rules route for Claude Code
- **THEN** the route is adapted and the reason names the project-local rules directory UZE declines to create

### Requirement: A package's rules never reach a harness's machine-wide rules
The system SHALL NOT deliver a package's `rules/` directory, or any rule it
contains, to a location a harness reads for every project. This covers a
plugin a harness installs for the whole machine, and the harness's global
rules directories. A package that contains `rules/` SHALL be delivered to
such a harness without it, in a plugin UZE generates rather than the package
tree unchanged. Until project delivery of package rules exists, the package's
rules SHALL be reported as not delivered, with the reason.

#### Scenario: Antigravity does not receive a package's rules machine-wide
- **WHEN** a package containing `rules/ui.md` is installed for Antigravity
- **THEN** the plugin `agy` stages under its global plugin directory contains no `rules/`
- **AND** the package's rules are reported as not delivered, because rules are delivered per project

### Requirement: The rules engine is installed once per machine and activated by the checkout
The system SHALL install the rules engine as part of machine setup for each
harness that needs it, as receipt-owned entries in that harness's own
configuration (a command hook in Claude Code's and Codex's user settings, a
generated plugin in OpenCode's global plugin directory), never touching
foreign entries. The engine SHALL decide at run time, from the session's
working directory, which checkout it is in and SHALL answer nothing for a
checkout without `.agents/rules/`. It SHALL NOT require any per-project
command, file or registration to activate.

#### Scenario: A project without rules pays nothing
- **WHEN** an agent works in a checkout with no `.agents/rules/` directory
- **THEN** the engine answers with no context and the tool call proceeds unchanged

#### Scenario: Cloning a project with rules is enough
- **WHEN** a person clones a repository that contains `.agents/rules/` on a machine where UZE setup has run
- **THEN** the first session in that checkout on any of the four harnesses receives its rules, with no `uze install` in between

#### Scenario: Removing setup removes only the engine
- **WHEN** the rules engine is removed from a harness
- **THEN** only the entries and files UZE delivered for it are removed, after their content is inspected
- **AND** the operator's own hooks and plugins are unchanged

### Requirement: The engine runs without UZE and never blocks the agent
The delivered engine SHALL run with no `uze` process on its execution path.
It SHALL be observational: it SHALL never deny, delay beyond its timeout, or
alter a tool's input. Any failure — a missing dependency, an unreadable rule,
an unknown payload — SHALL let the tool call proceed with no context added,
and SHALL be reportable by `uze doctor`. The engine SHALL record which rules
it delivered per session only in UZE's generated tier, so that deleting that
record costs at most a repeated delivery.

#### Scenario: Missing dependency fails open
- **WHEN** the engine's dependency (`jq` for the `sh` engine) is absent
- **THEN** every tool call proceeds without rules
- **AND** `uze doctor` names the missing dependency and the harnesses affected

#### Scenario: Uninstalling UZE leaves rules working
- **WHEN** the `uze` binary is removed from the machine after setup
- **THEN** the delivered engine still delivers rules to a session

### Requirement: Paths a tool touched are recognised in every tool shape the harness uses
The engine SHALL recognise the paths a tool call touched from the fields the
harness's file tools declare, from the file headers of a patch tool, and from
the arguments of a shell command, relative to the checkout root or absolute
within it. A path recovered from a shell command is a best effort, and the
route SHALL say so where the harness reads files through its shell.

#### Scenario: Codex reading through its shell
- **WHEN** Codex runs `sed -n '1,40p' src/ui/button.ts`
- **THEN** the `ui.md` rule is delivered

#### Scenario: Codex editing through a patch
- **WHEN** Codex applies a patch whose header is `*** Update File: src/ui/button.ts`
- **THEN** the `ui.md` rule is delivered if it was not already

### Requirement: An agent can check a project's rules offline
`uze agent rules check [<path>]` SHALL validate every file under
`.agents/rules/` of the checkout containing `<path>` (default: the working
directory) with the same validation `uze status` reports. It SHALL need no
workspace, no network and no harness. It SHALL exit zero only when every
file is a deliverable rule. Otherwise it SHALL exit non-zero and name each
offending file with its reason. When it succeeds, it SHALL name each rule
with its trigger and, for a `glob` rule, its patterns.

#### Scenario: A rule with an unsupported pattern fails the check
- **WHEN** an agent runs `uze agent rules check` in a checkout whose `ui.md` declares `globs: src/**/*.{ts,tsx}`
- **THEN** the command exits non-zero
- **AND** its output names `.agents/rules/ui.md` and the unsupported pattern

#### Scenario: A valid set passes and is summarized
- **WHEN** every file under `.agents/rules/` is a deliverable rule
- **THEN** the command exits zero and lists each rule with its trigger and patterns

#### Scenario: A checkout without rules is not an error
- **WHEN** the checkout has no `.agents/rules/` directory
- **THEN** the command exits zero and says the project has no rules

### Requirement: `uze status` reports the project's rules
`uze status` SHALL report, for the project, the number of rules per trigger,
every file under `.agents/rules/` that is not a deliverable rule with its
reason, and the rules route per detected harness. It SHALL NOT read or
report any rule body.

#### Scenario: Status names the problem and the route
- **WHEN** a project has two valid rules and one with an unsupported glob, and Claude Code and Antigravity are detected
- **THEN** status reports `2 rules` by trigger, the invalid file with its pattern, Claude Code as adapted and Antigravity as native
