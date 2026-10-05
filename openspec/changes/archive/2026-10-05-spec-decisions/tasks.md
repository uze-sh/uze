## 1. Shared places

- [x] 1.1 Move `architect::ArtifactSource` to `uze-extensions/src/shared/` as the declared places (one or many roots, or why none), and have the host hand them to both surfaces
- [x] 1.2 Keep every architect test green against the moved type

## 2. Decision reader

- [x] 2.1 Recognise a record by its numbered name; title and status from front matter or structure, never a heading's words; unit tests for adr-tools, MADR, a Portuguese record, a README and a guide
- [x] 2.2 Read supersession only from front matter `supersedes` / `superseded-by`, header links as references; derive the inverse edge; keep an unresolved reference as written
- [x] 2.3 Catalog decisions across every declared place on the surface's background read, sorted by file name

## 3. Surface

- [x] 3.1 Add `Subject::Decisions`, offered only when a declared place holds a decision; list title and status, superseded ones marked
- [x] 3.2 Render a decision as Markdown with its supersession named above the body; activating opens it in the code surface
- [x] 3.3 Add `Role::Decision`: a numbered record inside a unit is listed after the design
- [x] 3.4 A checkout with decisions and no tool marker opens on the decisions subject instead of the empty state
- [x] 3.5 The subject reaches the screen through the generic subject chips, now drawn in the tab strip's leading slot; covered by `the_navigation_is_the_bars_and_does_not_move_with_the_layout` and the spec surface's own `decisions_*` tests rather than a selector test of its own

## 4. Documentation

- [x] 4.1 `web/content/docs/workspace/spec.mdx`: the decisions subject, the format test (title, status, decision section), where decisions are looked for (`workspace.artifacts`), supersession, and the decision role inside a change
- [x] 4.2 `web/content/docs/reference/project-files.mdx`: `workspace.artifacts` is read by both the architect and the spec surface, each taking the files it recognizes
- [x] 4.3 `plugins/uze/skills/architect/SKILL.md` and any skill that teaches where ADRs go: decisions are found by format in the declared places, so no directory is required
- [x] 4.4 `AGENTS.md` (the `uze-extensions` bullet: what `spec` reads, `shared/` now holding the declared places)

## 5. Gate

- [x] 5.1 `cargo test -p uze-extensions`, then `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`
- [x] 5.2 `openspec validate --all --strict`
- [x] 5.3 Hand-validate on this repository: `docs/adr` listed with statuses, the records inside `project-agent-environment/adr/` shown as that change's decisions, and ADR-019 left unrelated to ADR-054 (its `Supersedes in part:` line is a field no published format defines)

## 6. Navigator follow-up

- [x] 6.1 Slide a navigator row's name that is too long for the column — a decision's title, a change, a file or folder in the spec surface's list — while the pointer is on it, reusing `widget::row::push_trailing_marquee` (the sidebar sections' caption effect) and `FrameMetrics::marquee` to keep the clock turning only while something slides; cover it with a `TestBackend` test that a long name slides under the pointer and is clipped otherwise
