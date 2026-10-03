## 1. Delivery report model

- [x] 1.1 The effective view carries, per capability, the exposed name
  (`CapabilityDelivery::exposed_name`, asked of the integration) and the
  plan's `CompatibilityRoute` with its evidence; a degraded agent's evidence
  leads with the fields not carried, and a name held by something UZE does
  not own is reported as blocked.
- [x] 1.2 Each integration fills it; Claude names explicit vs generated
  envelope and why; loose fallback carries its reason
  (`deliver-the-whole-plugin` 6.1).
- [x] 1.3 `render_add_report` (`src/cli/report.rs`, `render_deliveries`)
  and update rendering (`delivery_issues`) list every harness, its route and
  why, every attachment, every blocked and every non-native capability; the
  per-harness `BTreeMap` collapse is gone; JSON mirrors the text.

## 2. Effective view

- [x] 2.1 `uze inspect <plugin> --harness <h>` builds plans without
  attaching; classify in `command_performance.rs`.
- [x] 2.2 Test: inspect and install report agree for a composed package on
  each integration (fake harnesses).

## 3. Failure is not success

- [x] 3.1 Install rolls back receipts and a Store entry it created when no
  harness attached.
- [x] 3.2 Partial delivery record (`UndeliveredRegistry`, state tier,
  `Shaped`), shown by `status -m`, `inspect` and `doctor` (which leaves that
  harness to the record), cleared per harness when a
  later delivery reaches it.
- [x] 3.3 Tests: sole-harness failure leaves nothing
  (`an_install_the_only_harness_refuses_is_not_installed`); two-harness
  partial is reported
  (`an_install_one_harness_refuses_is_listed_as_partially_delivered`);
  pre-existing package survives a failed reinstall
  (`a_failed_reinstall_keeps_the_package_that_was_there`).

## 4. Update by content

- [x] 4.1 `update_machine` compares Store content digests; linked edit
  reports updated
  (`a_machine_update_of_a_linked_edit_reports_the_package_updated`).

## 5. Doctor against intent

- [x] 5.1 Doctor compares expected capabilities per harness with present and
  readable ones, reporting missing, renamed and unreadable findings;
  per-integration readability checks through `IntegrationPort::unreadable`
  (Claude cache, label rule, OpenCode agent fields).
- [x] 5.2 Test: an empty plugin cache and an agent OpenCode drops are both
  reported as unreadable
  (`doctor_reports_an_empty_plugin_cache_and_an_unreadable_agent`); a
  missing agent and a renamed hook entry are reported
  (`doctor_reports_an_agent_whose_file_is_gone_as_missing`,
  `doctor_reports_a_hook_delivered_under_another_name_as_renamed`).

## 6. Documentation reachable from the install

- [x] 6.1 Root help and `uze doctor` name the docs URL (`DOCUMENTATION_URL`
  in `src/cli/help.rs`; `doctor_ends_with_the_documentation_url`), the
  `uze:author` skill links the plugin authoring and format pages on uze.sh,
  and `plugins/uze/README.md` links absolute URLs instead of `../../docs`.

## 7. Verification

- [x] 7.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`,
  `openspec validate honest-delivery-reports --strict`.
