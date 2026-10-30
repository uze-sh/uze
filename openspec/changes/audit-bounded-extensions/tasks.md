## 1. EXT-1 - template fan-out on the drawing thread

- [x] 1.1 Regression tests: `crates/uze-extensions/src/declared/tests.rs::a_template_fan_out_is_folded_once_the_build_spent_its_budget`, `an_each_over_its_own_parent_list_is_bounded_by_the_budget`, and `crates/uze-core/src/capability/extension/tests.rs::a_template_fan_out_past_the_bind_bound_is_refused_by_the_check`, failing on the audited commit
- [x] 1.2 Fix: `EXPANSION_MAX` and `CompiledView::expansion` in `uze-contract`, the binder's step budget, and the compiler's `refuse_fan_out`
- [x] 1.3 Exploit re-run: `poc/mkmarket.sh 12 10`, then `uze agent plugin check` and `uze install` in the sandbox

## 2. SCRIPT-2 - allocation before the size check

- [x] 2.1 Regression tests: `crates/uze-script/tests/sizes.rs` (one per shadowed function, and the shadows' semantics), failing on the audited commit
- [x] 2.2 Fix: `primitives::register_sized` shadows `replace` (string and character finds), `map` and `zip`; the package table in design.md
- [x] 2.3 Exploit re-run: `run.sh t_replace.rhai` in the sandbox

## 3. SCRIPT-1 - resident nesting overflows the stack

- [x] 3.1 Regression tests: `crates/uze-script/tests/depth.rs`, failing (aborting) on the audited commit
- [x] 3.2 Fix: `engine::STACK` for the resident thread and every answer, and the `this` depth check with `Limit::Nesting`
- [x] 3.3 Exploit re-run: the audit's `deep_nesting_across_resident_calls` against the fixed library, in the sandbox

## 4. EXT-2 - escline bypasses the slashdash guard

- [x] 4.1 Regression test: `crates/uze-core/src/capability/extension/bounded.rs::tests::an_escline_does_not_hide_a_slashdashed_block_inside_another`, failing on the audited commit
- [x] 4.2 Fix: the scanner's `continued` state and `line_break`
- [x] 4.3 Exploit re-run: `gen/mk2.py 14` view checked in the sandbox

## 5. Proof

- [x] 5.1 Gates: build, the touched crates' tests, clippy, fmt, `openspec validate audit-bounded-extensions --strict`
- [x] 5.2 evidence.md for every finding
