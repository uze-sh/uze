---
trigger: glob
globs: "**/conformance/**, **/.github/workflows/conformance*.yml"
---

The Harness Conformance Lab proves what each real harness does, in a
disposable Docker environment: the real harness binary, a synthetic
provider, zero Internet, zero tokens. Vendor-specific by design and never
linked into the deterministic suite.

- `contract/` states what **every** harness must prove, in outcome terms,
  and names no vendor. `harnesses/<vendor>/bindings.py` says how that
  harness is driven and carries no assertion. `scenarios.py` keeps only
  what is genuinely unique to the vendor.
- A harness that cannot deliver part of a contract declares it through
  `bindings.unsupported` with a reason, which the run records; never by
  omitting the check.
- Fixtures by owner: the Lab's marketplace in
  `conformance/_fixtures/marketplace/`; per-harness session and provider
  state under `conformance/harnesses/<vendor>/fixtures/`; the
  deterministic suites' inputs stay in `tests/_fixtures`.
- `python3 conformance/lab.py --harness <h>` runs one vertical, as two legs
  (`--part contract|vendor`); `make lab-replay` replays a recorded run. A
  leg that already passed against the same inputs is reused
  (`.github/scripts/proof.py`); the nightly always runs.
  `conformance-stability.yml` is ADR-035's promotion gate (3 clean runs per
  vertical), nightly only.
- The image bakes a release `uze` built from the checkout and is rebuilt
  when its `sh.uze.lab.source` label differs from the checkout (uncommitted
  files included); a run's `provenance:` line says which binary it tested,
  and `UZE_LAB_IMAGE` pins another image only on purpose.

Debugging a failure starts with the `conformance-debug` skill.
