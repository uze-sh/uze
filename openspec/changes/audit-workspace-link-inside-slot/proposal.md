## Why

- **WKS-1**: a declared link's destination is reached through whatever the
  checkout's tree holds on its path, which breaks the property that what UZE
  prepares in a checkout stays inside that checkout.

The property this change establishes: every directory UZE makes or writes
through, to prepare a checkout from the primary, is a real directory of that
checkout's own, asked about without following a link.

## What Changes

- A declared link whose path in the checkout runs through a link, or through
  anything but a directory, is not made, and the checkout says so with a
  warning; nothing is created where that link points.
- A link's missing directories are made one level at a time, each asked about
  first, rather than all at once through whatever stands on the way.
- An untracked change carried into a checkout is refused the same way, and also
  when the checkout holds a link at the file itself.
- A link whose directories are real directories of the checkout is made exactly
  as before.

## Capabilities

### New Capabilities

### Modified Capabilities
- `worktree-policy`: what a checkout is prepared with from the primary lands
  only inside that checkout.

## Impact

`crates/uze-workspace` (`checkout/pool.rs`: `materialize` and a helper the copy
shares; `checkout/lifecycle.rs`: `carry_untracked`; tests in `checkout/tests.rs`).
A project that links through a directory its own tree commits as a link loses
that link, with a warning.
