Lab: a probe binary calling `uze-script`'s API, debug build on Linux with
`overcommit_memory=0`, against the audited sources and this branch.

## SCRIPT-3

- **Fix**: `pad` counts the bytes of the characters it will write, refuses past
  the string bound with `Limit::StringSize`, and reserves exactly that
  (`crates/uze-script/src/primitives.rs`).
- **Regression test**: `crates/uze-script/tests/sizes.rs::a_padding_is_counted_by_what_it_writes_not_by_how_long_it_is`
  and its two neighbours (`cargo test -p uze-script --test sizes padding`). Against
  the audited sources the test binary aborts: `memory allocation of 549754765314
  bytes failed`. All of `cargo test -p uze-script` passes now.
- **Exploit re-run**: the proof in findings.md, on this branch:
  ```
  OK 1048576 trace=(0, 0, None)
  rc=0
  ```
- **Bypass attempts**: a two-byte padding to the bound in characters is refused at
  the string bound; a length past the bound is refused; a length that is not a
  valid `usize` leaves the text alone, as before.

## Residual

- Not re-run through the real runner, on Windows or on macOS.
- `regex` caps its match count but not the total bytes captured. This was noted
  and not measured; it is not claimed as a finding.
