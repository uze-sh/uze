## Why

A plugin is read by an agent and an extension is run by the workspace, yet
both ship as one package: `extensions/` inside a plugin (`extension-platform`
D11). The product already works against that shape. `PackageKind::of` guesses
that a package is an extension because it reaches no harness, `delivers_to`
holds such a package back, and `plugins/workspace` is a `plugin.json` no agent
reads, wrapping three unrelated extensions. No package in this repository's
marketplace serves both consumers, and themes would be a third thing forced
into the same envelope. UZE is in beta with a handful of users, so moving the
identity costs little now and much more later.

## What Changes

- `marketplace.json` lists each kind on its own: `plugins[]` beside
  `extensions[]`, either optional. Themes are the next kind this shape is
  meant to hold; they are not built here.
- **BREAKING** A plugin no longer carries extensions. A plugin's `extensions/`
  is not read, and its check says where the extension belongs.
- **BREAKING** An extension's identity becomes `<id>@<marketplace>`. Consent,
  secrets, preferences and key bindings recorded under the old identity are
  not carried: each extension is asked about again.
- **BREAKING** `workspace@uze` leaves the marketplace; `gsd`, `focus` and
  `contributions` become three entries of `extensions[]`.
- `uze add`, `remove` and `update` keep one spelling. The catalogue says what
  a name is, and a name is unique within its marketplace across kinds.
- `agents.yaml` gains `extensions:` beside `plugins:` under each marketplace,
  and `agents.lock` pins what it declares. Declaring an extension offers it;
  only the operator's grants let it run.
- An extension may name the plugins it needs. The reference is flat and
  one-way: no versions, nothing transitive, nothing installed for it, and a
  gap the operator can read. An MCP grant always names its plugin.
- **BREAKING** Authoring gets its own noun: `uze agent extension
  create|check|lsp` replaces `uze agent plugin create --extension`, and the
  extension parts of `plugin check` and `plugin lsp`. The `uze:author` Skill
  stays the one guide across kinds.

## Capabilities

### New Capabilities

- `marketplace-surfaces`: what a marketplace offers by kind, how a name
  resolves to a kind, what a project declares, and what an extension may say
  it needs.

### Modified Capabilities

- `marketplace`: the manifest a marketplace is validated against holds
  `extensions[]`, and `plugins[]` stops being mandatory.
- `extension-views`: an extension is no longer inside a plugin, its
  attribution names its new identity, and scaffolding moves to the new noun.
- `plugin-authoring`: a marketplace check covers `extensions[]` entries.

## Impact

- `uze-core`: the marketplace manifest, acquisition, `UzeHome` (extension
  bytes under `sources/`), the project manifest and lock. The extension
  decoder leaves `capability/`.
- `uze-workspace`: takes the extension domain, and re-keys `state/extensions.json`.
- `uze-application`: `Extensions::installed`, the MCP lookup, `PackageKind`,
  `reaches_a_harness` and `delivers_to`.
- `src/cli/`, the Plugins and Extensions screens, the `uze:author` and
  `uze:try` Skills, `web/` and its docs, `journeys/`, and the `plugins/workspace`
  tree.
- In-flight `extension-*` changes whose delta specs still say
  `plugin@marketplace`: see design.md, Risks.
