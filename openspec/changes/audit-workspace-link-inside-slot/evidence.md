Lab: the regression tests below, in scratch repositories made through `uze-testkit`
with every link inside the test's own scratch directory, debug build on Linux
(WSL2), against the audited sources and this branch. Tracked as GHSA-2m3v-7xj9-hx9g.

## WKS-1

- **Fix**: a declared link, and an untracked change carried into a checkout, has
  its directories asked about one at a time without following a link, missing
  ones made with a single `mkdir`, and is refused with a warning at a link or a
  non-directory; the destination is asked with `symlink_metadata`
  (`crates/uze-workspace/src/checkout/pool.rs`, `directories_within`, used by
  `materialize` and by `carry_untracked` in `checkout/lifecycle.rs`).
- **Regression test**: `crates/uze-workspace/src/checkout/tests.rs::a_declared_link_is_made_only_inside_its_checkout`
  and `::an_untracked_file_is_carried_only_inside_the_checkout`
  (`cargo test --locked -p uze-workspace --lib -- checkout::tests::a_declared_link checkout::tests::an_untracked_file_is_carried_only_inside_the_checkout`).
  Against the audited sources the two fail:
  ```
  test checkout::tests::a_declared_link_is_made_only_inside_its_checkout ... FAILED
  test checkout::tests::a_declared_link_makes_its_missing_directories_inside_the_checkout ... ok
  test checkout::tests::an_untracked_file_is_carried_only_inside_the_checkout ... FAILED
  test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 462 filtered out; finished in 0.72s
  ```
  On this branch:
  ```
  test checkout::tests::a_declared_link_is_made_only_inside_its_checkout ... ok
  test checkout::tests::an_untracked_file_is_carried_only_inside_the_checkout ... ok
  test checkout::tests::a_declared_link_makes_its_missing_directories_inside_the_checkout ... ok
  test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 462 filtered out; finished in 0.99s
  ```
- **Exploit re-run**: the first test is the original proof: a target commit turns
  the declared link's directory into a link, and a slot is placed from that tip.
  Against the audited sources:
  ```
  made outside the checkout: [".../slots-link-parent-elsewhere-.../sub", ".../slots-link-parent-elsewhere-.../.env"]
  written outside the checkout: [".../carry-inside-elsewhere-.../notes.txt", ".../carry-inside-elsewhere-.../new.txt"]
  ```
  On this branch the directory the link reaches stays empty, and each entry is
  reported, as in
  ```
  could not prepare `config/.env`: `config` is a link in the checkout, and nothing is made through it
  ```
- **Residual**: none known. A component swapped for a link between being asked
  about and the next one being made is not forced by any test; the final
  `resolves_within` narrows it, and the checkout's tree is Git's at placement.
  The copy's only caller cuts the slot from the primary's own `HEAD`; the
  copy's test calls it on a slot cut elsewhere. Not run on macOS or Windows,
  where a committed link checks out as a file by default.

## Gate

- `cargo fmt --check`: clean.
- `cargo clippy --locked --all-targets -p uze-workspace -- -D warnings`: clean.
- `cargo test --locked -p uze-workspace --lib`: 464 passed, 1 ignored (before
  the platform-gate comments; the change after it is comments only).
- `cargo test --locked -p uze-application --lib -- link carry`: 15 passed.
- `cargo test --locked --test architecture`: 51 passed.
- `openspec validate --all --strict`: 101 passed.
- Not run: `cargo test --workspace`, left to CI because the machine was in use.
