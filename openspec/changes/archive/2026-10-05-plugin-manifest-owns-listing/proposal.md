## Why

A plugin's `description` and `keywords` are written twice, once in the
marketplace's `plugins[]` entry and once in the plugin's own `plugin.json`,
and uze reads only the first. The two copies have already drifted in this
repository's own `uze` plugin, with nothing to notice, and the Agent
Plugins 1.0 standard says no other file may replace, supplement or override
the core fields of `plugin.json`.

## What Changes

- The plugin listing (`plugins_offered_by`, the plugins tree and drawer,
  `market inspect`, the JSON reports) reads `description` and `keywords`
  from each plugin's `plugin.json`: in place for a local marketplace,
  from the mirror at the catalogue's commit for a Git one, from the
  embedded snapshot for the official one.
- A `marketplace.json` entry carries `name`, `source` and an optional
  `category`, and nothing that describes the plugin. **BREAKING** for
  marketplaces that relied on an entry's `description`/`keywords`: they are
  ignored, so such a plugin lists without a description until its
  `plugin.json` carries one.
- `uze agent market check` warns on an entry still carrying `description`
  or `keywords`, with the action: move it to `plugin.json`, delete it
  because `plugin.json` already says the same, or keep one of two values
  that differ.
- `uze agent plugin create --description` writes the description to
  `plugin.json` only.
- The official `uze` marketplace becomes the reference for the split: its
  `plugin.json` carries the description, keywords and the standard's
  `author`, `homepage`, `repository` and `license`; its entry carries
  `name`, `source` and `category`; and a test keeps its `market check`
  free of findings and warnings.
- The Lab's and the test fixtures move their descriptions and keywords
  into `plugin.json`; the `uze:author` Skill and the plugin-format docs
  say which file owns which field.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `marketplace`: the manifest's entry shape, and where a plugin's listing
  is read from.
- `plugin-authoring`: the scaffold writes the description to `plugin.json`
  only, and `market check` warns on describing fields left on an entry.

## Impact

- `uze-core`: `MarketplacePluginEntry` loses `description`/`keywords`; a
  lenient reader of a plugin's listing fields; `check_marketplace`
  warnings; `scaffold_plugin`.
- `uze-application`: the catalogue reads plugin manifests (one
  `git cat-file --batch` per mirrored catalogue, memoized with it);
  `bootstrap` reads the embedded `plugin.json` files.
- Data: `marketplace.json` and `plugins/uze/plugin.json`,
  `conformance/_fixtures/marketplace`, `conformance/_fixtures/parity/uze`,
  `tests/_fixtures/golden/marketplace`. The vendor-owned
  `.claude-plugin/marketplace.json` fixture is unchanged.
- Docs: `web/content/docs/reference/plugin-format.mdx`, the `uze:author`
  Skill. ADR-032's schema line is superseded by the ADR this change writes
  when it is archived.
