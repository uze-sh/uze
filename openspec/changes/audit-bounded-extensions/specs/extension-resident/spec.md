## Purpose

A declared script that keeps its state while its surface is open, reads raw
input while focused and draws on request, each call held to the per-answer
bounds.

## ADDED Requirements

### Requirement: What a resident script keeps stays within a value's depth
After every call, what a resident script keeps on `this` SHALL nest no
deeper than a value may cross to the host (128 levels). A call that leaves
it deeper SHALL stop the session with a `nesting depth` error, which every
later call answers; the session thread SHALL never overflow its stack.

#### Scenario: Nesting across calls stops the session, not the process
- **WHEN** each timer tick nests `this.a` eight levels deeper
- **THEN** the session stops within a few ticks of passing 128 levels with a nesting depth error, and the workspace keeps running
