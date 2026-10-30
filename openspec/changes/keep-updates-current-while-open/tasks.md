## 1. Cheaper requests

- [x] 1.1 Read the `releases/latest` redirect without following it, and test the version read from a `Location`
- [x] 1.2 Fetch a marketplace mirror with branch and tag refspecs only, and test that a `refs/pull/*` on the origin is not fetched

## 2. Release check

- [x] 2.1 Name `state/update.lock` and `state/marketplace-fetch.lock` in `UzeHome`
- [x] 2.2 `UZE_RELEASE_CHANNEL` (`latest` default, `stable`): a found release is settled at once on `latest`; test both
- [x] 2.3 A due check takes the lock with `try_lock`, stamps `checked_at`, then asks; a held lock skips the network; test both
- [x] 2.4 `watch` wakes every 15 minutes ±20% instead of every hour

## 3. Plugins while the workspace is open

- [x] 3.1 `MarketplaceCatalogues` refreshes a stale Git marketplace under the fetch lock, stamping the attempt first, and forgets its memo
- [x] 3.2 `Plugins::keep_current` fetches the stale marketplaces, then runs `auto_update`; test that a pushed revision is applied without reopening
- [ ] 3.3 The workspace runs `keep_current` on its tick thread and raises a toast for what was applied and a staying one for what waits for consent

## 4. Documentation and checks

- [ ] 4.1 `docs/versioning.md`: the channel and the cadence
- [ ] 4.2 `openspec validate keep-updates-current-while-open --strict`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, targeted tests
