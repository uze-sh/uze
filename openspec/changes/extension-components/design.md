## Context

See proposal.md. A view is compiled by `view::compile` from a whole KDL
document, its nodes keeping spans that later code places against the
file's whole text (`checked.rs::place`, the inspector's trace). A script
is compiled by `Script::compile`, which requires `answer`, from a whole
Rhai text. The manifest checks each file's suffix. Consent digests the
extension's bytes, whatever their names, and `needs_consent` decides
which manifests run code. `uze-extensions` and `uze-script` may reach no
UZE crate but `uze-contract`.

Two independent reviews of the first draft (one against the code, one
from an author's seat and the migration's) are folded into the decisions
below: the reader's crate, blanking instead of shifting, `call`'s consent
and its resident case, and the migration's missing pieces.

## Goals / Non-Goals

**Goals:** a view and its logic in one file; a button's logic a function
beside it, not a file; one way to write an extension; every position a
`.uze` file's own.

**Non-Goals:** the manifest or tests in `.uze`; a `<style>` block; editor
grammars beyond UZE's own highlighter; running a script at check time.

## Decisions

- **Blocks are tags alone on their lines**, ruled in `uze-contract` (D1).
- **Each block is read from the file with everything else blanked**, so
  every offset, span and Rhai position is the file's (D2).
- **One way to write it** (D3).
- **`call` runs a named function**, needs consent like `script`, and is
  refused on a resident's script (D4).
- **Script-only components**, and a script's role decides what it must
  define (D5).
- **The manifest and the tests stay KDL** (D6).
- **Highlighting treats tag lines as fences** (D7).
- **The check reaches every script and scans only its block** (D8).
- **The language server knows `.uze`** (D9).
- **`answer` stays a source's entry; an action only `call`s** (D10).
