## 1. Reader

- [x] 1.1 `spec/milestone.rs`: the open milestone from `STATE.md` front matter and body, phases from `ROADMAP.md`, plans and waves from the phase directories; unit tests
- [x] 1.2 The next step, per phase and for the milestone, following GSD's routing; a closed milestone is not read

## 2. Surface

- [x] 2.1 `Subject::Milestone`, offered in front of the others only while a milestone is open; the surface lands on it when the agent has no unit of its own
- [x] 2.2 Rows: overview, then one per phase with plans done out of all and the current one marked; overview and phase rendered as Markdown
- [x] 2.3 A step menu on activation, the next step highlighted; picking one answers `SpecOutcome::Prompt`

## 3. Host

- [x] 3.1 `IntegrationPort::skill_sigil`, `/` by default, overridden where a harness differs; `AgentIdentity.skill_sigil`
- [x] 3.2 The orchestrator closes Spec and pastes the spelled step into the focused agent, bracketed, control characters stripped, never sent

## 4. Documentation

- [x] 4.1 `web/content/docs/workspace/spec.mdx`: the milestone subject, its rows, the steps and how they are typed

## 5. Gate

- [x] 5.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`
- [x] 5.2 `openspec validate --all --strict`
