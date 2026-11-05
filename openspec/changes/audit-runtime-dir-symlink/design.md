## Context

See proposal.md for the finding. `usable` is the one way into a runtime directory:
`endpoint`, `restore` and, through `shared_private_directory`, `user_socket_directory`.
`private_directory` already judged the entry correctly, but ran last.

## Goals / Non-Goals

**Goals:** nothing on this path is created or changed through a link.

**Non-Goals:** `fs::create_private_dir_all` itself. Its other callers name paths under
`UZE_HOME`, and following a link there is what a person who linked their home wants.

## Decisions

- **`usable` creates with a plain recursive `DirBuilder`, only when `lstat` says the
  entry is absent, and no longer narrows.** `mkdir` does not follow a link at its last
  component, and a `DirBuilder` never changes what is there, so all narrowing is left
  to `private_directory`, after the entry is judged. A separate symlink check in front
  of `create_private_dir_all` was the alternative, and it would leave its `chmod` one
  race away from a link.
- **`private_directory` narrows through a descriptor.** Opened with `O_NOFOLLOW |
  O_DIRECTORY`, compared by device and inode with what `lstat` judged, then `fchmod`.
  In a sticky directory nobody else can swap this user's entry, but `TMPDIR` is any
  directory a person named, and a path `chmod` there would follow what stands at the
  name by then.
- **The record's directory is proven before the fallback is made.** At that point
  `usable` replaces the following call. It runs first so that a refusal leaves no
  unpredictable directory behind nobody would ever find. A directory neither usable nor
  able to hold a record has nowhere to remember the fallback, so every later caller would
  make another: refusing is the only answer that does not split a workspace.

## Risks / Trade-offs

- [An address whose directory is a person's link, too long for a socket, with every
  shared name taken, is now refused] → It was narrowed and written through before. A
  directory that fits and every shared name that is free behave as they did for a link:
  stepped over.
- [A directory at a runtime name with a mode that gives others access while denying its
  owner read (`0077`) can no longer be opened to narrow] → It is refused and the next
  candidate is used, as any refused entry is.
- [A read-only directory of this user's, open to others, becomes `0700` here, where the
  general call kept the owner's bits] → It could not hold a socket either way; this is
  what `private_directory` already did on the remembered path.
