# Decisions taken while the change is open

## D1 — An identity no record backs is still held to an agent's rules

- status:   pending
- task:     2.3
- commit:
- trigger:  `uze agent memory` run with `UZE_AGENT`/`UZE_AGENT_KEY` set but in a directory or a `UZE_HOME` where no record backs the claim (a nested harness, an agent that `cd`'d elsewhere) — the work verbs refuse such a process.
- decided:  it is treated as a launched agent: provenance `an agent, task <id>`, project scope from the directory it stands in, and a `-m` write is a proposal. The gate on machine facts is "the invocation carries a launch identity" (design D2), not "a record proves it".
- rejected: refusing (a stray `cd` makes memory unusable for an agent); treating it as the operator (an injected agent could `cd` out of its checkout to write machine facts directly).
- blast:    one branch in `MemoryCaller::Agent` resolution.

## D2 — The proposal question is a three-answer dialog that opens on the way out

- status:   pending
- task:     4.2
- commit:
- trigger:  the spec asks the operator to accept or discard, but the dialog widget had two answers (Cancel, confirm).
- decided:  `widget::dialog::render_choice` adds an alternative between the way out and the affirmative (Cancel · Discard · Accept); focus moves over all three and the hint names the one Enter takes. The proposal dialog opens on Cancel, like the approval of a project's commands, so an Enter pressed without reading accepts nothing. Cancel leaves the proposal; the next session asks again.
- rejected: Cancel meaning Discard (Escape would delete text an agent wrote, unread); discarding only through the file (the spec's scenario asks the workspace).
- blast:    one widget function, the 11 existing dialogs unchanged.

## D3 — The shim renders the memory section through a free function, not the service

- status:   pending
- task:     4.1
- commit:
- trigger:  the design says the application's launch path renders the section from `MemoryService`, but the launch path is the shim, which never builds a `UzeApplication` (it reads a few `stat`s before `exec`).
- decided:  `uze_application::application::memory::context_sections(home, primary, launching, integrations)` is a free function over the same `gather` the service uses; the shim passes it to `continuity::plan_with` as a closure, so `uze-workspace` composes sections knowing nothing of memory.
- rejected: building the application in the shim (setup and detection reads on every launch); computing the index in `uze-workspace` (the workspace would name memory and the harnesses' sources).
- blast:    one function and one closure.

## D4 — The journey checks the next agent of the same project, not another project's

- status:   pending
- task:     6.3
- commit:
- trigger:  task 6.3 asks for "a launch in another project", but a journey world holds one project, and a second one would be a second world.
- decided:  `12-an-agent-keeps-a-fact-for-later-agents` proves the proposal, the acceptance and the files on disk, and that the next agent's own context lists the accepted machine fact. That the machine scope is shared across projects is a store property, held by `a_proposed_machine_fact_reaches_no_agent_until_accepted` and the machine directory being one per `UZE_HOME`.
- rejected: a second world in the same file (a different world forces a file of its own, `journeys/README.md`).
- blast:    a journey of its own if a cross-project claim is ever wanted.

## D5 — A fact's front matter is written with JSON-quoted values

- status:   pending
- task:     1.2
- commit:
- trigger:  rendering YAML front matter without a YAML writer, for values an agent supplies (a description may hold `:`, `#`, quotes or newlines).
- decided:  every free-text value is written as a JSON string, which YAML reads as itself; the fixed fields go in a fixed order and the body follows untouched. Reading stays `split_frontmatter` + `noyalib`'s serde, already in `uze-core`.
- rejected: hand-escaping YAML scalars (a second escaping rule to get wrong); `noyalib::cst` (contained to `manifest/edit.rs` on purpose).
- blast:    the render function.
