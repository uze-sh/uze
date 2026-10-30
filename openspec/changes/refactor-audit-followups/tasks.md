## 1. redact out of uze-platform

- [ ] 1.1 Decide the home (new leaf crate vs recorded exception) per design.md, and record the choice in decisions.md
- [ ] 1.2 Move `redact` and its tests; update the callers and the architecture suite's leaf list
- [ ] 1.3 `cargo test -p uze-git -p uze-core -p uze-application`, `cargo test --lib telemetry`, `--test architecture` green

## 2. A complete CodeIdentity only

- [ ] 2.1 Check whether `CodeIdentity` is persisted (receipts, lock, state); if so stop and record it in decisions.md as a format question
- [ ] 2.2 `command_execution` takes the package digest; `IntegrationPort::package_executions` passes it; remove the placeholder and the replacing step
- [ ] 2.3 Integrations and packages suites green, including the DELIV-1 and ACQ-1 regression tests

## 3. Hash only when trust needs it

- [ ] 3.1 Measure `uze update` on a local-path plugin and `uze install` of a skills-only package (before)
- [ ] 3.2 Make the digest lazy in `authorize`; measure again and put both numbers in the commit body
- [ ] 3.3 Packages and lifecycle suites green
