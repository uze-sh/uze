## 1. Manifest root (uze-core)

- [x] 1.1 Replace `worktrees` and `artifacts` in `ProjectManifest` with one opaque `workspace` value, and `Section::Worktrees`/`Section::Artifacts` with `Section::Workspace`
- [x] 1.2 Carry no compatibility for root `worktrees:` / `artifacts:`: they meet the ordinary unknown-field error
- [x] 1.3 Reduce `SCAFFOLD` to the header and the commented `marketplaces:` example; update the scaffold tests and `module-boundary`'s "first install declares no policy" test to assert no workspace text at all
- [x] 1.4 Point `manifest::set_scalar` callers at `workspace.<key>` and test that writing `workspace.delivery` into a file without the section creates it and leaves `marketplaces:` byte-identical

## 2. Workspace declaration (uze-workspace)

- [x] 2.1 Add `WorkspaceDeclaration` (flat, `deny_unknown_fields`) and derive `WorktreePolicy` and the artifact roots from it in `declaration.rs`
- [x] 2.2 Rename the serialized values: `AgentPlacementDefault` to `always | manual`, `completion` to `delivery`; keep `CompletionBehavior::abi_name` unchanged
- [x] 2.3 Make `artifacts` accept one directory or a list, validating each entry on its own
- [x] 2.4 Update every malformed-manifest message that names a key (`worktrees.slots`, `worktrees.link`, ...) to `workspace.<key>`
- [x] 2.5 Update the projected AGENTS.md region text and the `uze agent work name` refusal that names `worktrees.branch`

## 3. Surfaces

- [x] 3.1 Architect catalog walks every declared root, an origin carrying its root when there is more than one; an entry that leaves the project refuses the declaration by name
- [x] 3.2 `uze agent artifacts check` reads every declared root
- [x] 3.3 Confirm the TUI labels "To worktree" / "To worktree, no changes" and their descriptions in `uze-keys` match the specs, and the menu tests in `src/ui/orchestrator/tests.rs`
- [x] 3.4 Policy popup reads and writes `workspace.delivery`, attributing it to `agents.yaml`

## 4. Project files and fixtures

- [x] 4.1 Move this repository's `agents.yaml` to the new shape (`worktree: always`, `delivery: pr`, `branch: conventional`, `artifacts: docs` so both `docs/architecture` and `docs/adr` are read)
- [x] 4.2 Update test fixtures (`tests/_fixtures`, testkit) and journeys that write `worktrees:` or `artifacts:`
- [x] 4.3 Amend ADR-017's file-layout text in the open `project-agent-environment` change in place

## 5. Documentation

- [x] 5.1 `web/content/docs/reference/project-files.mdx`: rewrite the `agents.yaml` section around root `marketplaces:` and one flat `workspace:` section, documenting every workspace key there now that the scaffold does not, plus the one-pass migration the refusal prints
- [x] 5.2 `web/content/docs/workspace/agents.mdx` and `keys.mdx`: `worktree: always | manual`, `delivery`, and the "To worktree" / "To worktree, no changes" actions in place of Isolate / Isolate clean
- [x] 5.3 `web/content/docs/workspace/architect.mdx`: `workspace.artifacts` as one directory or a list, places not kinds
- [x] 5.4 `web/content/docs/reference/agent-cli.mdx` and `glossary.mdx`: every `worktrees.<key>` reference becomes `workspace.<key>`
- [x] 5.5 `plugins/uze/skills/worktree/SKILL.md`, `plugins/uze/skills/architect/SKILL.md`, `plugins/uze/README.md` and `docs/capabilities/uze-skill.md`: the keys and the action names the skills teach
- [x] 5.6 `AGENTS.md` (Workspace layout, Concurrent work isolation, `uze agent` bullet) and `docs/architecture/invariants.md`: the `workspace:` section and `workspace.branch`
- [x] 5.7 Grep the repository for `worktrees.`, `worktrees:`, `artifacts.path`, `in-place`, `isolated` as a value, and "Isolate" as a label; nothing outside archived changes and accepted ADRs still names them

## 6. Gate

- [x] 6.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`
- [x] 6.2 `make artifacts` and `openspec validate --all --strict`
- [ ] 6.3 Hand-validate in the TUI: a manual project moves an agent with "To worktree"; an `always` project places at launch; an old-shape file is refused with the new path named
