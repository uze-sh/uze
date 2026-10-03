## 1. Make room: Markdown moves to `shared/`

- [x] 1.1 Move `code/markdown.rs` and `code/highlight.rs` to
      `shared/markdown.rs` and `shared/highlight.rs`, update `shared.rs`'s
      doc with why they joined it, and keep every code-surface test green
      with no behaviour change.

## 2. The catalog: dialects as data

- [x] 2.1 `spec/dialect.rs`: `Dialect`, `Collection`, `Shape`, `Tally`,
      `Order`, `Subject` and `Role`, with role patterns as strings matched
      by `classify` (`*` for one segment or a run inside one, `**` for one
      or more), unit-tested (single segment, `**`, a wildcard inside a
      segment, first match wins, fallback to `Other`).
- [x] 2.2 The OpenSpec entry: marker `openspec/`, changes (directories,
      skipping `archive`), specs (`**/spec.md` by capability path),
      archive (newest first), roles proposal/design/tasks/`specs/**/spec.md`.
- [x] 2.3 `spec/catalog.rs`: detect every dialect whose marker exists at a
      checkout root through `Host::list_dir`, walk its collections into
      units and artifacts with roles, order artifacts by role, list
      unnamed files as `Other`, bound the walk's depth, and skip hidden
      entries.
- [x] 2.4 A second dialect read as data through the same path as
      OpenSpec. Done by shipping real entries rather than a test-only Kiro
      one: Spec Kit (#142), then Superpowers and GSD (#175), each with a
      fixture tree in `spec/tests.rs`; `SHIPPED` lists the four.
- [x] 2.5 Fixture tests for OpenSpec: a change with all four roles, a
      change with an unnamed file, a change from a schema that names none
      of the files, `changes/` holding only `archive/`, and no marker at
      all.

## 3. Progress and ownership

- [x] 3.1 `spec/progress.rs`: count `- [ ]`/`- [x]` (and `*`, `+`, `X`)
      at any indentation outside fenced code; no count when there are no
      checkboxes; complete when every one is checked; `receipts` for a
      dialect whose plan is done once its summary is written. Counted for
      changes in flight only.
- [x] 3.2 `spec/ownership.rs`: `touched` reads the paths from `git status
      --porcelain=v1 -z` and, when a target is given, `git log --name-only
      HEAD --not <target> [<target>@{upstream}]`; `lies_in` maps them onto
      units by their path. Tested against a real temporary repository:
      uncommitted file, committed since base, a commit the target's
      upstream already has, the target branch itself.

## 4. The surface

- [x] 4.1 `spec.rs`: `CATALOG` entry, `SpecView` state, `SpecAnswer`
      (checkout, units, ownership) and `read_spec(host, checkout, target)`;
      register it in `ExtensionRegistry::builtin` and add
      `ExtensionHit::Spec`.
- [x] 4.2 `view()`: `Layout::Sidebar`, subjects Changes / Specs / Archive
      (a subject no dialect fills is not offered), one navigator group per
      unit with its progress as a trailing marker and its own-checkout
      mark, rendered Markdown or source by mode, the checkout caption from
      `shared::checkout`, and the three empty states (reading, no layout
      found naming the known ones, nothing in flight).
- [x] 4.3 `handle_command` / `handle_mouse` / `handle_scroll` over the
      existing `Command` and `ViewHit` vocabulary, and `SpecOutcome`
      (`Stay`, `Close`, `OpenPath`) for activation.
- [x] 4.4 `SpecPlace` by unit and artifact name, `place()` and
      `resuming()`, with a test that an archived change is not restored.
- [x] 4.5 Selection on first open: the first own-checkout unit, opened on
      its first document, else the first unit; own-checkout units listed
      first, then in progress, ready to archive, and done (folded).

## 5. Host wiring

- [x] 5.1 `crates/uze-theme`: `Symbol::Spec` in the three shipped glyph
      sets; regenerate the theme schema.
- [x] 5.2 `crates/uze-keys`: `Scope::Spec`, `toggle-spec` bound to `alt+x`
      once in `Scope::Surfaces` (since #184, one chord per action), the
      spec scope's own keys, passing the keymap's conflict test;
      affordances in `tests/architecture/affordances.rs`.
- [x] 5.3 Orchestrator: `open_spec` / `close_spec` in
      `orchestrator/surfaces.rs` (closing the other lenses the way they
      close each other), `spawn_spec_read` in `orchestrator/reads.rs` and
      `absorb_spec` in `orchestrator/answers.rs`, on a thread with the
      checkout carried in the answer and a stale answer dropped, the target
      taken from `delivery_policy(..).target` on the read thread, and
      `spec_places` remembered per checkout beside `architect_places`.
- [x] 5.4 Tab strip: the spec button leading the extension group, before
      architect and code, lit while open; `SpecOutcome::OpenPath` opens the code surface at
      the path, as `ArchitectOutcome::OpenPath` does.
- [x] 5.5 Orchestrator tests: the button opens and closes the surface and
      leads the group, a key typed while open does not reach the pane, a
      late answer after close or for another checkout is dropped, reopening
      restores the place.

## 5b. The sidebar summary

- [x] 5b.1 `spec/summary.rs`: ask Git which changes the checkout touched,
      open only those, total their checkboxes, and answer a
      `view::Section` whose caption is the total and whose rows are the
      changes; no section for a checkout working on none.
- [x] 5b.2 Host: `spawn_spec_summary` / `absorb_spec_summary` on a paced
      re-read (`SPEC_SUMMARY_REFRESH`) for the tab in front, the section
      reserved above the timeline, each section folding on its own, a row
      opening the surface on its change.

## 6. Docs and gate

- [x] 6.1 `docs/architecture/containers.mmd`: the TUI container's
      description names the three lenses; run `cargo test -p
      uze-extensions` and `uze agent artifacts check`.
- [x] 6.2 `crates/uze-extensions/src/lib.rs` crate doc: three extensions,
      and what the spec one is.
- [x] 6.3 User docs: a spec section in
      `web/content/docs/workspace/extensions.mdx` beside the architect
      one, saying which layouts are read; the new key in
      `workspace/keys.mdx`; the term in `reference/glossary.mdx`.
- [x] 6.4 `make check` green (fmt, clippy `--all-targets`, workspace tests,
      deny, artifacts).
- [x] 6.5 Hand validation on this repository: open the surface in an
      agent worktree that is editing a change, and in the operator's
      checkout.
