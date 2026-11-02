# Decisions

## D-1. Ladder checks live in `uze_document::earlier::rungs`

- **Chose:** `rungs::<T>()` asserts the ladder is contiguous, ends at `T::SHAPE` and that `lasts` never grows going down; the architecture suite calls every crate's `earlier()`, so every ladder is checked there.
- **Over:** a separate suite walking ladders, which needs every record type to be visible to the root test crate (most are private to their module).
- **Why:** the check sits where the ladder is turned into entries, and a module that forgets `rungs()` gets a dead-code warning (`-D warnings`) or the suite's coverage scan.
- status:

## D-2. A crate-level `earlier` module in `uze-core`, outside the five concerns

- **Chose:** `uze_core::earlier`, gathering the concerns' entries, beside `capability/`, `delivery/` and the rest.
- **Over:** one aggregator per concern, re-exported at the root.
- **Why:** it is a registry the suite reads, not a concern of its own; one door per crate is what the suite needs.
- status:

## D-3. Identifier marks are `legacy_`, `retired_` and `earlier_`; `pre_` is dropped

- **Chose:** three prefixes, counted at a word start on code lines, with `src/ui/orchestrator/input.rs` sanctioned (the X10 mouse encoding).
- **Over:** the design's four, including `pre_`.
- **Why:** measured, `pre_` matched `pre_tool_use` (17, a hook event) and `pre_exec` (6, `CommandExt`) and only three real sites (`pre_sources_layout`), which `spelled` covers once that entry is declared.
- status:

## D-4. The unmarked-default rule freezes today's 43 defaults as exact budgets

- **Chose:** a per-file exact budget of `#[serde(default)]` attributes without `Additive` in the doc above, in files that implement `Shaped`.
- **Over:** annotating all 43 now.
- **Why:** each one needs a judgement (additive versus an earlier shape's absence, which belongs in a rung); freezing them stops new ones today and leaves the judgement to the commit that touches each record.
- status:
