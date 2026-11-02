---
trigger: glob
globs: "**/crates/uze-document/**"
---

`uze-document` is the one rule for what another build left: the shape a
record declares, the ladder that carries it across, the floor beneath that,
and (`earlier`) the release floor bounding every piece of support for an
earlier release. It names no domain, no path and no harness, because the
terminal runtime holds the workspace and reaches nothing of UZE's.

- **A record written in a shape this build knows is carried across, and
  nothing is said about it.** Carrying a record across is the product
  working, not an event.
- Setting one aside is the *floor*, reached only by a shape with no rung:
  what the world still knows is reconstructed and only the residue
  reported.
- A record from a *newer* build is never taken: two builds on one machine
  is the daily state of this repository.
- The shape is read before the record; a record with no shape is shape 1.
- A ladder step declares how long it `lasts`, and is deleted when the
  release floor passes it; a record rewritten only when next changed is
  `Unbounded`. The current struct stays clean *because* old shapes live in
  the ladder; scattered compatibility code elsewhere is refused (the
  `earlier-releases` rule).
