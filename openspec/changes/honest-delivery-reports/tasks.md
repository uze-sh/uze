## 1. Delivery report model

- [x] 1.1 Exposure plans carry the exposed name and a status with reason
  (native / adapted with fields not carried / shadowed / unsupported).
- [x] 1.2 Each integration fills it; Claude names explicit vs generated
  envelope and why; loose fallback carries its reason
  (`deliver-the-whole-plugin` 6.1).
- [x] 1.3 `render_add_report` and update rendering list every harness and
  every non-native capability; drop the per-harness `BTreeMap` collapse; JSON
  mirrors the text.

## 2. Effective view

- [x] 2.1 `uze inspect <plugin> --harness <h>` builds plans without
  attaching; classify in `command_performance.rs`.
- [x] 2.2 Test: inspect and install report agree for a composed package on
  each integration (fake harnesses).

## 3. Failure is not success

- [x] 3.1 Install rolls back receipts and a Store entry it created when no
  harness attached.
- [x] 3.2 Partial delivery record (state tier, `Shaped`), shown by
  `status -m`, `plugin list`, `inspect`, `doctor`, cleared on full delivery.
- [x] 3.3 Tests: sole-harness failure leaves nothing; two-harness partial is
  reported; pre-existing package survives a failed reinstall.

## 4. Update by content

- [x] 4.1 `update_machine` compares Store digests; linked edit reports
  updated.

## 5. Doctor against intent

- [x] 5.1 Doctor compares expected capabilities per harness with present and
  readable ones; per-integration readability checks (Claude cache, label
  rule, OpenCode agent fields).
- [x] 5.2 Test: empty plugin cache and an unreadable agent are reported.

## 6. Documentation reachable from the install

- [x] 6.1 `uze --help` (after_help) and `uze doctor` name the docs URL;
  `plugins/uze/README.md` and the `uze:author` skill link to the plugin
  format and harness matrix pages on uze.sh instead of repo-relative paths.

## 7. Verification

- [ ] 7.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`,
  `openspec validate honest-delivery-reports --strict`.
