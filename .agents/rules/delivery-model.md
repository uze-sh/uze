---
trigger: glob
globs: "**/crates/uze-core/**, **/crates/uze-integrations/**, **/crates/uze-application/**, **/src/shim.rs, **/src/bin/**"
---

How a package reaches a harness. `docs/architecture/invariants.md` names
the test behind each property.

- **Store** (`uze_core::store`) owns installed package bytes and is the
  single source of truth; it never writes anything a harness reads, and
  integrations never mutate it.
- **One Skill capability; invocation policy is its semantics.** Explicit
  user action vs. background knowledge is the `invoke: {model, user}` block
  in SKILL.md (ADR-030), never a second capability kind: no canonical
  `Command`, no `commands/` surface. Integrations translate the policy;
  Store bytes stay verbatim.
- **Portable Hooks** (ADR-033, compiled per ADR-040): one authored
  `hooks.json`; handler contract is `HOOK_*` environment in, exit code out
  (0 allows, 3 denies with the reason on stderr, anything else fails by the
  group's effect), bounded output, a per-handler timeout, first-deny-wins,
  fail-open for observational vs fail-closed for deny/ask/transform. It is
  compiled at install time into the delivered artifact (a POSIX `sh` or
  PowerShell wrapper, the owned OpenCode bridge) with no `uze` on the
  execution path; a platform with no template is reported Unsupported,
  never adapted. Every projection is receipt-owned, content-identity
  inspected, and never touches foreign hooks, plugins or order.
- **Native > Generated Native > Safe Adaptation > Unsupported**, per
  capability per harness. "Native" means an officially supported mechanism
  preserving canonical semantics, not the same physical primitive across
  vendors.
- **Derived artifacts are non-authoritative and rebuildable** from the
  Store + Engine alone.
- **Receipts drive lifecycle safety.** Every managed filesystem artifact
  (symlink, generated directory, config entry) has a typed receipt; drift
  or an unreadable ledger blocks destructive mutation rather than
  authorizing one; removal inspects current state before detaching.
- **The runtime PATH shim** (`src/shim.rs`, `uze_core::harness_runtime`)
  projects `AGENTS.md` into a harness without writing into the project and
  must never invoke itself (`tests/integrations/runtime_boundary.rs`).
- **`agents.yaml` is authored, `agents.lock` is derived**: the lock holds
  only a commit per marketplace and a digest per package, so it is
  regenerated, never repaired. A marketplace is a Git repository whether
  spelled as a URL or a local path. `uze install` converges the manifest in
  both directions: a plugin dropped from `agents.yaml` leaves the lock,
  while the Store and every harness keep it (`uze remove <plugin> -m`
  takes it off the machine).
- `IntegrationPort` stays one trait proven by conformance across all
  harnesses; splitting it per capability was considered and rejected absent
  a concrete implementation problem.
- A new harness needs no change to Store, Engine, Router, application, CLI
  or TUI: one integration vertical, one `IntegrationRegistry` entry,
  conformance, and docs.
