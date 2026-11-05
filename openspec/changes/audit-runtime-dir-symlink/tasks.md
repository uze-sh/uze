## 1. PLAT-1 - a runtime directory is judged before anything follows it

- [x] 1.1 Regression tests in `crates/uze-platform/src/endpoint.rs`: `a_link_at_a_socket_directory_name_is_refused_and_what_it_reaches_keeps_its_mode`, `an_endpoint_directory_that_is_a_link_is_refused_and_what_it_reaches_keeps_its_mode`, `an_address_that_can_only_be_remembered_through_a_link_is_refused`, `restoring_over_a_link_is_refused_and_what_it_reaches_keeps_its_mode`
- [x] 1.2 Fix: `usable` and `private_directory` in `crates/uze-platform/src/endpoint/unix.rs`, with the record's directory proven through `usable`
- [x] 1.3 Keep what worked: `this_users_own_directory_at_a_runtime_name_is_narrowed_and_used`, `a_runtime_directory_not_there_yet_is_made_private_with_its_parents`
- [x] 1.4 Read the other callers of `fs::create_private_dir_all` and leave them unchanged
- [x] 1.5 Exploit re-run through the crate's API
