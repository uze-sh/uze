## 1. Measurements that close the design's open edges

- [x] 1.1 Measure Antigravity's glob dialect with real `agy` sessions: which of `**`, `*`, `?`, `{a,b}`, `[ab]`, a leading `/` and a pattern with spaces match a touched file. Record the supported intersection as the shared pattern table (task 2.2), and the version measured
- [x] 1.2 Measure what `agy` does with a rule that has no frontmatter, an unknown `trigger`, and `trigger: glob` without `globs`. The spec's "not a rule" reporting follows what Antigravity itself ignores
- [x] 1.3 Measure whether Codex fires `SessionStart` (and with which `source`) after a compaction. If it does not, record the limitation in Codex's rules route reason
- [x] 1.4 Confirm on Claude Code that `SessionStart` with no matcher fires for `compact` and `clear`, and that its `additionalContext` reaches the model after a compaction

## 2. The Rule model (uze-core, no harness named)

- [x] 2.1 `uze-core::project::rules`: parse a rule file's frontmatter (`trigger`, `globs`, `description`; unknown fields ignored) into a typed Rule or a typed reason it is not deliverable (no frontmatter, unknown or missing trigger, missing required field, pattern outside the dialect)
- [x] 2.2 Glob dialect: a matcher for `**`, `*`, `?` and literals, relative to the checkout root, plus the validator that rejects anything else. One shared fixture table (pattern, path, expected) lives where both the Rust tests and the engine's tests (5.4) read it
- [x] 2.3 Discovery: the rules of a checkout are the `*.md` files directly under `<root>/.agents/rules/`, named by their checkout-relative path. Worktrees resolve to their own root
- [x] 2.4 `UzeHome::rules_sessions_dir()` under `runtime/`, and register it with `every_path_uze_owns_is_named_in_the_map`
- [x] 2.5 Unit tests: every reason a file is not a rule, each trigger's required fields, the full fixture table, a checkout with no `.agents/rules/`

- [x] 2.6 Discovery reads `.agents/rules.json` entries as Antigravity does (path relative to the checkout root, a directory's immediate `*.md`, a single file, `include_only`/`exclude` relative to the entry, one rule per file however often it is registered); an unparseable manifest adds nothing and is reported; `inherits` is not followed and is reported
- [x] 3.5 The engine reads the same entries, proven against shared fixtures with the Rust model

## 3. The engine templates (uze-integrations::rules)

- [x] 3.1 The POSIX `sh` engine template, implementing design D5:
  - read the payload and resolve the checkout from `cwd`;
  - exit after one stat when there are no rules;
  - parse the frontmatter and translate the dialect to an ERE;
  - `SessionStart` answers with the always-on bodies and the `model_decision` index;
  - `PostToolUse` answers with the matched `glob` rules (path recovery from fields, patch headers and shell tokens);
  - deduplicate per session, and truncate on a `compact`/`clear` start;
  - wrap each rule as `<rule …>`;
  - fail open on every error, including the `jq` guard reused from the portable wrapper.

  Nothing in the template names the packager.
- [x] 3.2 The OpenCode V2 plugin template, `rules.ts`:
  - default export and no imports;
  - `session.hook("context")` pushes the always-on rules and index into `event.system`;
  - `tool.hook("execute.after")` appends matched rules to `event.result.content` on `completed`;
  - both ask the same `sh` engine through `Bun.spawn`, with the payload built from the event (`input.path` and the other V2 field names).
- [x] 3.3 Golden tests: each template rendered byte-for-byte against a fixture for a given `UzeHome`, and stable across renders
- [x] 3.4 Engine tests run against recorded payloads per harness (Claude `Read`/`Edit`/`Bash`, Codex `exec_command`/`apply_patch`, OpenCode `read`/`edit`/`write`). Cover:
  - match and no-match;
  - absolute and relative paths, and a path outside the checkout;
  - deduplication, and the reset on compaction;
  - a checkout without rules, an invalid rule skipped while the valid ones still answer;
  - `jq` absent.

## 4. Delivery per harness

- [x] 4.1 `rules_route()` on `IntegrationPort`:
  - Antigravity: Native, delivers nothing;
  - Claude Code: Adapted, with the reason naming the project rules directory UZE declines to create (design D3);
  - Codex: Adapted, with the shell best-effort note, plus 1.3's result if negative;
  - OpenCode: Adapted;
  - Windows: Unsupported for the adapted three, with the reason.
- [x] 4.2 Claude Code: write the engine file and receipt-owned `SessionStart` and `PostToolUse` (`Read|Edit|MultiEdit|Write|Bash`) entries in `~/.claude/settings.json`, in exec form with absolute paths. Inspect by content identity, and never touch a foreign entry
- [x] 4.3 Codex: the same in `~/.codex/hooks.json` (shell-line form, as the portable hooks' Codex entries)
- [x] 4.4 OpenCode: the generated `rules.ts` in the global plugin directory, receipt-owned like the portable-hook bridge, and no `plugin` entry in `opencode.json`
- [x] 4.5 Removal: detaching the rules engine removes only its own entries and files, after inspection, and refuses drift as portable hooks do
- [x] 4.6 Guard the machine-wide leak:
  - Antigravity delivers a package containing `rules/` through its generated plugin with `rules/` excluded, the way a non-default invocation policy already forces capability-by-capability delivery;
  - the other harnesses' plugin envelopes never carry `rules/`;
  - the package's rules are reported as not delivered, with the per-project reason.

  Include a test that the plugin staged by `agy` has no `rules/`
- [x] 4.7 Integration tests with `FakeHarness`/isolated homes:
  - setup is idempotent;
  - a foreign hook and plugin survive;
  - drift blocks removal;
  - Antigravity receives nothing.

## 5. Application, CLI and reporting

- [x] 5.1 `uze setup` (and the workspace's automatic setup) delivers the rules engine to every detected harness whose route needs it, and reports it per harness like the other setup results
- [x] 5.2 Sweep `runtime/rules/sessions/` entries older than seven days during setup
- [x] 5.3 A rules read model in `uze-application`, so `src/` names no `uze_core`
- [x] 5.4 `uze status` reports:
  - rules per trigger;
  - each file that is not a rule, with its reason;
  - the route per detected harness.

  It never prints a rule body. Add `TestBackend`/snapshot tests for the output
- [x] 5.5 `uze doctor` reports a missing engine dependency (`jq`) and a delivered engine whose entry or file drifted, naming the harnesses affected
- [x] 5.6 `uze agent rules check [<path>]`. Behavior:
  - resolve the checkout from the path;
  - run the shared validator;
  - exit non-zero with file and reason per offender;
  - list each rule with its trigger and patterns on success;
  - say "no rules" (exit zero) when there is no `.agents/rules/`.

  Hidden from `uze --help` like the rest of `uze agent`. CLI tests for each spec scenario
- [x] 5.7 Classify `uze agent rules check` (and any changed leaf command) in `command_performance.rs`. Both it and `status` are `Budgeted`: rule discovery is a directory read and a small-file parse
- [x] 5.8 Name `uze agent rules check` in the `uze:rules` Skill. No UZE text is projected into `AGENTS.md` (`keep-agents-md-the-operators`)

## 6. Conformance Lab and journeys

- [x] 6.1 Contract scenario `rules-glob-reaches-model`, harness-neutral (`conformance/contract/rules.py`). A checkout with a `**/ui/**` glob rule; one scripted turn writes a control file in another directory, then a matching file. The synthetic provider's requests carry the rule's body after the matching write returned and in no request before it, the one after the control's write included. Claude Code, Codex and OpenCode run the engine the workspace's setup delivers; Antigravity runs natively
- [x] 6.2 Contract scenarios for `always_on` (in the turn's first request) and `model_decision` (its description in the first request; its body in none until the agent read the file), asserted on the synthetic provider's captured requests
- [x] 6.3 Contract scenario for compaction: a live session compacted with `/compact`; the request after it carries the provider's compaction summary and the always-on rule. Green on Claude Code, Codex and OpenCode; Antigravity declares it (`/compact` sits behind the server-side flag `enable-compact-slash-command`, absent offline)
- [x] 6.4 Antigravity runs the rules contract natively in the offline Lab, signed in (1.4.2): glob, always_on and model_decision green, with the 1.1–1.2 measurement (`experiments/antigravity/rules`) as the design's evidence; only the compaction scene is declared through `bindings.unsupported`, registered in `evidence/expected.json`
- [ ] 6.5 A journey in `05-delivery`: clone a project containing `.agents/rules/`, run a harness session that edits a matching file, and check the file on disk. It `proves:` the rules docs page

## 7. Authoring Skills (plugins/uze)

- [x] 7.1 New `plugins/uze/skills/rules/SKILL.md` (`uze:rules`), following design D7:
  - a trigger text that matches "create/add/write a rule", "this only applies to src/…" and "split AGENTS.md";
  - deciding rule vs `AGENTS.md` vs Skill;
  - choosing the trigger;
  - the glob dialect;
  - one concern per file;
  - writing a `description` as when-to-read;
  - wording as a convention;
  - closing with `uze agent rules check`.

  Invocation policy and frontmatter follow the sibling Skills
- [x] 7.2 Extend `plugins/uze/skills/init/SKILL.md`:
  - recognise `.claude/rules/`, `.cursor/rules/` (`.mdc`), `.windsurf/rules/` and `.agent/rules/`, and path-specific sections of `AGENTS.md`;
  - report them in the inventory;
  - propose migrating them to `.agents/rules/` through `uze:rules` with the mechanical frontmatter mapping;
  - flag, never rewrite, a pattern outside the dialect.
- [x] 7.3 Run `uze agent plugin check plugins/uze` and the marketplace check. Add the `rules` Skill to whatever lists the official plugin's Skills (marketplace description, docs)
- [x] 7.4 Lab or fixture proof that `uze:rules` is delivered like its siblings (invocation policy honored per harness). Reuse the existing Skill delivery scenarios rather than adding a vertical

## 8. Documentation and diagrams

- [x] 8.1 Author-facing page on project rules:
  - location and frontmatter;
  - the four triggers and the glob dialect;
  - writing a rule as a convention rather than an imperative;
  - the per-harness route table;
  - what `uze status` reports.
- [x] 8.2 Update `docs/architecture/core-components.mmd` (`project` includes rules) and `containers.mmd` (integrations project rules), then run `make artifacts`
- [x] 8.3 Add the guarded properties to `docs/architecture/invariants.md`, each tied to its test:
  - no rules directory written into a project;
  - the engine fails open;
  - Antigravity receives nothing.
- [ ] 8.4 Open the upstream Claude Code request named in design D3's exit condition (`--add-dir` rule `paths` evaluated against the session's working directory, or `globs` read), and link it from the Claude route reason
