## MODIFIED Requirements

### Requirement: A vendor installer's output is logged, never interleaved

The vendor installer's output SHALL NOT interleave with UZE's report: it
SHALL be written to a per-harness log,
`$UZE_HOME/logs/setup/<harness>.log`, replaced on each run, and the
installer SHALL run in a session of its own with no controlling terminal
and no standard input. The setup report SHALL name that log for each
harness.

#### Scenario: Setup runs a vendor installer

- **WHEN** `uze setup opencode` runs OpenCode's official installer
- **THEN** the installer's output is written to
  `$UZE_HOME/logs/setup/opencode.log`, replacing the previous run's
- **AND** none of it appears in UZE's report, which names that log.
