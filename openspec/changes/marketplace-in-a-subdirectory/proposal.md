## Why

A team keeps its plugins in a directory of a larger repository
(`<repo>/aikit/marketplace.json`). UZE registers it — `market add` reads the
manifest in place — and then cannot install from it: a linked marketplace
reads `<toplevel>/marketplace.json` (`failed to read <repo>/marketplace.json`)
and a mirrored one reads `git show <commit>:marketplace.json` at the root
(`path 'marketplace.json' does not exist`). The only workaround is to move
the catalogue to the repository root, which a monorepo cannot always do.

The subpath is half there: the locator grammar accepts `url#sub` and
`PackageSource::Git.subdirectory` stores it, but the marketplace request
never reads it, a local source has no subpath at all, and linking stores the
toplevel.

`market add <path>` also prints `Added marketplace from https://gitlab.com/…`
for a local checkout — the remote's URL, its identity — which reads as if the
remote were what gets read.

## What Changes

- **A marketplace may live in a subdirectory of its repository.** Its
  identity stays the repository (URL-shape identity, PR #106) plus a
  normalized relative subpath; the catalogue and every plugin `source` resolve
  under that subpath, for mirrored and linked reads, in the lock and in
  freshness.
- **`market add <repo>/<sub>` records the subpath** it was given, and
  `market link <name> <repo>/<sub>` keeps it.
- **`market add` says what it will read**: the identity, and for a local
  source the checkout and subpath, and whether it is linked or mirrored.

## Capabilities

### New Capabilities

- `marketplace-subdirectory`: a marketplace catalogue located below its
  repository's root, read the same way for every source kind.

### Modified Capabilities

<!-- none -->

## Impact

- `crates/uze-core/src/package/acquisition/{marketplace,forge,mirror}.rs`:
  subpath on the marketplace source and on `repository_of`'s result.
- `crates/uze-application/src/application/{marketplace,marketplace_catalogue}.rs`:
  `materialize_plugin`, `materialize_from_link`, `refill`/`adopt`/`manifest_at`
  resolve under the subpath.
- `state::marketplace_link` stores the subpath with the checkout; the
  marketplace record gains a field (record tier: additive, the ladder carries
  shapes without it as "root").
- `agents.lock` marketplace entries carry the subpath.
- `src/main.rs:1800` and `src/ui/worker.rs:522` report text.
