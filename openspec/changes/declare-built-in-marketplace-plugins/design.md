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
  reproduces it the way it can: on the same uze version, byte for byte.

**Non-Goals:**
- Linking the built-in marketplace to a checkout for live edits. `make
  try` installs this checkout's uze instead, which carries its `plugins/`.
- Pinning a built-in plugin across uze versions: the binary is the
  marketplace, and another version is another marketplace revision.

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

**The lock's marketplace becomes two shapes.** `LockedMarketplace` turns
into an enum, untagged in YAML so each reads as the manifest does: `Git {
git, ref, subdirectory, revision }`, unchanged, and `BuiltIn { uze:
<version> }`, whose one field is the version that resolved it. Every
caller that read `locked.git` matches on the shape; only `Git` reaches the
Git transport. Alternative: optional `git`/`revision` plus an optional
`version` on one struct. Refused: it lets a lock say "a Git marketplace
with no commit", which the module's first rule forbids.

**Another version re-resolves; the same version verifies.** Installing a
locked built-in plugin compares the entry's version with the running one.
Equal: the bytes are checked against `integrity` as any pin is. Different:
the plugin is installed from this binary, the lock rewritten with this
version and the new integrity, and the report lists it as updated.
Alternative: fail, asking for `uze update`. Refused: a teammate on another
uze version could never install the project, and nothing they could
fetch would satisfy the pin.

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
- [The `LockedMarketplace` enum touches every lock reader] → the compiler
  finds each one; the YAML of an existing Git entry is unchanged, so no
  lock on disk needs rewriting.
