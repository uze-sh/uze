## D1 — Find the cursor's place with a lexer, not the last good parse

- status:   pending
- task:     1.4
- commit:
- trigger:  The design (T6) said `locate` falls back to the file's last good parse when the document does not parse. While the author types, the document almost never parses, and the last good parse is out of date with the very line being typed.
- decided:  `locate` reads the text up to the cursor the way KDL's lexer does (strings, raw strings, comments, braces, statement ends) and answers from that, broken document or not. It never parses.
- rejected: Keeping a last good parse per open file: it needs state in the server, answers about the document as it was, and still has to guess on the line being typed.
- blast:    One module (`capability/extension/locate.rs`) and its tests; the LSP's completion would call a parse-based fallback instead.

## D2 — Make strsim a direct dependency of uze-core

- status:   pending
- task:     1.3
- commit:   da61a48c
- trigger:  The nearest-name suggestion needs an edit distance.
- decided:  `strsim` 0.11 as a direct dependency of `uze-core`. It is already in the lock through clap (which uses it for the same "did you mean"), and it has no dependencies.
- rejected: A hand-written Damerau–Levenshtein: about forty lines that the lock already carries a tested copy of.
- blast:    One line in `crates/uze-core/Cargo.toml`; `document::nearest` would need its own distance.
