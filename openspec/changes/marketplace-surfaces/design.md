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

## Proposed Decisions

Each item is a recommendation awaiting the operator's call, so nothing here is
settled until it moves into the Decisions above. `tasks.md` waits for them.

### P1. One `uze add` for every kind (confirms D2)

The catalogue already knows the kind, and `add` keeps one meaning. A noun per
kind would repeat that on every verb. Only authoring gets a noun, because its
inputs differ.

### P2. Do not move the extension domain in this change (revises D5)

Only `package/authoring` reaches `capability::extension` inside `package/`,
`delivery/` and `project/`, while 9 files outside `uze-core` name it. What the
design needs is that the package manager stops calling it, which a layering test
can prove. The physical move to `uze-workspace` is a behaviour-free refactor that
can follow on its own, and it should not share a review with an identity change.

### P3. No bundle; a need is one press away (confirms D6)

Nothing mixes the two kinds today: `ai` has no `extension.kdl`, and on this
machine only `workspace@uze` carries extensions. The gap in the extension's
drawer offers `add <plugin>`, so the person still chooses. A need naming a
marketplace that is not registered says to register it first, because a
reference holds no URL.

### P4. A theme is machine-scoped, whatever the directory (confirms D8)

`uze add nord@uze` inside a project writes nothing to `agents.yaml`, and its
report says `on this machine only`, as ADR-054 requires. A palette is a person's
preference, and a team's would overwrite it on every clone.

### P5. One rung, and no notice (confirms D9)

A `state/extensions.json` rung drops old-identity records, `Lasts::Unbounded`
since it only runs when the record is next written, and retired entries cover
secrets and preferences. The extension is offered again on its own, so no
migration message is needed. The release notes name the `agents.yaml` edit.

### P6. Clear the in-flight changes first, in their own pull request

20 `extension-*` and `audit-*` changes are complete and can be archived now,
as #348 did. Two need a look: `extension-platform` has 8 open tasks in section
26, follow-ups that belong in changes of their own, and
`project-agent-environment` has 18 and carries the `agents-manifest` delta that
D8 extends. Doing this first lets this change's deltas modify
requirements that exist.

### P7. The item says the minimum uze it needs; nothing searches for an older one

Compatibility is the real cost of the split. `protocol` does not cover it: the
check refuses unknown nodes, so any new catalog node breaks an older binary
without a protocol bump.

- `extension.kdl` gains `requires-uze "1.2.0"`, read from the item at the
  catalogue's revision, as a plugin's listing already is.
- A fresh install on an older binary is refused with `uze upgrade` as the next
  step. An update it cannot run is held at the installed revision, and shown.
- `uze agent market check` fails an entry using something newer than its
  `requires-uze`, so the market's CI keeps the claim true.

*Rejected:* tags or branches per protocol, and walking the mirror's history for
the newest revision that fits. Both exist to serve old binaries, and upgrading
is one command. Themes need none of this: a partial theme is completed from the
default.

### P8. The optional marketplace is also named `uze`: two origins, one name

This replaces a new name such as `gsd@market`. The repository is
`uze-sh/market`, and its `marketplace.json` says `"name": "uze"`.

- Catalogue of `uze` = what the binary embeds (Bundled) + what the mirror offers
  (Official). These are the two seals the design system already defines.
- Identities stay `gsd@uze`, and a project keeps writing `uze:` with no source.
  `agents.yaml` still refuses a source there, so nothing can shadow it.
- A name the build bundles is refused in the market by `market check`, and the
  embedded one wins if it ever happens.

*Cost:* `uze` stops being purely embedded, and the code mentions the built-in
marketplace or an embedded source on 57 lines. Each lock entry must say whether
it pins a uze version or a commit. Working on a checkout of the market needs a
way to link it under the reserved name.

### P9. Nothing to pre-register

This follows from P8: the source is a constant in the binary. Without a network
the listing shows what is bundled plus the last catalogue it read. The first read
happens when the Marketplace opens or a `uze add` asks, then on the hourly fetch
an open workspace already runs.

### P10. Themes become a kind here, reversing a non-goal above

- Bytes go in `sources/themes/<marketplace>/<id>/`. `themes/` stays the operator's
  authored tier, and its files appear in a `themes/` group marked as theirs.
- `[appearance] theme` keeps a bare id for bundled and authored themes and takes
  `<id>@<marketplace>` for the rest.
- A selected theme that is not installed draws in `adaptive`, and `uze doctor`
  and the Settings screen say so. No toast, since the first frame must be right.
- The palette can be shown before installing, read from the catalogue's revision.
- `ascii` and `nerd` stay in the build: they are glyph sets the chrome depends on.

### P11. `pingpong` stays in this repository, outside any catalogue

It is proof of delivery and an authoring example, not something a person would
choose to install: the root crate builds its MCP server, and `tests/` and
`make try` use it. It moves to `tests/_fixtures/`, is installed by path, and the
market does not offer it.

### P12. `uze:try` stays bundled

It is the way in for someone with nothing installed, and it can be read offline.
It says so plainly if the market cannot be reached. Moving it would hide the
door behind what it exists to introduce.

### P13. Proof on both sides

- This repository: a small fixture extension and theme under `tests/_fixtures/`
  replace what the suites read from `plugins/workspace`. A nightly journey installs
  from the real market at a pinned commit, so a break is seen without blocking
  a pull request.
- The market: CI runs `uze agent market check` with the newest release, plus
  each entry's `requires-uze` as stated.
- History moves with `git filter-repo`. The market needs its own licence and CLA
  check. The example GIFs move with their extensions.

### P14. Two changes, in this order

1. `marketplace-surfaces` (this one): catalogue, identity, the theme kind, the
   Marketplace screen, all served from this repository.
2. `market-repository`: create `uze-sh/market`, add the second origin, move the
   bytes.

The first can ship and be lived with. The second then moves bytes to a format
that has already settled, and each can be reverted alone.

### P15. The Marketplace screen (interactive prototype shared with the operator)

- Tabs lose Extensions: `Overview · Marketplace · Integrations · Shortcuts ·
  Settings`.
- Nothing folds. A menu column, the Overview's shape, lists the kinds
  (`plugins`, `extensions`, `themes`) and, under `from`, `all` and each
  marketplace. Choosing a marketplace narrows the grid and puts its source and
  actions in the drawer, so it needs no view of its own. With `all`, a marketplace
  is a section's heading.
- Everything is a card from `view::catalog`, with today's anatomy, and each kind
  says what it is. A plugin's foot counts what it brings. An extension's says
  where it stands, and its drawer draws the workspace with those areas lit. A
  theme's card stands on the theme's own colours, and `p` tries the theme on the
  whole screen until the cursor moves.
- A filter `all · installed · updates` stands over the grid.
- The screen answers both "what can I get" and "what runs", which replaces the
  design system's "Marketplace says what a package brings; Extensions says what
  runs". That rule existed because a package carried extensions. It and the
  "a package that carries only extensions" wording change with this.
- Below about 130 columns the menu lies across the top, and the drawer stays.
- Settings keeps choosing among installed themes and links to more.

*Rejected:* keeping the Extensions tab beside a new view, which leaves two places
named the same; a tree of marketplaces and kinds, which folds and makes "which
themes can I get" a walk through every marketplace; and a plain list per kind,
which loses what the current cards show at a glance.

*Cost:* a plugin's resources stop being rows of their own. They are read in its
drawer, so a single skill no longer has a detail panel.

## Open Questions

- Should `uze status` group by kind, or keep one list with a kind column? The
  design-system skill decides this when the report is written.
- Does the Plugins screen split into tabs by kind or filter one catalogue? Same
  owner, same moment.
