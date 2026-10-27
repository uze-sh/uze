## 1. Net and fetch

- [x] 1.1 `Grant::Net(host)` in the contract, decoder and check; consent and `uze status` list the hosts
- [x] 1.2 `fetch` in `uze-script` under `net`, run as `curl` through the program runner (G1): https only, host checked, no redirect elsewhere, bounds

## 2. Secrets

- [x] 2.1 `PreferenceKind::Secret`; `uze-platform::secrets` per platform; set on the Extensions screen, never redisplayed
- [x] 2.2 `fetch`'s `secret` option through `curl`'s config on stdin (G2); answers scrubbed of the value

## 3. Gallery and the fence

- [x] 3.1 Fixture `ci-status` (status row and section, `net:api.github.com`, a `token` secret) with tests on recorded responses
- [x] 3.2 The `kdl` fence coloured in a Markdown file in the editor (parent 23.8)
- [x] 3.3 Decisions recording what is not built: `edit:checkout`, digest pins, pipes, extension CLI commands

## 4. Proof and docs

- [x] 4.1 The extensions rule, invariants, the skill
- [ ] 4.2 `make check`, `openspec validate --all --strict`, and the hand proof script
