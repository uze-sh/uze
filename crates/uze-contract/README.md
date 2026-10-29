# uze-contract

The contract between a declared extension and everything that handles it.
A plugin declares an extension in KDL: `uze-core` reads and checks it,
`uze-extensions` binds it to data and describes what to draw, and the
workspace client draws that. This crate is the one definition all three
share, so none of them has to reach another.

- `manifest` — what an `extension.kdl` declares: surfaces, sources, actions, grants, preferences
- `compiled`, `binding` — a view after it was checked, and how it refers to data and UI state
- `catalog`, `icons` — the nodes a view may name, what each accepts, and the icons by idea
- `component` — the one reader of a `.uze` file's `<view>` and `<script>` blocks
- `clean` — the one way a string an extension supplied becomes drawable
- `question`, `testing` — what an extension asks the host, and what its test file declares
- `docs` (feature `schema`) — the schema and reference written to `docs/extensions/`

Plain data and pure functions: no I/O, no UZE crate, no rendering library
and no KDL parser, so the client that links it can never parse a document a
third party wrote. It is the one UZE crate `uze-extensions` may depend on.

```bash
cargo test -p uze-contract
cargo run -p uze-contract --features schema --example extension-docs   # rewrite docs/extensions/
```

See `openspec/changes/extension-platform/` (D22) and ADR-041.
