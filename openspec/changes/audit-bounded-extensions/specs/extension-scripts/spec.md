## Purpose

How a declared extension's script runs: compiled once, answered in an
engine bounded in operations, time, depth and sizes, reaching only what its
grants register.

## ADDED Requirements

### Requirement: A script's work is bounded before it allocates
A function a script may call SHALL check that its result fits the string,
array and map bounds before building it, or as it builds it, never after:
no call SHALL allocate more than a small multiple of those bounds. Every
script SHALL run on a thread whose stack is sized for the deepest value the
engine's bounds allow, whichever thread asked for the answer.

#### Scenario: A replace that would build gigabytes is refused at once
- **WHEN** a script replaces each `a` of a 512 KiB string with 8 KiB
- **THEN** the call fails at the string size bound at once, and the process keeps running

#### Scenario: A map collecting fresh large strings is refused as it collects
- **WHEN** a script maps ten thousand items to fresh 512 KiB strings
- **THEN** the call fails at the string size bound after the first few, not after holding gigabytes

#### Scenario: A deep value built in one answer is compared without overflow
- **WHEN** one answer nests a value hundreds of levels deep and compares and prints it
- **THEN** it answers, or fails at its deadline, and the thread that asked is unharmed
