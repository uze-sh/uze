---
trigger: glob
globs: "**/tests/**, **/crates/uze-testkit/**"
---

The root `tests/` holds domain-organized integration suites, one `main.rs` per
domain (`cli/`, `memory/`, `packages/`, `project/`, `lifecycle/`,
`projection/`, `integrations/`, `architecture/`, `acceptance/`), with
installer tests in `tests/scripts/`. Shared infrastructure is
`crates/uze-testkit` (isolated `TestEnvironment`, `FakeHarness`,
canonical/scenario fixtures, the loopback forge). The L0-L4 taxonomy is in
`tests/README.md`.

`tests/architecture/layering.rs` enforces the layering and ownership rules
AGENTS.md lists; a failure there is the design talking, and the fix is in
the code.

A flaky test is fixed by removing the race, never by a retry or sleep
loop.
