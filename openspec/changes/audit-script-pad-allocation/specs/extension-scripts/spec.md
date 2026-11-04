## ADDED Requirements

### Requirement: Padding is bounded by what it writes
`pad` SHALL count the bytes of the characters it writes and SHALL refuse a
result past the string bound before allocating. What it reserves SHALL NOT
depend on the padding's length beyond the characters taken from it.

#### Scenario: A long padding pads to the bound
- **WHEN** a script pads a short string to the string bound with a 512 KiB padding
- **THEN** it answers with a string of that length, and the process keeps running

#### Scenario: A multi-byte padding past the bound is refused at once
- **WHEN** a script pads to the bound in characters with a two-byte padding
- **THEN** the call fails at the string size bound without building the result
