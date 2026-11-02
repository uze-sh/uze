## Context

Every path UZE owns is composed by a builder on `UzeHome`
(`crates/uze-core/src/machine/home.rs`) and listed with its tier, form and
standing in `UzeHome::owned_paths()` (`home/owned.rs`);
`every_path_uze_owns_is_named_in_the_map` and
`every_disposable_root_has_one_collector` hold the map and the collectors to
each other, and the health catalogue (`uze-application`'s `catalogue.rs`)
names one subject per entry. So a rename is a change to a builder, its
entry, its catalogue subject and the callers that spell the old name in a
test, a journey or a conformance seed.

What the change starts from, observed on a working machine and in code:

- `runtime_tenants` (`home.rs`) does not list `runtime/agents`, so the
  `Projections` collector deletes `runtime/agents/<a>/reports.{jsonl,lock}`
  on every `doctor` and first occupancy pass, while an agent holds the lock
  (`uze-workspace/src/reports.rs`).
- Nothing writes `runtime/attachments/` since the vendor store (#345):
  builds stage beside their destination. What still names it is cleanup of
  what earlier builds left there.
- `runtime/packages/` is live: `delivered_root::materialize` copies a
  package there for an extension's MCP server.
- `compiled_extensions_dir` is `runtime/extensions` itself, and that
  directory is the tenant sheltering `scratch/`, `pictures/` and
  `costs.json`.
- `setup-<harness>.log` is composed in `src/cli/setup.rs`, outside the map;
  the map test does not anchor on `logs_dir()`.
- The installed `uze` and every worktree's `target/debug/uze` share
  `~/.uze` daily.

## Goals / Non-Goals

**Goals:**
- One rule for every name: one word, context from the directory, no file
  name in two tiers, no file beside a directory of its stem, a guard
  sharing the stem of what it guards.
- The root holds only the operator's text: `config.toml`, `keys.json`,
  `themes/`.
- Every rename carried across from the floor release, through its registry.

**Non-Goals:**
- The top-level tiers, `sources/`, `store/` and `state/terminal/`.
- Names a harness or a package format dictates (`plugin.json`,
  `.claude-plugin/marketplace.json` in the store, `SKILL.md`): the rule is
  about names UZE chooses.
- Extension preferences moving into `config.toml`: another change owns
  that; this one writes the shape rules it lands in.
- Settings showing `[memory] native`: a later UI change.

## Decisions

### D1. The final layout

```
~/.uze
├── config.toml  keys.json  themes/
├── state/
│   ├── packages.json  attachments.json  undelivered.json  provisioning.json
│   │   marketplaces.json  hosts.json  install.json  layout.json
│   ├── mutation.lock  config.lock  fetch.lock  reports.lock
│   ├── release.json/.lock   launches.json/.lock   usage.json/.lock
│   ├── mirrors/<entry>.lock
│   ├── extensions/ consent.json/.lock  store/  preferences/  secrets/
│   ├── memory/ proposed/
│   ├── projects/<p>/ project.json  tasks.json/.lock  approvals.json
│   │                 prompts.json/.lock  conversations/ + .lock
│   │                 reviews/ + .lock  delivery.lock  agents-md.lock  memory/
│   └── terminal/
├── runtime/
│   ├── projects/<p>/ root.json  agents/<a>/  <vendor>/
│   ├── agents/<a>/reports.jsonl
│   ├── servers/<package>/
│   ├── sessions/<vendor>/  rules/sessions/
│   └── extensions/ compiled/<digest>.json  scratch/  pictures/  costs.json
├── shims/<harness>
└── cache/
    ├── harnesses.json  detection.json  requirements.json  shell.json
    │   overview.json  transcripts.json  inspection.json  appearance
    │   collection.json  changelog.md  marketplaces/<entry>/
    └── logs/ <name>.<date>.log  setup/<harness>.log  gate/<agent>.log
```

### D2. Locks stay in their scope's directory; there is no `locks/`

A guard sits beside the document it guards with its stem; an activity lock
is named for the activity in the directory of its scope (`state/` for the
machine, `projects/<p>/` for a project). A `state/locks/` was considered and
refused: #267 removed that directory because it outlived the projects whose
locks it held, and moving `mutation.lock` or `config.lock` would leave the
installed and the dev binary without exclusion until both are rebuilt.
`fetch.lock` and `mirrors/<entry>.lock` are renamed anyway: no two builds
mutate the store through them, and a lost exclusion there costs one
duplicate fetch.

`reports.lock` becomes one machine-wide lock in `state/`, sharing the stem
of the `reports.jsonl` it guards: an append and an occasional halving are
short, and one per agent would leave a file behind for each.

### D3. `runtime/agents` is a tenant with a collector keyed on recorded agents

Moving the lock is not enough: the sweep also deletes live
`reports.jsonl`. `runtime/agents` joins `runtime_tenants`, and the
workspace registers one `Collectable` for it that removes the directory of
an agent no project records any more.

### D4. Every rename is carried across through the floor's registry

Each old path becomes a `retired(..)` entry of the owned map with
`.carried_by(step)`, declared through `earlier-release-support`'s registry
and run where the move it replaces would run: a machine-wide record from
`MutationLock::acquire`, a project's records from `record::ensure`. A
lazily carried entry is `Unbounded`; a disposable one `Until(Beta(22))`. A record step moves the file (and nothing else: the shape is
unchanged, so no ladder rung is added); a lock is not carried, its old name
is retired and swept. The two authored moves are carried the same way:
`theme-overrides.json` is written once into `[appearance.*]` with
`toml_edit` and then removed, and `[memory] harness` is renamed in place to
`[memory] native`; a `config.toml` that does not parse stops the carry and
is reported, never written over. Moving every record a project holds goes
through one step per project directory, so a project is never half moved.

`runtime/attachments` is not written since #345, but `1.0.0-beta.22` still
writes it: its cleanup stays, as an entry of the integrations' `earlier`
modules.

### D5. `config.toml` shape

One section per concept (`appearance`, `notifications`, `memory`,
`projects`, `extensions`); at most two header levels (`[section]`,
`[section.<id>]`), deeper structure as dotted keys inside the table; one
table per identified thing (a built-in extension by its bare id, a plugin
by `"plugin@marketplace"`, a project by its path), the `@` telling the two
apart without a `builtin` level; vocabulary ids kept as they are, quoted
when they hold a dot; tolerant read and surgical write as today; no
secrets. The rule is written into `persistence-tiers` under the authored
tier.

TOML was kept over YAML and KDL: `toml_edit` already writes the file
preserving comments; YAML would widen `noyalib`, a written exception kept
to one file; KDL reads better nested, but what the file holds is shallow.

### D6. Theme overrides are a value, not a file

`[appearance.colors]`, `[appearance.symbols]` and `[appearance.syntax]`
are the `ThemeFile` fields an override layer may set; `[appearance] glyphs`
is the same glyph-set concept the theme file's `glyphs` names. `uze-theme`
gains a resolve from an already-read value, so it still resolves no path;
`src/theme.rs` builds the override layer from `config.toml` and names
`config.toml [appearance.<field>]` in a problem. `theme_overrides_path`
leaves the map, the catalogue, `uninstall` and `disk`.

### D7. `[memory] harness` becomes `[memory] native`

Its values stay `keep` (default) and `off`. "harness" read as a harness
name; "native" says it is the harness's own memory.

### D8. The name rule is a test

A test over `owned_paths()` fails when two `Document` or `Lock` entries in
different tiers share a file name, or when one directory holds a file and
a directory of one stem. Opaque subtrees and placeholders are skipped.

## Risks / Trade-offs

- [The installed `uze` (beta.22) keeps writing the old names after the dev
  build carried them] → two builds on one machine diverge for renamed
  records until `make install`; the carry runs once per old path, so a
  file the old build wrote again afterwards is reported by `doctor` as a
  leftover rather than carried a second time over newer state.
- [A carry interrupted midway] → each step writes the new path atomically
  and removes the old one only after; a rerun finds the new path and skips.
- [Journeys and conformance seeds spell old names] → each rename updates
  them in the same commit; `journey validate` and the Lab run before it
  lands.
