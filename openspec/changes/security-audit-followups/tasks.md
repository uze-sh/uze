## 1. Retire only a server started inside a pane

- [ ] 1.1 Test: a server started under `no_new_privs` but not sealed survives an unconfined client's attach (fails today)
- [ ] 1.2 Record `started_inside_pane` at server start and carry it in the handshake; bump the protocol version
- [ ] 1.3 `make_way` retires on the flag only; `cargo test -p uze-terminal` green

## 2. Delivery in an LFS repository

- [ ] 2.1 Test: delivering a checkout with a `filter=lfs` attribute stops before the push with the next step (fails today)
- [ ] 2.2 Detect LFS in `uze-workspace` landing; stop with a message per the design-system skill
- [ ] 2.3 `cargo test -p uze-workspace` and the landing journeys green

## 3. Windows installer `latest`

- [ ] 3.1 Pester: a fake host answering `/latest` with a redirect; install with no `UZE_VERSION`
- [ ] 3.2 Run it in CI's `installer-windows` job

## 4. Open findings (private)

- [ ] 4.1 GHSA-w6jq-59jg-h4pg: fixed as its own `audit-` change, entering the repository with its fix
- [ ] 4.2 GHSA-97f7-95q3-wcjw: same

## 5. redact out of uze-platform

- [ ] 5.1 Decide the home (new leaf crate vs recorded exception) per design.md, and record the choice in decisions.md
- [ ] 5.2 Move `redact` and its tests; update the callers and the architecture suite's leaf list
- [ ] 5.3 `cargo test -p uze-git -p uze-core -p uze-application`, `cargo test --lib telemetry`, `--test architecture` green

## 6. A complete CodeIdentity only

- [ ] 6.1 Check whether `CodeIdentity` is persisted (receipts, lock, state); if so stop and record it in decisions.md as a format question
- [ ] 6.2 `command_execution` takes the package digest; `IntegrationPort::package_executions` passes it; remove the placeholder and the replacing step
- [ ] 6.3 Integrations and packages suites green, including the DELIV-1 and ACQ-1 regression tests

## 7. Hash only when trust needs it

- [ ] 7.1 Measure `uze update` on a local-path plugin and `uze install` of a skills-only package (before)
- [ ] 7.2 Make the digest lazy in `authorize`; measure again and put both numbers in the commit body
- [ ] 7.3 Packages and lifecycle suites green
