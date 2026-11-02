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
