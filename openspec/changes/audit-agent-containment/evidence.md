Re-runs used the audit sandbox (`$AUDIT/fix-containment`, bubblewrap) with
the binary built from `64c3dda6` (TERM-1 re-run) and `2adab0a9` (the
others; `64c3dda6` changes only the library's public `connect`, which the
binary does not call). The kernel is WSL 6.18 (Landlock ABI ≥ 6). Inside
bubblewrap every process runs under `no_new_privs`, so the re-runs launch
the PoCs the way the workspace launches an agent: as a program pane from a
persisted `workspace.json`. The attacker client was rebuilt with protocol
version 24 and given a `door` mode that reads `workspace.operator` and
connects to the person's endpoint.

## TERM-1

- **Fix**: `9f789b2c`, `2adab0a9`, `64c3dda6`. Program panes start in a
  Landlock domain scoping abstract sockets; attach and stop are served only
  at a random abstract endpoint that domain cannot reach; the shared socket
  refuses them while it stands; an unconfined client refuses a confined
  server or endpoint peer.
- **Regression test**:
  `crates/uze-terminal/src/runtime/tests.rs::a_process_that_escapes_its_pane_cannot_attach_even_with_the_key`
  and `crates/uze-platform/src/seal/linux.rs::a_sealed_process_cannot_reach_the_endpoint_however_it_leaves`;
  `cargo test -p uze-terminal --lib a_process_that_escapes` and
  `cargo test -p uze-platform --lib seal`. Shown failing by disabling the
  fix (`seals: false` in `Server::new`): `left: "unreachable", right:
  "sealed off"`. On the audited commit the escaped helper's keyed attach is
  served (the finding's PoC), where the test asserts `refused`.
- **Exploit re-run**: `run-term1.sh` (`pane.sh` plus the person's endpoint):
  ```
  === in-pane direct (expect REFUSED) ===
  ... a process inside a uze pane may open a space or read usage and nothing else; ...
  === escaped setsid --fork (reparented to init) ===
  === escaped setsid --fork, person endpoint [published] ===
  NoNewPrivs:	1
  ... this endpoint serves what a pane may ask and nothing else; attach from a terminal of your own
  attacker connect err (door): Operation not permitted (os error 1)
  --- canaries
  no PWNED file
  ```
  Control (`run-control.sh`, the same helper run outside every pane):
  served, `PWNED-CONTROL` created.
- **Residual**: an agent typed into the person's own unsealed shell pane, or
  any process outside every sandbox and pane (the control above), keeps
  ADR-056's model, by design (D3). A sealed agent can kill the server
  (signals not scoped, D5): a DoS, and a server it restarts inside its domain
  is retired by the person's next client. Ways around the seal found while
  fixing it are not closed by this change and are tracked privately in
  GHSA-w6jq-59jg-h4pg until their fix ships (`AGENTS.md`, Security findings):
  **TERM-1 is partially closed.**

## TERM-2

- **Fix**: `2adab0a9`. `OpenSpace`-created spaces are unvisited until a
  client selects one: at most 3 per asking pane and 8 in total, the rest
  refused, and none persisted until visited.
- **Regression test**:
  `crates/uze-terminal/src/runtime/tests.rs::spaces_opened_unseen_are_bounded_and_never_persisted`
  and `::one_pane_is_held_to_its_own_bound_of_unseen_spaces`;
  `cargo test -p uze-terminal --lib unseen`. Not run red on the audited
  commit (the bound did not exist there): the claim that they fail rests on
  the finding's evidence, 25 wire `OpenSpace` requests served and persisted
  where the test asserts 8 and 1 persisted.
  `::a_space_whose_shell_failed_to_start_is_still_bounded_and_never_persisted`
  covers a space whose shell did not start (found by the branch's review):
  run red before the fix, 11 of 11 opened and none refused.
- **Exploit re-run**: `run-term2.sh` (`pane_dos.sh`, 25 `OpenSpace` from a
  program pane):
  ```
  DRIVER pane UZE_PANE=1 sid=11 -- NO key used
  spaces persisted: 1
  processes in the box: 11   (3 space shells: the per-pane bound)
  after restart-free stop, spaces persisted: 1
  ```
  Audited: 25 spaces persisted, 33 processes.
- **Residual**: an unvisited space's shell starts in a directory the pane
  chose, unsandboxed and given no input (ADR-056's stated `.envrc`
  residual), now at most 8 at once and never relaunched.

## WKS-4

- **Fix**: `9f789b2c`, `2adab0a9`. `asker()` also refuses a process the
  kernel reports under `no_new_privs`; refusal wording says why.
- **Regression test**:
  `tests/acceptance/command_approval.rs::a_confined_process_cannot_approve_stripped_of_its_stamp_and_given_a_terminal`
  (`cargo test --test acceptance a_confined_process_cannot_approve`),
  shown failing with the audited `src/asker.rs` restored (the approval goes
  through: `panicked at tests/acceptance/command_approval.rs:107`). Also
  `src/asker.rs::tests::a_confined_process_is_never_the_operator_whatever_its_environment`.
  `crates/uze-core/src/machine/home.rs::tests::nothing_named_writable_to_a_harness_lies_under_state`
  pins that UZE names no harness a writable root under `state/`; it passes
  on the audited commit too: no narrowing was needed.
- **Exploit re-run**: `run-wks4.sh`:
  ```
  --- as the agent, env stripped + pty:
  error: an agent cannot approve the commands its project runs, nor can anything in a sandbox; ...
  exit=1
  --- record:
  === poc1b-forge.py: write the record by hand (same-uid writer, the stated residual)
  ✓ nothing to approve: ~/work/proj1 declares no command this machine runs that waits for an answer
  ```
- **Residual**: the forged record (`poc1b`) still approves: an agent whose
  harness does not confine writes can write `$UZE_HOME/state`, and no
  disk-only scheme stops a same-uid unconfined writer (ADR-055's stated
  boundary). An unsandboxed agent in the person's unsealed shell pane has
  no kernel marker and can still strip its stamp. A person in a sandboxed
  terminal (Flatpak) is refused and answers elsewhere (D6).

## WKS-5

- **Fix**: `8696b95d`. A record with no key digest admits no claim; a split
  subagent is issued a key nobody holds.
- **Regression test**:
  `crates/uze-workspace/src/conversation.rs::a_record_issued_no_key_admits_no_claim`
  and `crates/uze-application/src/application/services/work/tests.rs::a_subagents_record_admits_no_claim_without_its_key`
  (`cargo test -p uze-workspace --lib a_record_issued_no_key`,
  `cargo test -p uze-application --lib a_subagents_record`). Both run
  before the fix and failed (`left: Some(AgentId("kmpimi")) right: None`;
  `a split child is issued a key`).
- **Exploit re-run**: `run-wks5.sh` (`poc4-child-impersonation.sh` against
  the audited box's key-less child, then a child split by this build):
  ```
  zvtwtn w1 False
  error: this process is not an agent UZE launched
  list exit=1
  error: this process is not an agent UZE launched
  a7qtnz w1 True
  error: this process is not an agent UZE launched
  list exit=1
  error: this process is not an agent UZE launched
  ```
  (`split exit=0` in the transcript is `tail`'s status; the split printed
  the refusal and made no checkout.)
- **Residual**: none for the claim. An agent placed before ADR-056 and never
  re-placed starts its harness fresh on relaunch (D1).

## Gate

- `cargo build --locked --bin uze`: ok.
- `cargo test -p uze-platform -p uze-core -p uze-workspace -p uze-application`: ok.
- `cargo test -p uze-terminal`: 169 passed, 1 failed:
  `foreground_status_prefers_the_shim_identity_over_a_version_named_comm`,
  which fails identically on the parent commit with this change's terminal
  files reverted (environmental on this machine, not this change).
- `cargo test --lib` (root crate): 1286 passed.
- `cargo test --test acceptance --test cli --test architecture`: ok.
- `cargo check -p uze-platform -p uze-terminal --tests` for
  `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`: ok.
- `cargo clippy --all-targets -- -D warnings`: ok. `cargo fmt --check`: ok.
- `openspec validate audit-agent-containment --strict`: valid.
