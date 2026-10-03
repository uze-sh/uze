## 1. Reading a plugin's listing

- [x] 1.1 Add a lenient reader in `uze-core` that turns a `plugin.json`'s bytes into its `description` and `keywords`, absent for missing bytes, non-JSON or a wrong type; unit-test each case
- [x] 1.2 Remove `description` and `keywords` from `MarketplacePluginEntry`; prove a manifest carrying them still parses

## 2. Catalogues carry listings

- [x] 2.1 Give `Catalogue` its `listings`, read in place for a local marketplace
- [x] 2.2 For a Git marketplace, read each plugin's `plugin.json` at the catalogue's commit on refill and record the listings in `catalogue.json`; fill and write back an entry that has none on its next read
- [x] 2.3 Read the official marketplace's listings from the embedded table in `bootstrap`
- [x] 2.4 Build `MarketplacePluginSummary.description`/`keywords` from the listings in `plugins_offered_by`
- [x] 2.5 Test: editing only a linked plugin's `plugin.json` changes the listing; a cached Git catalogue lists from `plugin.json` with the repository gone; a broken `plugin.json` lists its plugin without a description and the rest as usual

## 3. Authoring

- [x] 3.1 `scaffold_plugin` writes the description to `plugin.json` only; the entry carries `name`, `source` and `category`
- [x] 3.2 `check_marketplace` warns per entry field (`description`, `keywords`) with the move / delete / reconcile action; test the three
- [x] 3.3 Update the `uze:author` Skill: `--description` lands in `plugin.json`, the check's warnings are acted on, and how to bring an existing marketplace over

## 4. Data and docs

- [x] 4.1 Make `plugins/uze/plugin.json` the reference manifest: the newer `description`, the `keywords`, and the standard's `author`, `homepage`, `repository` and `license` (`Apache-2.0`)
- [x] 4.2 Reduce the official `marketplace.json` to `name`, `owner`, and per entry `name`, `source` and `category`
- [x] 4.3 Guard test: `check_marketplace` on the repository root (the official marketplace the binary embeds) has no findings, no warnings, and is Agent Plugins 1.0 conformant; the embedded listing shows `plugin.json`'s description
- [x] 4.4 Do the same in `conformance/_fixtures/marketplace`, `conformance/_fixtures/parity/uze` and `tests/_fixtures/golden/marketplace`; leave `.claude-plugin/marketplace.json` alone
- [x] 4.5 Update `web/content/docs/reference/plugin-format.mdx`: which file owns which field

## 5. Gates

- [x] 5.1 `cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`, `cargo test --workspace --no-fail-fast`, `openspec validate plugin-manifest-owns-listing --strict`
