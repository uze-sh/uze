## D1 — Prove store ownership in the acceptance suite, ignored until delivery moves

- status:   pending
- task:     1.3
- commit:
- trigger:  the task names `tests/integrations/vendor_neutral.rs`, whose tests use in-process fake integrations; only the acceptance suite drives the real binary against the fake vendor CLIs, which is what writes real harness files. A test that "must be red on the current code" also cannot land enabled without breaking every commit's gate.
- decided:  `tests/acceptance/store_ownership.rs::no_harness_artifact_names_outside_its_store`, on a new canonical fixture `store-tie` (a skill, an MCP server and a hook, each naming `${PLUGIN_ROOT}`), marked `#[ignore]` with the task that removes the mark (4.9). Run with `--ignored` it is red today and lists every tie.
- rejected: an in-process test in `vendor_neutral.rs` (it would prove the fakes, not the delivery); landing it red (breaks the gate for the whole run).
- blast:    moving one file between suites; removing the `#[ignore]` at 4.9.
