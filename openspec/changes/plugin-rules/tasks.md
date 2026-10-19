## 1. Measurements

- [x] 1.1 Measure with real `agy` sessions whether a rule in a subdirectory of `.agents/rules/` is applied (try `<name>@<market>/ui.md`, including the `@`). Decide D3's namespace form from the result
- [x] 1.2 Measure whether `agy` applies a rule whose file name carries the flat prefix form (`<plugin>@<market>--ui.md`), as the fallback
- [x] 1.3 Confirm that Codex's plugin format has no rules directory of its own a package could leak through. Record the version measured. Claude Code is answered by its plugins reference (2026-10-06): a plugin ships skills, commands, agents, hooks, MCP, LSP, output styles, workflows, themes, monitors, `bin/` and `settings.json`, and no rules. A `CLAUDE.md` at the plugin root is not loaded and `claude plugin validate` warns on it. Re-check only if that reference gains a rules component

## 2. Package capability

- [x] 2.1 Recognise `rules/*.md` in a package as the Rule capability, validated with `project-rules`' validator. `uze agent plugin check` fails on a non-deliverable rule
- [x] 2.2 `uze agent plugin create --rules` scaffolds `rules/` with one example rule that passes the check
- [x] 2.3 Store bytes stay verbatim. The rules are part of the ingested bytes the lock's `integrity` covers. Test that changing a rule changes `integrity`

## 3. Projection into the checkout

- [x] 3.1 The namespace form from 1.1/1.2, as one function in `uze-core::project` naming the location for a plugin identity. Widen `project-rules`' discovery to include that form, and only that form
- [x] 3.2 Copy a locked plugin's rules byte for byte into its namespace during `uze <plugin>@<market>` and `uze install`, never touching the project's own rules or another namespace
- [x] 3.3 Inspection: classify each copy against the plugin's rules at the locked revision as current, drifted or missing, and each extra file in the namespace as extra, with no `$UZE_HOME` state needed
- [x] 3.4 Lifecycle:
  - install restores missing copies;
  - update replaces current copies and removes rules the new revision dropped;
  - remove deletes the namespace;
  - all three refuse to overwrite or remove a drifted copy, and name it.
- [x] 3.5 Machine scope: `uze plugin install` reports the rules as delivered per project only, naming `uze <plugin>@<market>`, and writes into no project
- [x] 3.6 Unit and integration tests for every scenario in the spec, including:
  - a fresh `$UZE_HOME` judging a cloned project;
  - two projects on one machine, only one declaring the plugin.

- [x] 3.7 Folder layout (D3, D7): copies under `.agents/rules/<plugin>@<market>/`, the folder registered in `.agents/rules.json`, owning only UZE's entries; an unparseable manifest never written; the manifest created and removed with the first and last plugin. Tests for every scenario of the folder requirement
- [x] 3.8 Lab: the plugin-rules contract on all four harnesses with the folder layout, Antigravity reading it through the manifest

## 4. Reporting and Skills

- [x] 4.1 `uze status`: per plugin, the rule count and copy state, naming each file that is not current. Without network, the copies are reported as unverified
- [x] 4.2 `uze:author` Skill: the `--rules` flag. Rules shipped in a plugin are delivered per project and committed by the consumer. An `always_on` rule is how a plugin speaks to every agent of a declaring project (D6). Validate with `uze agent plugin check`
- [x] 4.3 `uze:rules` Skill: when a rule belongs in a shared plugin rather than the project, and how to move a project rule into one (the D5 "take ownership" path in reverse)

## 5. Conformance and journeys

- [x] 5.1 Contract scenario: a plugin with a `glob` rule declared in project A and absent from project B, both on one machine. The rule fires in A on each harness and never in B
- [ ] 5.2 Journey in `02-packages`:
  - add a rules plugin to a project;
  - commit;
  - clone into a fresh world;
  - `uze status` reports the copies current with an empty `$UZE_HOME`;
  - a hand edit is reported as drift and survives `uze install`.

## 6. Documentation

- [x] 6.1 Extend the project rules page with sharing rules through a plugin: publishing, declaring, committing the copies, drift and updates
- [x] 6.2 Add to `docs/architecture/invariants.md`, each tied to its test:
  - plugin rules never reach a non-declaring project;
  - a drifted copy is never overwritten.
