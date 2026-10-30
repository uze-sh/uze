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
