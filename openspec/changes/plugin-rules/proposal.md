## Why

`project-rules` lets one project write rules for its agents. An
organization's conventions usually outlive one project: a squad writes the
rules that every repository of the same stack should follow, and today the
only way to share them is to copy files between repositories by hand,
where they drift the day after.

UZE already distributes everything else a project's agents use through
marketplaces, `agents.yaml` and `agents.lock`. Rules are the one capability
that cannot ride the existing delivery. Every other capability reaches a
harness's user scope and is visible in every project. A rule injects itself
whenever a path matches, so delivered machine-wide, one project's
conventions would govern code in every other project on the machine.

## What Changes

- **A plugin may ship rules**: `rules/*.md` at the package root, in the
  canonical rule format of `project-rules`.
  `uze agent plugin create --rules` scaffolds the directory, and
  `uze agent plugin check` validates the rules with the same validator.
- **A project that declares the plugin receives its rules in its own
  checkout.** `uze <plugin>@<market>` and `uze install` copy the package's
  rules into a namespaced directory under `.agents/rules/`. From there every
  harness reads them exactly as it reads the project's own rules:
  Antigravity natively, and the other three through the `project-rules`
  engine. Nothing about the copies is delivered machine-wide.
- **The copies are committed with the project**, beside `agents.lock`. A
  rule change arriving through `uze update` is a reviewable diff in the
  pull request. A clone, a worktree or CI has the rules without running
  UZE, and Antigravity reads them natively there.
- **Integrity is derived from the lock, not from machine state.** A copy is
  owned by the plugin its directory is named after. It is correct when it
  matches that plugin's rules at the locked revision, whose bytes the
  lock's `integrity` already pins. A copy someone edited is reported as
  drift and never overwritten. A copy missing on disk is restored by
  `uze install`.
- **`uze remove <plugin>`** in the project removes its rules directory,
  after inspecting it. **`uze plugin install`** (machine scope) reports a
  plugin's rules as delivered per project only.
- **`uze status`** reports each plugin's rules as current, drifted, missing
  or out of date against the lock.
- **The `uze:author` Skill** gains the rules capability, the way it covers
  hooks and MCP.

## Capabilities

### New Capabilities
- `plugin-rules`: rules shipped in a plugin, their copy into the checkout of
  every project that declares the plugin, ownership and integrity of the
  copies, and their lifecycle across add, install, update and remove.

### Modified Capabilities
<!-- None archived yet. This change builds on two changes still in flight:
     `project-rules` (its rules location and engine) and
     `project-agent-environment` (agents.yaml / agents.lock). Their
     requirements are extended here as a capability of its own. If either
     archives first, the deltas this change implies are folded into it at
     that time. -->

## Impact

- Depends on `project-rules`: the rule format, the validator, the engine
  reading `.agents/rules/`, and the guard keeping a package's rules out of
  machine-wide delivery.
- `uze-core`: the rule capability in a package, plus the projection of a
  locked package's rules into the checkout and its inspection.
- `uze-application`: the add, install, update, remove and status
  lifecycles gain the rules step.
- `plugins/uze`: `author` and `init` Skills. `uze agent plugin create`
  gains `--rules`.
- Conformance Lab and a journey: two projects, one declaring the plugin,
  and the rule fires only in that one.
