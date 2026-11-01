## D1 — Prove store ownership in the acceptance suite, ignored until delivery moves

- status:   pending
- task:     1.3
- commit:
- trigger:  the task names `tests/integrations/vendor_neutral.rs`, whose tests use in-process fake integrations; only the acceptance suite drives the real binary against the fake vendor CLIs, which is what writes real harness files. A test that "must be red on the current code" also cannot land enabled without breaking every commit's gate.
- decided:  `tests/acceptance/store_ownership.rs::no_harness_artifact_names_outside_its_store`, on a new canonical fixture `store-tie` (a skill, an MCP server and a hook, each naming `${PLUGIN_ROOT}`), marked `#[ignore]` with the task that removes the mark (4.9). Run with `--ignored` it is red today and lists every tie.
- rejected: an in-process test in `vendor_neutral.rs` (it would prove the fakes, not the delivery); landing it red (breaks the gate for the whole run).
- blast:    moving one file between suites; removing the `#[ignore]` at 4.9.

## D2 — Move each marketplace directory whole, not each package

- status:   pending
- task:     2.4
- commit:
- trigger:  the task asks for a per-package rename; the layout being moved is `store/plugins/<marketplace>/<plugin>` beside `store/superseded/<id>`, and `sources/` keeps exactly that layout.
- decided:  `package::sources::carry_into_sources` renames each child of `store/plugins` and `store/superseded` into `sources/` (one rename per marketplace and per superseded revision), never overwriting what already sits on the new side, then removes the emptied old directories. Same guarantees — resumable, nothing copied, nothing overwritten — in fewer renames. It runs from `MutationLock::acquire`, with the lock held, after the fence and before the digest backfill.
- rejected: a rename per package (more renames for the same guarantee); one rename of `store/` to `sources/` (would also carry anything else under `store/`, and `store/` is about to hold the vendor stores).
- blast:    the loop in `carry_entries`.
