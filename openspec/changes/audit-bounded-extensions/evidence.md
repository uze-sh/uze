Lab: `$AUDIT/fix-bounded` (fixed debug binary from this branch) and
`$AUDIT/fix-bounded-old` (the audit's own `$AUDIT/bin/uze`, the audited
build), each PoC run through `.agents/skills/security-audit/scripts/sandbox`.
Every regression test was run against the audited sources (the fix's files
checked out from the audited commit `98d1210f`, the new tests kept) and failed as noted.

## EXT-1

- **Fix**: The binder spends at most `EXPANSION_MAX` (20 000)
  entries per build, drawn or not, then stops walking and folds with
  `Folded: this view expands to more than 20000 nodes`; the check refuses a
  view whose templates alone expand past it.
- **Regression test**:
  `crates/uze-extensions/src/declared/tests.rs::a_template_fan_out_is_folded_once_the_build_spent_its_budget`
  and `::an_each_over_its_own_parent_list_is_bounded_by_the_budget`
  (`cargo test -p uze-extensions --lib budget`): on the audited binder the
  run was still walking after 3 minutes and was stopped; now 0.04 s for the
  module. `crates/uze-core/src/capability/extension/tests.rs::a_template_fan_out_past_the_bind_bound_is_refused_by_the_check`
  (`cargo test -p uze-core --lib fan_out`): audited compiler panics
  "accepted"; passes now.
- **Exploit re-run**: `poc/mkmarket.sh 12 10` (1 869 bytes), fixed binary:
  ```
  × bar.uze: templates expand this view to more than 20000 nodes, before any `each` repeats them (use templates fewer times inside one another)
  error: check failed (1 problem)
  rc=1 wall=.116
  uze install tick@evil → 1 extension added [126ms]; uze status → tick · tick@evil  × does not compile  bar.uze: templates expand …
  ```
  Runtime variant (3 levels, which pass the check, inside `each` over 1 000
  items: ~2.1 M entries at bind), `uze agent plugin check`: fixed `wall=.110`,
  audited binary `wall=1.41`.
- **Residual**: none for the hang. The bomb still installs and is reported
  as not compiling, as every non-compiling view is (decisions D2). A bind
  now costs at most 20 000 entries on the drawing thread (~1 ms).

## SCRIPT-2

- **Fix**: `replace` (string or character find, string
  substitute) counts its result before building it; `map` and `zip` count
  each result as it is collected; all three refuse with Rhai's own size
  error. Every package function was checked (design.md, SCRIPT-2 table).
- **Regression test**: `crates/uze-script/tests/sizes.rs` — one test per
  shadow (`replace_refuses_…`, `replace_of_a_character_…`, `map_refuses_…`,
  `zip_refuses_…`), `cargo test -p uze-script --test sizes`. On the audited
  engine each fails its "refused before allocating" assertion (2.8 s, 2.6 s,
  11.9 s, 4.3 s, with the allocations made); `map_counts_…` and
  `the_shadows_answer_as_rhai_does_…` are semantics tests that pass on both.
- **Exploit re-run**: `run.sh t_replace.rhai` and a `map` variant, fixed binary:
  ```
  × v.uze: source `src` did not answer: v.uze: the script crossed its string size bound
  rc=1 maxrss=47796KB wall=0.06          (replace)
  rc=1 maxrss=46832KB wall=0.02          (map)
  audited binary, map variant: rc=1 maxrss=5206716KB wall=7.68
  ```
- **Residual**: functions whose result is linear in their input (`split`,
  `to_chars`, `to_upper`) may hold up to ~16 × the 1 MiB string bound
  before the check; accepted and tabled. A Rhai upgrade must re-check the
  table.

## SCRIPT-1

- **Fix**: Every answer runs on a scoped thread with
  `engine::STACK` (64 MiB) and the resident thread uses it too; after each
  resident call, `this` nesting deeper than a value may cross stops the
  session with `Limit::Nesting` ("the script crossed its nesting depth
  bound").
- **Regression test**: `crates/uze-script/tests/depth.rs`
  (`cargo test -p uze-script --test depth`):
  `a_resident_nesting_this_across_calls_is_stopped_at_the_crossing_depth`,
  `a_deep_value_built_in_one_answer_is_compared_and_dropped_on_the_script_stack`,
  `a_deep_value_built_in_one_resident_call_is_compared_on_the_session_stack`.
  On the audited sources all three abort: `thread 'uze-resident' has
  overflowed its stack` / `fatal runtime error: stack overflow, aborting`.
  `a_resident_nesting_closures_in_this_is_stopped_at_the_crossing_depth`
  pins the same bound for nesting through closures, whose captures the
  first version of the bound did not walk (found by the branch's review;
  red before the depth walk entered a closure's captures).
- **Exploit re-run**: the audit's `deep_nesting_across_resident_calls`
  (`ws/crates/uze-script/tests/audit.rs`), built from this branch inside the
  sandbox:
  ```
  stopped at call 15: the script crossed its nesting depth bound
  survived: false
  test result: ok. 1 passed
  ```
  (audited: depth 6 408 at call 800, then SIGABRT in release; ~1 150 in debug).
- **Residual**: within a single call a debug build covers ~5 000 levels of
  `==`, several times the ~1 000 one answer nests before its deadline; a
  release build covers Rhai's structural ceiling (~20 000). Accepted.

## EXT-2

- **Fix**: The pre-scan treats `\` outside strings and comments
  as continuing the node over the next line break, so the existing refusal
  of a slashdashed block inside another applies.
- **Regression test**:
  `crates/uze-core/src/capability/extension/bounded.rs::tests::an_escline_does_not_hide_a_slashdashed_block_inside_another`
  (`cargo test -p uze-core --lib escline`), covering `\n`, `\r\n`,
  trailing spaces, a `//` and a `/* */` comment and U+00A0 after the `\`.
  The audited scanner accepts the PoC (`expect_err` panics).
- **Exploit re-run**: `gen/mk2.py 14 esc ''` and 22 levels, and the audit's
  `q` variant (`/-n /- \`), fixed binary:
  ```
  × timer.uze:5:1: a block commented out with `/-` sits inside another (remove the inner `/-`, or comment the outer block with `/* */`)
  error: check failed (1 problem)
  wall=.110
  ```
  The audited binary lets the view through the scan (only the plugin's own
  test failures are reported). In this lab its parse of the shape stayed at
  0.21 s even at 22 levels, so the slowdown the audit measured was not
  reproduced through the CLI; the bypass of the guard was.
- **Residual**: the 2 s parse timeout still leaves its thread running on any
  input the scan lets by (unchanged, out of scope).

## Gate

- `cargo build --locked --bin uze`: ok.
- `cargo test -p uze-contract -p uze-script -p uze-extensions -p uze-core`:
  all suites ok (744 uze-core lib, 539 uze-extensions lib, every uze-script
  suite); `cargo test -p uze-application extensions`: 86 passed;
  `cargo test --test architecture`: 38 passed.
- `cargo clippy --all-targets -- -D warnings` (root, and the four touched
  crates): ok. `cargo fmt --check`: ok.
- `openspec validate audit-bounded-extensions --strict`: valid.
- `cargo test --workspace` is left to the parent after the join.
