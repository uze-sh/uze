## 1. WKS-1 - a checkout is prepared only inside itself

- [x] 1.1 Regression tests in `crates/uze-workspace/src/checkout/tests.rs`: `a_declared_link_is_made_only_inside_its_checkout`, `an_untracked_file_is_carried_only_inside_the_checkout`
- [x] 1.2 Fix: `directories_within` in `crates/uze-workspace/src/checkout/pool.rs`, used by `materialize` and by `carry_untracked` in `checkout/lifecycle.rs`, with the destination asked through `symlink_metadata`
- [x] 1.3 Keep what worked: `a_declared_link_makes_its_missing_directories_inside_the_checkout`, `a_linked_file_reaches_the_primarys_and_a_missing_target_only_warns`, `an_untracked_link_travels_as_the_same_link`
- [x] 1.4 Exploit re-run through the regression tests, against the audited sources and the fix
