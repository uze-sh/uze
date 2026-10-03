## Context

See [proposal.md](proposal.md). Before this change, `parse_source` (in
`uze-application`'s `application/marketplace.rs`) kept the typed
subdirectory as `PackageSource::Local { path }`; `repository_of`
(`uze-core`'s `acquisition/marketplace.rs`) normalized it to the toplevel;
`MarketplaceRequest::of` set `subdirectory: None` for local sources;
`materialize_plugin` read `marketplace.json` at the repository root; and
`state::marketplace_link` (`uze-core`'s `delivery/state.rs`) stored the
toplevel.

## Decisions

### Subpath beside identity, never in it

Identity stays the repository (URL-shape, PR #106): two marketplaces in one
repository are two records with one identity and different subpaths, and
the mirror is shared. The subpath is a normalized relative path (no `..`, no
absolute), computed from the typed path against `--show-toplevel`, or from
`url#sub` for remote locators, which already parse it.

### One resolver for "where is the catalogue"

Every reader (`materialize_plugin`, `materialize_from_link`, catalogue
`refill`/`adopt`, freshness `offered_revision`) asks one type,
`MarketplaceSubpath` in `uze-core`'s `acquisition/marketplace.rs`, for the
catalogue path (`manifest_path`), the plugin path (`plugin_path`) and, for a
working tree, the catalogue directory (`directory_in`). Keeping that answer
in one place is what keeps a future reader from reintroducing the root.

### Record shape

No record gains a field. The registered source already carries the subpath
(a local `path` is the marketplace directory itself, a Git source its
`subdirectory`), and the link keeps recording the checkout's top level: a
linked read is `<checkout>/<registered subpath>`. Linking the checkout or
its marketplace directory is therefore the same link, and linking another
directory of the same repository is refused as a different marketplace.
Keeping the subpath in one place is what keeps two copies of it from
disagreeing, and the marketplace registry stays at shape 1.

## Risks / Trade-offs

- [Plugin `source` pointing outside the subpath (`../shared`)] → refused like
  any source escaping its marketplace; a monorepo that shares code places it
  under the marketplace directory.
