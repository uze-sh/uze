## D1 — Find the cursor's place with a lexer, not the last good parse

- status:   pending
- task:     1.4
- commit:   a0f51e3b
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

## D3 — Compose the one analysis in the root crate, not in uze-application

- status:   pending
- task:     2.2
- commit:
- trigger:  T4 put `analyse(dir, overlay)` in `uze-application`, calling the engine's fixture binding. `uze-application` does not depend on `uze-extensions` (the engine sits under presentation, ADR-041), so the service cannot bind a view.
- decided:  `uze-application` answers `check_extension(dir, overlay)`: compile diagnostics, fixtures, decoded tests and file texts, all structured (`ExtensionDiagnostic`). The root crate adds the engine's findings (`extension_check::bind_fixtures`, then the test runner) to that one value, and both `uze agent plugin check` and the language server call that same path.
- rejected: Adding a `uze-extensions` dependency to `uze-application`, which would invert the layering the architecture tests hold.
- blast:    `CheckedExtension`'s diagnostics went from strings to `ExtensionDiagnostic`; JSON output still prints them as the same lines.

## D4 — Run tests from the library's ui module, not src/cli

- status:   pending
- task:     3.1
- commit:
- trigger:  T2 placed the runner in `src/cli/extension_test/`. The CLI is the binary crate and the renderers are private modules of the library's `ui`, which the binary cannot reach.
- decided:  The runner is `uze::ui::extension_test`, a public module of `ui` whose children may reach `extension_view`, the widgets and the theme; `uze agent plugin check` calls `run_tests` from the binary.
- rejected: Making `extension_view` and the widgets public to the binary: the TUI's internals would become the library's API.
- blast:    The module moves; nothing else names it.

## D5 — Default sizes per slot, and a text match that reads across wrapped rows

- status:   pending
- task:     3.3
- commit:
- trigger:  The spec says what a test leaves unsaid takes "the slot's default size" without naming one, and a wrapped paragraph is never on one row.
- decided:  A `view` is drawn at 100×30, a `section` at 30×16 (the sidebar's width), a `drawer` at 46×30. `expect-text` passes when the text is on one row, or when its words follow each other across rows once the drawer's frame is set aside.
- rejected: Requiring `size` in every test (noisier tests for the common case); matching only within a row (every long paragraph would need a substring chosen to fit a row).
- blast:    Snapshot file names carry the size, so a changed default renames the snapshots of tests that omit `size`.
