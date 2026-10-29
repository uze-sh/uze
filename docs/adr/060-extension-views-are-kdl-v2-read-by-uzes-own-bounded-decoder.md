# Extension views are KDL v2, read by UZE's own bounded decoder

Status: Accepted

## Context

A plugin's extension declares its surfaces and the views they draw, and
those files are third-party input that the workspace turns into cells. The
format is written by hand by authors, read by UZE on every reload, and
later by tests and an LSP. It needed a tree that reads as a UI, positions
for every error, and a parser that untrusted input cannot stall or exhaust.

## Decision

**Manifests, views and tests are KDL v2, decoded by a hand-written decoder
in `uze-core` over the `kdl` crate, and the client only ever draws
compiled views.**

- The parse is bounded before it runs: a 64 KiB cap, a depth pre-scan
  refusing above 32, and a worker thread with a 2-second limit.
- The decoder walks `KdlDocument` with spans rather than the crate's serde
  bridge, and refuses unknown nodes, properties and arguments, repeated
  properties, type annotations, the wrong case and v1 spellings with file,
  line, column and a suggestion. Catalog nodes are PascalCase.
- What it produces is `uze-contract` data, cached in the generated tier.
  `kdl` is a dependency of `uze-core` only; `uze-contract`,
  `uze-extensions` and `src/ui` never parse KDL.

The rejected alternatives were YAML with a JSON Schema (completion on day
one, but a verbose tree, fragile indentation and `{field}` colliding with
flow maps) and a TOML manifest beside KDL views (two formats for one
author).

## Consequences

An author gets precise errors and a view that reads as its layout, and the
drawing thread never meets a parser. UZE owns the decoder, the grammar for
its editor and, with the next change, an LSP: editors give no completion
for these files until then. The `kdl` crate has one maintainer, so its
updates go through the dependency rule like any other, and the guards are
tested against a 400-deep nest.

Source change: openspec/changes/archive/2026-10-24-extension-view-engine/
