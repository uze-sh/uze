## Context

See proposal.md for why. What the code holds today:

- `MarketplaceManifest` has a mandatory `plugins` and no
  `deny_unknown_fields`, so an older build should ignore another top-level
  list (not yet proven by a test). `agents.yaml` is the opposite: it sets
  `deny_unknown_fields`.
- `Identity { marketplace, plugin, id }` keys consent, secrets, preferences
  and `ext.<plugin>@<marketplace>/<id>.<action>`.
- `Extensions::installed` walks installed plugins and discovers
  `extensions/*/extension.kdl`, reading a linked marketplace's working tree.
- `uze-core::capability` decodes extensions even though the core's doc says a
  capability is what a plugin declares and how it reaches a harness. Consent
  already lives in `uze-workspace::extensions`.
- `mcp "rtc" plugin=...` is optional, and omitting it means the extension's
  own plugin (`extensions/mcp.rs`).
- `trust.rs` asks whether a package may introduce code a harness executes.
  ADR-041 made running extension code a separate question, answered by grants.

```
                marketplace (a Git repository)
                marketplace.json
                        |
       +----------------+----------------+
       v                v                v
   plugins[]       extensions[]      themes[] (later)
   plugin.json     extension.kdl     theme file
       |                |                |
       +-------+--------+----------------+
               v
   SHARED: registry, mirror, access, freshness, acquisition,
           name rule, lock, update
               |
       +-------+--------+----------------+
       v                v                v
   Engine/Router    grants, enabled     appearance
   receipts         per project         selection
       |                |                |
       v                v                v
     AGENT             TUI              TUI
```

## Goals / Non-Goals

**Goals:**

- The catalogue says what a thing is; nothing infers it from a package's
  contents.
- An extension installs, updates and is pinned without the delivery pipeline
  ever seeing it.
- The next kind adds a list, a manifest and a consumer, and leaves the shared
  half alone.

**Non-Goals:**

- Building themes, or a theme catalogue.
- A dependency resolver, version constraints or bundles.
- Changing the extension contract: `extension.kdl`, views, scripts, grants and
  the `Host` boundary stay as they are.
- Carrying an earlier identity's consent forward.
- A format other harnesses read: `marketplace.json` stays UZE's own (ADR-032).

## Decisions

### D1. Kinds are sibling lists in `marketplace.json`

```json
{
  "name": "uze",
  "plugins":    [{ "name": "uze",  "source": "./plugins/uze" }],
  "extensions": [{ "name": "gsd",  "source": "./extensions/gsd", "category": "productivity" }]
}
```

The entry keeps the shape of a plugin's: `name`, `source`, optional `category`.
The entry's `name` must equal the manifest's `id`, because the identity is
spoken in both places and two spellings would be a question with no answer.

*Alternatives:* a `type` field on one list reads well for tools and badly for
a person, and an older build would install an extension as a plugin. A second
file (`extensions.json`) duplicates the marketplace's identity. An `extension`
field in `plugin.json` keeps the envelope this change removes.

### D2. One verb; the catalogue resolves the kind

`uze add gsd@uze` looks the name up across the marketplace's lists. A
marketplace offering one name under two kinds is refused at `market add` and
by `market check`, so the lookup never needs a disambiguating flag, and the
report says what was added.

*Alternative:* `uze ext add` / `uze theme add`. It repeats on every verb a
distinction the catalogue already holds, and `add` keeps one meaning (declare
and install) either way. A person thinks "that thing from that marketplace".

Authoring is different: its reader knows what they are writing, and the
verbs take different inputs, so authoring has a noun per kind (D7).

### D3. The identity is `<id>@<marketplace>`

`Identity` drops `plugin`. `ext.<id>@<marketplace>.<action>` and the host's
attribution follow. A built-in extension stays a bare id, and reserved ids stay
reserved.

This does not reuse `PackageId`: an extension and a plugin never share a
namespace on disk, only a name rule and a uniqueness check.

### D4. Extension bytes live beside plugin bytes, not in the store tier

`sources/extensions/<marketplace>/<id>/`, named in `UzeHome`. The tier fits:
`sources/` costs "the packages, until they are acquired again", while `store/`
is "what each harness reads", which an extension never is. A linked marketplace
is still read from its working tree.

A new `state/` ledger is not assumed: what is installed is the directory, as
with plugins, until implementation shows that provenance needs a record.

### D5. The extension domain moves to `uze-workspace`

`capability/extension` leaves `uze-core` for `uze-workspace::extensions`, where
consent already is. The package manager then names no extension concept beyond
"a catalogue entry of kind extension, whose bytes are acquired to this path".
`uze-core` still does not depend on `uze-workspace`, so the compiler holds it.

*To validate while implementing:* `doctor`, `machine/extension_program.rs` and
`machine/mcp.rs` reach this module today, and `uze-workspace` needs `uze-contract`.

### D6. A dependency is a flat, one-way reference

```kdl
needs "std@cardinal"
mcp "rtc" plugin="std@cardinal" timeout="5m"
```

- `needs` names a plugin; `mcp ... plugin=` is now mandatory and implies it.
- Only an extension says it. A plugin cannot need an extension, since an agent
  has to work without the workspace.
- It holds no version and is not transitive, so nothing is resolved: it is one
  lookup against what is installed.
- UZE never installs what is named. Like `plugin-requirements`, a gap is
  reported with the command that closes it, and the extension still installs.
- A surface whose need is unmet is drawn as such, and its grant call answers
  with an error, instead of failing silently.

Prompting the agent in words needs no reference at all. `gsd` works that way:
GSD is the operator's own install.

*Alternatives:* a real dependency graph is machinery for a problem no entry has
yet. Bundling the two into an installable product is the envelope again; it can
be added later as a catalogue-level concept if a real case asks.

### D7. Authoring is `uze agent extension create|check|lsp`

`create <id> --market <name>` writes `extensions/<id>/` and its catalogue entry;
`check <path>` takes `--run` and `--update-snapshots`; `lsp` moves over. The
verbs live under `uze agent`, an audience and not a category, each classified in
`command_performance.rs`. The `uze:author` Skill stays the single guide, with a
section per kind.

### D8. `agents.yaml` declares extensions beside plugins

```yaml
marketplaces:
  uze:
    extensions: [gsd]
  ai:
    git: https://github.com/hiukky/ai
    plugins: [git, code]
    extensions: [review-board]
```

The source and `ref:` above already say where each comes from and what moves
it. `agents.lock` gets a digest per extension. `uze install` converges the
declaration both ways as it does for plugins. Declaring offers: the project's
extension offer dialog stays what lets one run (ADR-041, ADR-055). A cloned
project still cannot run anything by declaring it.

Themes will not take this path. A theme is a person's preference, so when that
kind arrives it installs on the machine and `agents.yaml` does not name it.

### D9. No carry-over, one rung

The operator decided the old identity is not migrated. Under the
`earlier-releases` rule that still needs a declared entry, not silence: a rung
of the `state/extensions.json` ladder that drops records keyed by the old
identity, and likewise for secrets and preferences. An installed `workspace@uze`
is left to the behaviour of an entry its marketplace stopped offering, and a
project declaring it gets that error from `uze install`. The release notes name
the edit.

## Candidate ADRs

- An extension is a marketplace kind of its own, not a plugin capability: it
  supersedes `extension-platform` D11 and sets how every later kind arrives.

## Risks / Trade-offs

- [In-flight `extension-*` changes carry delta specs saying
  `plugin@marketplace`, among them `extension-platform`,
  `extension-manifest-and-scripts` and `extension-tests-and-lsp`] → Archive
  those first, so this change's deltas modify requirements that exist in main.
  Today's delta set assumes only what `openspec/specs/` holds.
- [The extension domain's move touches many files at once] → Split it into its
  own commit with no behaviour change, ahead of the identity.
- [An author who wants a skill and its panel now publishes two entries] → The
  reference says how they relate, and the operator adds both. A bundle stays
  possible later as a catalogue concept.
- [A user's `agents.yaml` with `extensions:` read by an older build fails on
  the unknown field] → Accepted: that refusal is the file's standing rule, and
  it names the field.
- [Third-party plugins carrying `extensions/` stop showing them] → The plugin
  check names where each goes. This repository's marketplace has none left;
  third-party marketplaces have not been surveyed.

## Migration Plan

1. Move the domain to `uze-workspace` with no behaviour change.
2. Teach the catalogue, manifest and lock the new list, reading both shapes.
3. Switch the identity and the discovery, and retire plugin-carried extensions
   with D9's rung.
4. Move the `plugins/workspace` tree to `extensions/`, and update the docs,
   Skills and journeys.

Rollback before release is a revert. After it, a machine keeps its
`sources/extensions/` bytes as inert files, and consent is asked again.

## Open Questions

- Should `uze status` group by kind, or keep one list with a kind column? The
  design-system skill decides this when the report is written.
- Does the Plugins screen split into tabs by kind or filter one catalogue? Same
  owner, same moment.
