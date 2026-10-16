# uze-testkit

Shared test infrastructure, so every test binary in the workspace has one
notion of isolation.

- `temp::TestEnvironment` — owns HOME, UZE_HOME, PATH head, cwd and harness roots; guards the developer's real home
- `env::scope` — serializes and restores process-global env mutation
- `git::Repository` — a real scratch repo with every ambient Git config neutralized
- `fake_harness::FakeHarness` — an executable with a rule table and an invocation log
- `fixtures` — resolves `tests/_fixtures/**` from any crate
- `scenario::Scenario` — a deliberate system state from a few declarative steps
- `forge` — a forge on loopback (smart HTTP, SSH, a proxy) with nothing leaving the machine
- `marketplace` — stages a marketplace so a test installs the way a user does (`uze market add`, then `uze install -m`)
- `assertions` — semantic assertions that name the fixture, harness or path involved

Test-only; depends on nothing product-specific but the Git transport,
`uze-platform`, and `uze-terminal`'s launch variables (so a test process never
inherits a pane's stamp).

```bash
cargo test --workspace --no-fail-fast
```

See `tests/README.md` for the L0–L4 taxonomy.
