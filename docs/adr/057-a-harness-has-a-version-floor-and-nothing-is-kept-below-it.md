# A harness has a version floor, and nothing is kept below it

Status: Accepted

## Context

Each integration was written against whatever vendor release was current
when it was measured, and nothing said which releases it still served. When
a vendor changed a surface UZE delivers through (Antigravity's `agy plugin
list` became JSON in 1.3.x; its plugins began loading their own
`hooks.json`), the integration had two options: keep the older path beside
the new one, branching on the detected version, or stop serving the older
release.

Keeping both is the scattered compatibility this project refuses for
records (ADR-052): every vendor release adds a branch nobody removes, the
Lab proves only the newest one, and the older path rots unproven.

## Decision

**Each integration declares the oldest harness version its deliveries are
written for, and no path is kept for a version below it.**

- The floor is a port method (`IntegrationPort::minimum_version`) returning
  a version the core compares as plain dotted numbers, knowing nothing of
  the vendor. `None` declares no floor.
- `uze doctor` and `uze status` report a harness below its floor with both
  versions, and an operation refused because of it names the version
  needed. Below the floor nothing is worked around.
- A version UZE cannot read meets the floor, so a vendor changing its
  `--version` output never blocks anyone.
- Raising a floor deletes the code that served the releases beneath it, in
  the same change; the floor is stated in the release notes.

The floor is to harness behaviour what the bottom rung of a record's ladder
is to persisted shapes: the one place an old world is acknowledged, and the
reason nothing else has to acknowledge it.

## Consequences

An integration is written for one behaviour per surface, and the Lab
proves the code that runs. A vendor change that UZE adopts costs a floor
bump instead of a branch, and the operator on an older release is told to
upgrade rather than served a path nothing proves.

The cost is that an operator who cannot upgrade a harness loses UZE's
delivery to it until they do. That is accepted: a delivery UZE cannot prove
is not one it should make.

Floors set by the change that introduced this: Antigravity 1.3.4. Claude
needs none, since older Claude ignores the block-on-failure marker rather
than refusing it.

Source change: openspec/changes/archive/2026-10-16-refresh-harness-deliveries/
