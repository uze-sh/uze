## WKS-2

- **Fix**: 1c95c06c, f4d5f503, 7af6fd17. Every Git process `uze-git`
  starts runs with `core.hooksPath` at the null device, no fsmonitor,
  `protocol.ext.allow=never` and no submodule recursion. A command that can
  reach a filter, merge driver, signing program or transport first asks Git
  which program keys are set and in which scope; the repository's filters
  and merge drivers are answered with nothing (`false` for a driver), and
  the fixed-name transport and signing keys are always given from the
  operator's scopes.
- **Regression test**: `crates/uze-git/src/lib.rs::tests::a_write_runs_no_hook_the_repository_names`,
  `::no_filter_or_merge_driver_the_repository_defines_runs`,
  `::a_transport_the_repository_names_never_runs`
  (`cargo test -p uze-git`). Run against the audited `uze-git` (tests
  grafted onto `e295bfe3`'s `lib.rs`), all three fail:
  ```
  `git worktree add -q -b placed …/linked` ran "…/armed.sh reference-transaction … …/armed.sh post-checkout"
  a placement ran "…/armed.sh \n"
  tests::a_transport_the_repository_names_never_runs … (the planted upload pack ran and hung the fetch)
  test result: FAILED. 0 passed; 3 failed
  ```
- **Exploit re-run**: `poc2-hooks.sh` in the `wks` sandbox, fixed binary:
  ```
  ! setup was not run: this project's commands await your approval …
  split exit=0
  --- canary:
  cat: /home/attacker/work/PWNED-WKS2: No such file or directory
  ```
  `poc5-clock-fetch.sh` (the `ext::` remote, PLAUSIBLE at audit time):
  `no canary`; the `ext::` refusal is pinned by the regression test.
- **Residual**: Git LFS's `pre-push` upload does not run in UZE's delivery
  push (D1). Further findings on this surface, found while fixing it and not
  yet fixed, are tracked privately in GHSA-97f7-95q3-wcjw until their fix
  ships (`AGENTS.md`, Security findings).

## WKS-1

- **Fix**: a7bacb2c. `SessionId::new` is replaced by `SessionId::parse`
  (1-128 ASCII letters, digits, `-`, `_`, first a letter or digit), which
  serde also goes through. Claude Code, Codex and Antigravity read-backs
  require a UUID; OpenCode requires `ses_` and letters and digits.
- **Regression test**: `crates/uze-core/src/delivery/session.rs::tests::a_value_shaped_like_an_option_or_a_path_is_not_an_identifier`,
  `::a_stored_identifier_is_read_through_the_same_check`,
  `crates/uze-integrations/src/codex/session.rs::tests::a_rollout_naming_an_option_is_never_observed`,
  `crates/uze-integrations/src/claude/session.rs::tests::a_transcript_not_named_by_claude_code_is_never_observed`,
  `crates/uze-integrations/src/antigravity/session.rs::tests::an_index_entry_not_named_by_the_harness_is_not_a_conversation`,
  `crates/uze-integrations/src/opencode/session.rs::tests::only_an_identifier_of_opencode_s_shape_is_a_conversation`
  and `::a_listed_id_of_another_shape_is_never_observed`, which drives the
  same selection `observe` makes (`cargo test -p uze-core session`,
  `cargo test -p uze-integrations session`).
  Not run red on the audited commit, where `parse` did not exist and they
  do not compile; that they fail there rests on reading it: `SessionId::new`
  accepted any string and `observe` returned the newest rollout, so the
  Codex, Claude and Antigravity tests observe the planted id. The OpenCode
  observation test was run red against this branch with the shape filter
  swapped for the bare `SessionId::parse`.
- **Exploit re-run**: `poc3-resume-inject.sh` in the `wks` sandbox, fixed
  binary; the second launch no longer carries the planted id:
  ```
  --- launch 2 (operator relaunches / continues the agent):
  REAL codex argv: -c hooks.SessionStart=[…] -c hooks.state={…} -c notify=[…]
  ```
  (audited: `resume --dangerously-bypass-approvals-and-sandbox -c hooks.SessionStart=…`).
- **Residual**: none. No end-of-options marker is added before the id
  (D5); the type makes an option-shaped id impossible.

## WKS-3

- **Fix**: 6a514060. `forget_unpublished` runs `ls-remote --heads --
  <push URL> <patterns>`. A sweep of `uze-workspace` and `uze-git` for
  other values read back from Git and re-passed found none without `--`,
  `--end-of-options`, a validated name or a full object id.
- **Regression test**: `crates/uze-workspace/src/landing/tests.rs::a_push_url_shaped_like_an_option_runs_nothing`
  (`cargo test -p uze-workspace a_push_url_shaped`). With the `--`
  removed it fails: `the push URL ran as an upload pack`.
- **Exploit re-run**: the finding's own Git-level reproduction, in the
  `wks` sandbox (`poc6-lsremote-argv.sh`), with the audited argv and the
  fixed one side by side:
  ```
  fatal: strange pathname '--upload-pack=touch /home/attacker/work/PWNED-WKS3-fixed; false' blocked
  PWNED-WKS3-audited
  ```
  An end-to-end run through the workspace's sync clock was attempted
  (`poc6-pushurl-option.sh`) but the TUI in the sandbox never reached the
  sync on the audited binary either, so it proves nothing either way; the
  regression test drives the real `forget_unpublished`.
- **Residual**: none.

## Gate

- `cargo build --locked --bin uze`: ok.
- `cargo test --no-fail-fast -p uze-git -p uze-core -p uze-integrations
  -p uze-workspace -p uze-application`: all pass except
  `opencode::project_agents::tests::the_section_registers_exactly_what_the_rust_reading_projects`,
  which fails on this machine because `bun`'s mise shim is broken
  (environmental, untouched by this change).
- `cargo test --test integrations`, `--test architecture`, `--test
  acceptance`, `--test lifecycle`: ok.
- `cargo clippy --all-targets -- -D warnings` and with `-p` for each
  touched crate: ok. `cargo fmt --check`: ok.
- `openspec validate audit-repository-git-execution --strict`: valid.
- The full `cargo test --workspace` is left to the parent, per the brief.
