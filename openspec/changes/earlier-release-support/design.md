## Context

A census of the workspace found about 91 places that exist only because an
earlier release left something on a machine, in five kinds. Half go through
a shared mechanism; half are inline:

| kind | sites | through a mechanism | inline |
|---|---|---|---|
| record shape | 8 | 6 (`Shaped` ladders) | 2 (`marketplace_catalogue.rs:161` default, `terminal/runtime/server.rs:1014` clamp) |
| path in the UZE home | ~22 | ~9 (`owned.rs` retired entries) | ~13 (`carry_into_sources` inside `MutationLock::acquire`, `carry_across_the_move` inside `record::ensure`, `setup.rs` `RETIRED` and `catalogue.rs` `Subject::Retired` repeating the map, `runtime/attachments` cleanup, `RETIRED_ENGINE_PATH`) |
| outside the home (harness files, project files, `.git`) | ~45 | ~20 (`retired_region`, receipt retirement in `lifecycle/attach.rs`) | ~25 (`uze-local` in five functions, `uze-official` in `setup.rs`, OpenCode bridges across `bridge.rs`/`opencode.rs`/`rules.rs`, four skill-wrapper cleanups, `StaleProjection` beside `RetiredRegionsPlan`, earlier checkouts across `pool.rs`/`reconcile.rs`/`task.rs`/`occupancy.rs`) |
| wire, endpoint, process | ~11 | 9 (`terminal/runtime/endpoint.rs`) | 2 (`--resumed`, `uze terminal` in `cli/grammar.rs`) |
| command spelling, config | 3 | 1 | 2 |

Code that copes with a *newer* build (the refusal of a newer record, keys
and theme forward tolerance) and with a *vendor's* older release
(`opencode2`, Antigravity's `serverUrl`) is not this change's subject and
stays where it is.

`tests/architecture/layering.rs` already has the shape this change needs
for enforcement: a `Rule` with a forbidden token, a scope, `sanctioned`
files with their reason, an exact `budget` for debt, and a `remedy`
printed on failure.

## Goals / Non-Goals

**Goals:**
- Every piece of support for an earlier release is findable by kind and
  bounded by a release.
- Retiring a release's support is: raise the floor, delete what the build
  names.
- An agent cannot add such support anywhere else without the build saying
  where it goes.

**Non-Goals:**
- Changing what any supported upgrade does: moving a site into its registry
  preserves its behaviour.
- Newer-build tolerance and vendor-version tolerance.
- A runtime "compatibility mode": the floor is a build constant.

## Decisions

### D1. The vocabulary lives in `uze-document`

`uze-document` is a leaf (serde, serde_json, thiserror, tracing) that
`uze-core`, `uze-workspace` and `uze-terminal` already depend on. Its scope
widens from "reading a record another build wrote" to "what an earlier
release left", and its crate doc and `document-ladder.md` say so. It gains:

```rust
pub enum Release { Alpha(u32), Beta(u32), Stable(u32, u32, u32) } // ordered
pub const FLOOR: Release = Release::Alpha(1);   // raised in task 7
pub enum Lasts { Until(Release), Unbounded }

pub struct Earlier {
    pub name: &'static str,         // stable id, printed on failure
    pub lasts: Lasts,               // the last release whose run could leave it
    pub spelled: &'static [&'static str], // literals only this entry may name
    pub site: &'static str,         // filled by `earlier!` from file!()
}
```

`Step` gains `lasts: Lasts`. `uze-integrations`, `uze-application` and the
binary reach the type through a re-export from `uze-application` (`src/`
must not name `uze_core::`, and a direct dependency there would be a second
door).

Alternative: a new `uze-earlier` crate — refused, a second leaf for the
same rule.

### D2. `lasts` is the last release whose run could leave it

Records are rewritten only by a caller holding their lock, so a shape-1
`marketplaces.json` from beta.5 can still sit on a beta.22 machine that
never ran `market add`. An entry is therefore `Until(r)` only when every run
since `r` carries out what it handles — a disposable retired path removed
on every bootstrap is; a ladder rung, a carry inside `MutationLock::acquire`,
a leftover removed on attach are not. Those are `Unbounded`, and the floor
never passes them. Bounding them needs a release that settles them on every
run (climb and rewrite every registered record, carry every retired path,
retire every leftover); that is a later change, and from its release the
entries become `Until(it)`.

`Until` is derived from history: `git tag --contains <commit that stopped
writing it> --sort=v:refname | head -1`, then its predecessor in version
order. An empty result (unreleased) is the newest tag. The suite checks
`FLOOR <= CARGO_PKG_VERSION` and that `lasts` never decreases up a ladder.

### D3. Declarations in one place per kind; runners stay where they run

| kind | declared in | runner (unchanged call site, now registry-driven) |
|---|---|---|
| record shape | the record's `Shaped::ladder()` | `uze_document::read` |
| home path | `owned.rs` `retired(..)` + `.carried_by(fn)` + `.lasts(..)` | the bootstrap sweep for disposable paths; `MutationLock::acquire` for the sources move; `record::ensure` for a project's records |
| harness files | `uze-integrations/src/<vendor>/earlier.rs`, `shared/earlier.rs` | today's attach, detach and inspect paths, receipt-checked as the `delivery-model` rule requires |
| project files | `uze-core/src/project/earlier.rs` (today's `retired_region`) | `uze-application`'s `retired_regions`; `StaleProjection` folds into its plan |
| earlier checkouts | `uze-workspace/src/checkout/earlier.rs` | `reconcile` |
| wire and endpoint | `uze-terminal/src/runtime/earlier.rs` | `endpoint::make_way` |
| command spelling | `src/cli/earlier.rs` | the grammar |

No runner moves into bootstrap: it takes no lock on a settled machine and
skips when the mutation lock is held, and `from_env`, tests and the harness
matrix never run it. No `IntegrationPort` method is added: per-package
leftovers need the package, and "accepted as matched" happens when a receipt
is inspected. `IntegrationRegistry::earlier()` gathers the integrations'
entries.

Each crate exposes one `pub fn earlier() -> Vec<Earlier>`, its ladders'
rungs included (record types are often private). Lists that repeat retired
names (`setup.rs` `RETIRED`, `catalogue.rs` `Subject::Retired`,
`uze-official`) are derived from the registry.

### D4. The suite (`tests/architecture/earlier.rs`)

- **Floor.** Every `Until(r)` with `r < FLOOR` fails, naming entry and site
  and saying "delete this entry".
- **Spelled once.** Each literal in an entry's `spelled` appearing in
  production source outside that entry's file fails, naming both places.
  This is what holds a retired path, file or marketplace name in one place.
- **Identifiers.** `legacy_`, `retired_`, `pre_`, `earlier_` in code (not
  comments) outside the homes fail; today's sites are exact budgets that
  reach zero. A word scan over comments was measured and refused: about 520
  lines in ~120 files, most of them design history.
- **Serde.** `serde(alias` fails anywhere. `serde(default` on a field of a
  `Shaped` record fails unless the doc line above it says `Additive`: an
  additive field is how a record stays readable by an older build sharing
  the machine, and banning it would force shape bumps that make that build
  refuse the record.
- **Coverage.** Every file with `impl … Shaped for` belongs to a crate whose
  `earlier()` includes it; every `Shaped` ladder is contiguous; every rung
  declares `lasts`.
- `Standing::Retired` only in `owned.rs` and the registry readers.

Failure messages name `.agents/rules/earlier-releases.md` and the remedy.

### D5. The agent's guide

`.agents/rules/earlier-releases.md`, `model_decision` ("Read before writing
code that copes with anything an earlier UZE release left") plus a glob on
`**/earlier.rs`, `**/machine/home/owned.rs` and `crates/uze-document/**`:
the kinds and their homes, `Until` versus `Unbounded`, how to derive the
release, and that a failure's remedy is the instruction to follow.
`document-ladder.md` and `persistence-tiers.md` point at it; the latter's
"a path is retired only when no supported build reads it" becomes the
floor.

### D6. Order

1. Vocabulary, the suite and the rule, with `FLOOR = Alpha(1)` (every entry
   passes) and the identifier budgets frozen: no new inline support lands.
2. One commit per kind moving its declarations home, lowering budgets.
3. Derive every `lasts`; raise the floor to `Beta(22)` together with
   deleting the `Until` entries below it, after `07-upgrade` runs beta.22.
   `Unbounded` entries stay until a settling release exists.

## Risks / Trade-offs

- [`lasts` set wrong keeps dead code or deletes live support] → derived
  from `git tag --contains`, `Unbounded` by default, and `07-upgrade` runs
  the floor release against this build before the floor is raised.
- [Most entries start `Unbounded`, so raising the floor deletes little at
  first] → accepted: deleting a lazily carried rung is data loss; the
  settling release is what makes them deletable.
- [Moved tests behind `docs/architecture/invariants.md` citations] →
  updated in the same commit, `invariants_citations.rs` checks them.
- [Two open changes modify the same `persisted-state` requirement] →
  `refactor-home-file-shape` drops its beta wording and carries its renames
  as retired entries instead; this change lands first.

## Candidate ADRs

- A release floor bounds every piece of support for an earlier release:
  every later shape change and every retirement depends on it.
