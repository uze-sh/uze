## Context

See proposal.md for the finding. `materialize` runs on every placement and
`carry_changes` on a placement that carries the operator's changes. Both join a
relative path onto the slot and made its parents with `create_dir_all`, which
follows a link at any component, and `materialize` asked about the destination
with `exists()`, which follows one too.

## Goals / Non-Goals

**Goals:** nothing a checkout's preparation makes or writes passes through a link
the checkout's tree holds.

**Non-Goals:** the link UZE makes itself, which is meant to point at the
primary; a link the operator committed and links through on purpose, which now
loses that entry with a warning rather than being followed.

## Decisions

- **One helper, `directories_within`, walks the entry's parent one component at a
  time.** Each component is asked with `symlink_metadata`: a link or a
  non-directory refuses the entry, a missing one is made with a single
  `create_dir`, which does not follow a link at its last component. A final
  `resolves_within` of the parent against the slot is a second answer to the
  same question, so a component made by someone else between the two cannot
  carry the entry out unnoticed. Checking the resolved destination alone was the
  alternative, and it would still have `create_dir_all` make directories through
  the link before refusing.
- **Every link is refused, also one resolving inside the checkout.** Telling a
  harmless link from a harmful one needs the resolution to stay true between
  the check and the write; refusing the shape is the one answer that does not
  race, and a project linking a file through a committed link is rare.
- **The destination is asked with `symlink_metadata`.** An entry already there,
  link or not, is still skipped as before. For a carried file, a link at the file
  itself is refused, since `fs::copy` would write through it; a regular file
  there is overwritten as before.
- **The copy uses the same helper.** Its only caller cuts the slot from the
  primary's own `HEAD`, so the trees agree; the check holds without depending on
  that.

## Risks / Trade-offs

- [A project that linked or carried a file through a directory it commits as a
  link loses that entry] → The checkout's warning names it; the file can be
  linked at a real path instead.
- [The swap between asking about a component and making the next one cannot be
  forced by a test] → The final `resolves_within` narrows it; the checkout's
  tree is written by Git at placement, not by a concurrent attacker.
