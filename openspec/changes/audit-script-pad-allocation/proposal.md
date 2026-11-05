## Why

- **SCRIPT-3**: `pad` reserves bytes by the padding's whole length for each
  character it writes, so a call inside every bound can ask for about 1 TiB,
  and the failed allocation aborts the process.

The property this change establishes: `pad` counts what it writes, and refuses
past the string bound before it allocates.

## What Changes

- `pad` counts the bytes of the characters it will write, refuses a result past
  the string bound with `Limit::StringSize`, and reserves exactly that.

## Capabilities

### New Capabilities

### Modified Capabilities
- `extension-scripts`: padding is bounded by what it writes, before it allocates.

## Impact

`crates/uze-script` (`primitives.rs`, `tests/sizes.rs`). A multi-byte padding
past the bound keeps failing at the string size bound, now before building it.
