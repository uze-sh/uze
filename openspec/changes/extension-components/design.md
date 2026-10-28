## Context

See proposal.md. A view is compiled by `view::compile` from a whole KDL
document; a script by `Script::compile` from a whole Rhai text, whose
`answer` is its one entry. Each reports offsets against the file it read.
The manifest checks each file's suffix (`relative_file(…, "kdl"/"rhai")`).
Consent digests the extension's bytes, whatever their names.

## Goals / Non-Goals

**Goals:** a view and its logic in one file; a button's logic a function
beside it, not a file; one way to write an extension; errors that point at
the line in the `.uze` file.

**Non-Goals:** the manifest or tests in `.uze`; a `<style>` block; editor
grammars beyond UZE's own highlighter; running a script at check time.

## Decisions

- **Blocks are tags alone on their lines** (D1).
- **Each block is read in place**, offsets kept, so a diagnostic names the
  `.uze` line (D2).
- **One way to write it: `.kdl` views and `.rhai` scripts are refused**
  (D3).
- **`call` runs a function of the surface's own script** (D4); what it may
  do is still the manifest's.
- **A script-only `.uze` is a file with only `<script>`** (D5).
- **The manifest and the tests stay KDL** (D6).
- **Highlighting is UZE's own** (D7).
