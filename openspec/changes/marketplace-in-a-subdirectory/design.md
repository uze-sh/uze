## Context

See [proposal.md](proposal.md). `parse_source`
(`uze-application/.../marketplace.rs:351`) keeps the typed subdirectory as
`PackageSource::Local { path }`; `repository_of`
(`uze-core/.../acquisition/marketplace.rs:56`) normalizes to the toplevel;
`MarketplaceRequest::of` sets `subdirectory: None` for local sources
(`marketplace.rs:135`); `materialize_plugin` reads `marketplace.json` at the
root (`:254`); `state::marketplace_link` stores the toplevel (`state.rs:365`).

## Decisions

### Subpath beside identity, never in it

Identity stays the repository (URL-shape, PR #106): two marketplaces in one
repository are two records with one identity and different subpaths, and
the mirror is shared. The subpath is a normalized relative path (no `..`, no
absolute), computed from the typed path against `--show-toplevel`, or from
`url#sub` for remote locators, which already parse it.

### One resolver for "where is the catalogue"

Every reader (`materialize_plugin`, `materialize_from_link`, catalogue
`refill`/`adopt`, freshness `describe_path`) asks one function for the
catalogue path and the plugin path from `(repository, subpath)`; this is the
function that keeps a future reader from reintroducing the root.

### Record shape

The marketplace record and the link gain `subpath`, absent meaning the root,
so an older shape reads as root without a ladder rung; a newer record is
never taken by an older build (uze-document rule).

## Risks / Trade-offs

- [Plugin `source` pointing outside the subpath (`../shared`)] → refused like
  any source escaping its marketplace; a monorepo that shares code places it
  under the marketplace directory.
