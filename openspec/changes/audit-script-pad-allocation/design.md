## Context

See proposal.md for the finding. `pad` is the shadow of Rhai's own, added to
refuse an empty padding. `replace` already counts its result before building it.

## Goals / Non-Goals

**Goals:** `pad` counts what it writes, as `replace` does.

**Non-Goals:** the other functions in the SCRIPT-2 table, whose verdicts stand.
Total captured bytes in `regex` were noted and not measured.

## Decisions

- **Count by walking the characters taken.** `len` is already at most the string
  bound here, so the walk is bounded, and it is exact for a padding of mixed
  widths, which a multiplication is not.

## Risks / Trade-offs

- [One pass over at most 1 MiB characters before building] → The same pass builds
  the result right after, so the cost at most doubles.
