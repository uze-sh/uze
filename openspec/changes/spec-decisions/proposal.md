## Why

A project's decisions are the part of its intent that outlives every
change, and the spec surface, which exists to show intent beside the code,
cannot show them. No spec-driven tool fixes where they live: OpenSpec has
ADRs only through a community schema (`<repo>/adr/`), Spec Kit only
through the adrkit extension (`docs/adr/`), Superpowers and GSD have none.
The classic conventions (adr-tools, MADR) agree on the *format* — one
decision per file, a title, a status, context, decision and consequences —
and disagree on the place. This repository writes them both ways: in
flight inside an OpenSpec change's `adr/`, and kept in `docs/adr/`, where
the surface sees neither as a decision (the first is listed as "other").

## What Changes

- The spec surface offers a fourth subject, **decisions**: every ADR the
  project keeps, with its status and what it supersedes or is superseded
  by, rendered as Markdown and opened in the code surface on activation.
- Decisions are found by signals that hold in **any language**, inside the
  places the project declares in `workspace.artifacts` (see the
  `workspace-manifest-section` change): a record's numbered file name
  (`NNNN-slug.md`), its title and status from front matter or structure,
  and supersession only from front matter keys. The place is the project's
  decision because no tool makes it, unlike a tool's units, which the
  dialect still finds by the tool's own marker.
- A decision found needs no spec-driven tool at all: a project with ADRs
  and no marker gets the decisions subject and nothing else.
- A numbered record inside a change is given a
  new role, **decision**, listed after the design, instead of "other".
- Read-only, like everything the surface shows.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `spec-surface`: a fourth subject (decisions), a fifth role (decision),
  decisions found by format in the declared places, and a checkout with
  decisions and no tool marker no longer reads as empty.

## Impact

- `crates/uze-extensions/src/spec/`: a decision reader (numbered name,
  status, supersession), the `Decisions` subject, the `Decision` role.
- The artifact places move from the architect's `ArtifactSource` to
  `uze-extensions/src/shared/` now that a second surface reads them, and
  the host hands them to both (`src/ui/extension_host.rs`).
- `src/ui/` subject selector gains one entry; no new chrome.
- Depends on `workspace-manifest-section` for `workspace.artifacts` as a
  list; it can land after it or in the same release.
