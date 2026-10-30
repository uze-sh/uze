## Context

See proposal.md for why. Today `agents.yaml` refuses any entry named after
the built-in marketplace (`manifest::validate`), `project_environment::add`
installs a built-in plugin on the machine and writes nothing, and the lock
has one marketplace shape: `LockedMarketplace { git, ref, subdirectory,
revision }`, a Git repository at a commit. The `agents-lock` spec already
names an `embedded` source, which was never built. The spec deltas here
modify requirements of `agents-manifest` and `agents-lock`, which live in
the open `project-agent-environment` change and reach `openspec/specs/`
when it archives; this change archives after it.

## Goals / Non-Goals

**Goals:**
- One official marketplace, `uze`, for the machine and for projects alike.
- A project declares a built-in plugin the way it declares any other, and
  every machine installs it from the binary it runs, the lock saying which
  bytes and which version that was.

**Non-Goals:**
- Linking the built-in marketplace to a checkout for live edits. `make
  try` installs this checkout's uze instead, which carries its `plugins/`.
- Pinning a built-in plugin across uze builds: the binary is the
  marketplace, and another build is another marketplace revision.

## Decisions

**The manifest spells a built-in marketplace as a source-less entry.**
`marketplaces: { uze: { plugins: [workspace] } }`. `validate` accepts an
entry under the built-in name only when it has no `git:`, `path:`, `ref:`
or `subdirectory:`, and refuses a source there by name. Alternative: a
top-level `plugins:` list for built-ins. Refused: the spec has no top-level
`plugins:` key, and a plugin's identity is always `name@marketplace`.

**`project_environment::add` declares a built-in plugin unless it is
bundled.** The early return for the built-in marketplace narrows to
`bootstrap::DEFAULT_PLUGIN_IDS`; any other built-in plugin goes the
declared road, its request resolved from the embedded snapshot.

**The lock pins the built-in marketplace by a top-level `uze:` version,
not by a marketplace entry.** `agents.lock` gains `uze: <version>`, written
only while a built-in plugin is locked, and each built-in plugin is an
ordinary `plugins:` entry (`marketplace: uze`, `integrity:`). No entry
goes under `marketplaces:`, so every reader of a locked marketplace (Git
mirrors, `answers`, lineage, rules, the workspace's facts) never meets one
that is not Git. Alternative, first drafted: `LockedMarketplace` as an enum
of a Git and a built-in shape. Refused after review: every caller of
`locked.git` would branch, and an untagged YAML enum loses the precise
"missing field" messages the malformed-lock requirement wants.

**Built-in plugins have an install phase of their own, and follow the
binary.** `stale_against` and the Git phases (`resolve_declared`,
`reproduce_locked`, `rise_to_lock`, `update`, rules) skip `uze` entries. A
dedicated phase compares, per declared built-in plugin, the binary's bytes
(`bootstrap::materialize`, digest) with the Store's: missing is installed,
different is replaced through `replace_with` (the Store keys embedded
provenance by id alone, so installing again would keep the old bytes),
equal is left; then the lock records the integrity and the running
version. A difference is never refused: the binary is the only source,
and `make install` rebuilds without a version bump, so "same version,
other bytes" is a developer's rebuild, not tampering. `add` shares the
phase's helper for one plugin.

**The retired name is moved at seeding.** Already built (`setup.rs`): a
package under `uze-official` is taken off with its receipts and installed
again under `uze`, once, before the defaults are seeded.

## Candidate ADRs

- A project may declare the built-in marketplace's plugins, pinned by the
  uze version: reverses a rule PR #23 recorded, and fixes what a lock
  means for a source that is the binary itself.

## Risks / Trade-offs

- [Two teammates on different uze versions rewrite the lock in turn] →
  each install reports the plugins as updated, so the churn is visible;
  the version in the lock says which binary wrote it.
- [A project declared `workspace@uze` and a newer uze drops or renames the
  plugin] → resolution fails naming the plugin, as a Git marketplace that
  dropped one does.
- [A built-in plugin's integrity cannot catch tampering with the binary]
  → the binary is trusted as the tool itself is; the integrity records
  which bytes a project ran, and a difference is reported as an update.
- [Specs this change modifies live in the open project-agent-environment
  change] → this change archives after it.
