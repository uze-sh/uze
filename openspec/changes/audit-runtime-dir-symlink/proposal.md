## Why

- **PLAT-1**: a runtime directory's name is followed before it is judged, which
  breaks the property `private_directory` exists to hold: a link is refused
  without changing anything.

The property this change establishes: on the runtime directory path, the entry
itself is asked about before anything is created or changed, and a link is refused
with what it reaches left as it was.

## What Changes

- The endpoint's and socket directory's path makes a missing directory without
  following a link and never narrows through one.
- Narrowing a directory of this user's goes through a descriptor opened without
  following, and only if it is still the directory judged.
- An address whose directory is a link, with no other place to remember its
  fallback in, is refused instead of written through.

## Capabilities

### New Capabilities

### Modified Capabilities
- `terminal-runtime`: a runtime directory is never reached through a link.

## Impact

`crates/uze-platform` (`endpoint/unix.rs`, tests in `endpoint.rs`). The general
`fs::create_private_dir_all` keeps following a link for its other callers, all of
which name a path under `UZE_HOME`, where a person may link on purpose.
