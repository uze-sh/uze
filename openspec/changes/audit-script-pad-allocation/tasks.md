## 1. SCRIPT-3 - padding reserves by the padding's length

- [x] 1.1 Regression tests: `crates/uze-script/tests/sizes.rs::a_padding_is_counted_by_what_it_writes_not_by_how_long_it_is`, `::padding_refuses_a_result_past_the_string_bound_before_building_it`, `::padding_answers_up_to_the_string_bound_and_refuses_past_it`
- [x] 1.2 Fix: `pad` in `crates/uze-script/src/primitives.rs` counts what it writes and refuses before it allocates
- [x] 1.3 Exploit re-run through the crate's API
