Lab: a probe binary calling `uze-platform`'s API as one uid, debug build on Linux
(WSL2), against the audited sources and this branch. Tracked as GHSA-fxhj-rcfq-8qh6.

## PLAT-1

- **Fix**: `usable` asks about the entry first, makes a missing one with `mkdir`, which
  does not follow a link, and leaves narrowing to `private_directory`, which refuses a
  link first and narrows through a descriptor
  (`crates/uze-platform/src/endpoint/unix.rs`).
- **Regression test**: `crates/uze-platform/src/endpoint.rs::tests::a_link_at_a_socket_directory_name_is_refused_and_what_it_reaches_keeps_its_mode`
  and its three neighbours (`cargo test --locked -p uze-platform --lib endpoint::`).
  Against the audited sources the four fail:
  ```
  test endpoint::tests::restoring_over_a_link_is_refused_and_what_it_reaches_keeps_its_mode ... FAILED
  test endpoint::tests::a_link_at_a_socket_directory_name_is_refused_and_what_it_reaches_keeps_its_mode ... FAILED
  test endpoint::tests::an_address_that_can_only_be_remembered_through_a_link_is_refused ... FAILED
  test endpoint::tests::an_endpoint_directory_that_is_a_link_is_refused_and_what_it_reaches_keeps_its_mode ... FAILED
  test result: FAILED. 5 passed; 4 failed; 1 ignored; 0 measured; 108 filtered out; finished in 0.02s
  ```
  On this branch:
  ```
  test endpoint::tests::a_link_at_a_socket_directory_name_is_refused_and_what_it_reaches_keeps_its_mode ... ok
  test endpoint::tests::an_address_that_can_only_be_remembered_through_a_link_is_refused ... ok
  test endpoint::tests::restoring_over_a_link_is_refused_and_what_it_reaches_keeps_its_mode ... ok
  test endpoint::tests::an_endpoint_directory_that_is_a_link_is_refused_and_what_it_reaches_keeps_its_mode ... ok
  test result: ok. 9 passed; 0 failed; 1 ignored; 0 measured; 108 filtered out; finished in 0.02s
  ```
- **Exploit re-run**: against the audited sources:
  ```
  user_socket_directory -> None; target 700
  endpoint(link) -> placed elsewhere=true; target 700
  endpoint(remembered through link) -> ok=true; target 700; entries in target 1
  restore(link) -> ok=false; target 700
  ```
  On this branch:
  ```
  user_socket_directory -> None; target 755
  endpoint(link) -> placed elsewhere=true; target 755
  endpoint(remembered through link) -> ok=false; target 755; entries in target 0
  restore(link) -> ok=false; target 755
  ```
- **Residual**: a link at a parent of the directory is followed by `mkdir`; those are
  the temp directories themselves or the path to the operator's own home, and nothing
  is narrowed through one. `fs::create_private_dir_all` still follows a link for its
  other callers, by design: each names a path under `UZE_HOME`, a location the operator
  chose (the threat model's same-uid process is out of scope). Not run with a real
  second account, on macOS, or with the swap between `lstat` and `open` forced, which
  no test can time.

## Gate

- `cargo fmt --check`: clean.
- `cargo clippy --locked --all-targets -p uze-platform -- -D warnings`: clean.
- `cargo test --locked -p uze-platform`: 114 passed, 4 ignored.
- `cargo test --locked --test architecture`: 51 passed.
- `openspec validate --all --strict`: 100 passed.
- Not run: `cargo test --workspace`, left to CI because the machine was in use.
