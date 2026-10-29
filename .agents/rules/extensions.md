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
`uze-extensions` depends on no UZE crate but `uze-contract` (plain data:
the manifest, compiled views, the catalog, the sanitizer) and names no
process, filesystem or environment API, nor `kdl`; the architecture suite
fails on each. An extension is
code UZE runs in its own process, a different trust class from plugin bytes
a harness reads (ADR-041).

- One directory per extension (`code/`, `architect/`, `spec/`) with its
  surface in the file beside it, its `contract::Extension` in
  `extension.rs`, and one `ExtensionRegistry::builtin` entry. `view.rs`,
  `contract.rs` and `Host` are the contracts and sit at the crate root.
- The host speaks to every extension, built-in or declared, through
  `contract::Extension` alone, by the slot a surface stands in
  (`src/ui/orchestrator/declared.rs`); its hits are `ExtensionHit { slot,
  hit }`. What it reads is a `Job` it hands over, never a read the host
  spawns for it by name. A gesture or an answer the contract cannot carry
  is a new method with a default that does nothing, not a branch on which
  extension it is.
- A surface keeps two things for the host: `place`, per checkout, handed
  back when a door opens it there again, and `hold`, per tab, taken when
  another tab comes to the front and handed back after `Entry::Return`
  (`src/ui/orchestrator/surfaces.rs`, `settle_pane`). What a door must
  decide afresh (code's half) goes in `hold`, never in `place`. Both stay
  in memory; neither is written down.
- `Host` grants every capability, including the five that write
  (`write_file`, `delete_file`, `delete_dir`, `rename_path`,
  `restore_to_head`), whose only caller is `code/request.rs`.
- A picture a declared extension draws is asked by identity and name
  (`Host::media`, called only from `declared/media.rs`), never by path:
  the host reads the installed extension's own files, or what its
  programs left in their scratch, kept under `runtime/extensions/pictures/`
  (generated: the next run writes them again). The bytes are bounded
  before they are decoded and the pixels while they are, by
  `shared/picture.rs`, off the drawing thread; an extension never names a
  file of a checkout through an image.
- A declared extension's logic is a Rhai script in `uze-script`, the only
  crate that names `rhai`; `uze-extensions` and `src/ui` never do. The
  declared engine asks the host for an answer by declaration id
  (`Host::answer`, called only from `declared/`), never with argv or script
  text: what runs is what the operator consented to.
- A declared extension's program is started only by
  `uze_core::machine::extension_program`, through this binary's
  `__confine` helper where the machine confines;
  `uze-application`'s `extensions/program.rs` is the one caller, and the
  architecture suite fails on any other. It runs the accepted declaration
  by id, from the path pinned at consent or a copy of the consented
  bytes, never a `PATH` search or the files on disk.
- An extension's actions reach the keyboard through `Action::Extension` and
  a table of that extension's own, resolved after the host's; nothing in
  `uze-extensions` names a chord. An overlay's door may be live
  everywhere (`uze_keys::extension::EVERYWHERE`), and then only on a
  function key or a chord with ctrl or alt.
- The resident tier is the one exception to 'an extension knows nothing
  of the keyboard': a focused resident surface receives raw keys and
  pointer, except the host's escape chord and the pause key, which never
  reach it and cannot be rebound.
- A request leaves only through `fetch`: `uze-script` checks the URL
  (https, a granted host, port 443), `uze-application`'s
  `extensions/fetch.rs` checks it again and starts the system's `curl`
  through the program runner (`program::curl`), with the method, headers,
  body and any secret on its stdin, never in argv or the environment. A
  secret is read only there, from `uze_platform::secrets`, and scrubbed from
  every answer.
- An extension reaches an agent only through the bridge
  (`src/ui/orchestrator/declared/{bridge,agent}.rs`): it is told what
  agents did through `Extension::agent_events`/`agents` under
  `agent:observe`, and what it asks of one is an `Outcome::Agent` the host
  shows, guards and limits. `submit_notice` has two callers, the project's
  instructions and the bridge (ADR-059 amended); the architecture suite
  fails on a third. A notification goes out through `uze_platform::notify`
  from the host, under `notify`, attributed and rate-limited by the
  engine.
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
