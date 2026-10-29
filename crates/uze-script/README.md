# uze-script

The engine a declared extension's script runs in. An extension's logic is a
Rhai script whose `fn answer(input, context)` returns an `Envelope`; this
crate is the only one that names `rhai`.

- `Script`, `ScriptCache` — compile once by digest, answer many times
- `Reach`, `Grant` — the host's narrow interface (`read_checkout`, `list_checkout`, `clock`, `run_program`, `fetch`) and the grant each function needs
- `FetchRequest` — what `fetch` asked, checked against the `net:` grants before the host sees it
- `host_functions_of`, `ungranted_calls` — what the check reads from a script's text before anything runs
- `Resident` — a script kept running on a thread of its own while its surface is open, with `timer(ms)`
- `ScriptError`, `Trace` — the bound a script crossed or where it failed, and what it printed

The engine is built raw and reaches nothing of the machine: no modules, no
`eval`, strict variables, and bounds on operations, depth, sizes and wall
time. It holds only the host functions the extension's grants register,
each answered by the host through `Reach`. Beside them, every script has
pure primitives run in Rust: `lines`, `glob`, `regex`, JSON and front matter.

It depends only on `uze-contract` and `uze-platform`, and is used by
`uze-application` alone; the architecture suite fails on any other `rhai::`.

```bash
cargo test -p uze-script
```

See `openspec/changes/extension-platform/` (D19, D27) and
`extension-manifest-and-scripts/` (D12–D13).
