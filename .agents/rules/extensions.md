---
trigger: glob
globs: "**/crates/uze-extensions/**, **/src/ui/extension*"
---

An extension never draws, and reaches nothing it was not handed. It answers
with a `view::View` (a full-frame surface) or a `view::Section` (a
collapsible block of one of the host's own columns); the host owns
rendering, geometry, hit-testing and the palette, and
`src/ui/extension_view.rs` renders both. Every other capability (running
Git, reading a file, resolving `$HOME`) arrives through
`uze_extensions::Host`, implemented in `src/ui/extension_host.rs`.
`uze-extensions` depends on no UZE crate and names no process, filesystem
or environment API; the architecture suite fails on each. An extension is
code UZE runs in its own process, a different trust class from plugin bytes
a harness reads (ADR-041).

- One directory per extension (`code/`, `architect/`, `spec/`) with its
  surface in the file beside it, one `ExtensionRegistry::builtin` entry,
  one `ExtensionHit` variant per surface it draws. `view.rs` and `Host` are
  the two contracts and sit at the crate root.
- `Host` grants every capability, including the five that write
  (`write_file`, `delete_file`, `delete_dir`, `rename_path`,
  `restore_to_head`), whose only caller is `code/request.rs`.
- What more than one extension needs lives under `shared/`, and only once
  a second one actually needs it; `shared.rs` says why each module joined.
- `code` is **one** extension covering the checkout's changes, files,
  history and map, because the selection must survive a switch between them
  and a selection cannot live above two extensions. No handler branches on
  the mode to decide what the state *means*: the mode decides who is
  *asked*, and each half answers about the same path knowing nothing about
  the other. The map sits there, not with the diagrams, because every tile
  on it is a path the other halves already answer about.
- `architect` draws what somebody *wrote down* about the project, never
  anything measured. `spec` reads what a checkout *intends*: a spec tool's
  units, found by the tool's marker, and decision records, found by their
  shape in the places `workspace.artifacts` declares.
