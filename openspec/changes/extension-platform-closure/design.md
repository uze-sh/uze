## Context

The open tasks of `extension-platform` after its split changes, each
named by its umbrella number. The decisions they follow are the
umbrella's (D7 sanitizer, D15 catalog, D21 schema, D22–D24 binding and UI
state, D25–D26 authoring, D28 budget).

## Goals / Non-Goals

**Goals:** every open umbrella task delivered or dropped by a recorded
decision of this change.

**Non-Goals:** new gallery entries, new grants.

## Decisions

### C1. One change, five groups, each closing named umbrella tasks

Grouped by the code they touch so parallel work does not collide:
hygiene (contract and check), schema and reference (docs), input in views
(engine), inspector and budget (ui and application), journeys (last,
alone, since they run Docker).

## Risks / Trade-offs

- [Forms widen what a view may send] → a form's values reach only a
  declared action's input, cleaned, as an item does.
