---
trigger: glob
globs: "**/crates/uze-theme/**, **/crates/uze-keys/**, **/src/progress.rs, **/src/ui/theme.rs, **/src/ui/keys.rs"
---

`uze-theme` is the design vocabulary: colour `Token`s, named `Symbol`s, the
theme file schema, and the resolver that completes a partial theme from the
built-in default. `uze-keys` is the same for input: the `Action` a gesture
means, the `Scope` it is live in, the `Chord` that reaches it, with
`Keymap` the only thing that answers either direction.

Both are leaves naming no rendering or terminal library, resolving no path
and reading no environment. Each consumer adapts them: `src/ui/theme.rs`
(ratatui) and `src/progress.rs` (the CLI) for colour, `src/ui/keys.rs` for
keys, and `src/ui/keys.rs` is the only file in `src/` that names a
`KeyCode` (`one module names a physical key`). **Nothing outside those two colour adapters names a colour value**,
and nothing outside `src/ui/theme.rs` writes a chrome glyph inline; all three
fail the build in `tests/architecture/layering.rs`. Add a token or a symbol
when none of the existing ones says what yours means.
