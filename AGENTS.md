# AGENTS.md

This file provides guidance to agentic coding tools (Claude Code, Codex,
OpenCode, Antigravity CLI) when working with code in this repository. It
holds what is true whatever the task touches. What is true only under one
path or for one kind of change is a rule under `.agents/rules/`, which
arrives when it is relevant; check them with `uze agent rules check`.

## What this is

uze is a Rust CLI: the package manager and workspace for coding agents
(Claude Code, Codex, OpenCode, Antigravity CLI). You install a
plugin once; uze stores its bytes centrally and delivers it through each
harness's most native mechanism — a real plugin where one exists, native
capabilities where it doesn't, a safe adapter only as a last resort. It
treats one project context file, `AGENTS.md`, as the baseline every harness
reads, and writes nothing into it: what only means something inside UZE
reaches an agent through its launch, and what a plugin tells every agent is
an `always_on` rule.

This repository is itself the official uze marketplace: `marketplace.json` +
`plugins/**` at the root, with `plugins/uze` (the `uze:init`, `uze:rules`, `uze:worktree`,
`uze:architect` and `uze:author` Skills) as the one official plugin today.

## Commands

```bash
cargo build --locked --bin uze                          # debug build
cargo build --locked --release --bin uze                 # release build
make install                                              # install into ~/.cargo/bin (force-rebuild, no version bump, --features telemetry)
cargo run --quiet --bin uze -- --version

cargo test --workspace --no-fail-fast                     # full workspace suite (default-members is the root crate only); CI runs the same set with cargo nextest
cargo test -p uze-core some_test_name                      # single crate / targeted test during iteration
cargo test --test packages containment                      # one domain suite (tests/<domain>/main.rs), filtered

cargo fmt --check
cargo clippy --all-targets -- -D warnings                  # what CI and `make lint` both run
cargo llvm-cov --workspace --summary-only --fail-under-lines 68 --fail-under-regions 69

cargo deny check                                           # licences, advisories, bans, sources (deny.toml); part of `make check`
make attributions                                          # regenerate CREDITS.md (about.hbs + Cargo.lock); CI fails when it drifts
uze agent rules check                                      # every rule under .agents/rules is deliverable

UZE_LOG=info uze doctor                                    # spans and events as text (see docs/observability.md)
make observe                                               # local Jaeger (UI :16686); then OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 cargo run --features telemetry --bin uze -- doctor
```

`Makefile` wraps all of the above (`make build`, `make test`, `make check`,
etc. — run `make help` for the full list). `ci.yml` is the source of truth
for what actually gates a merge: its `Gate` job is the one required check,
and it calls `journeys.yml` and `conformance.yml` (the Lab) as reusable
workflows when a change reaches their paths; both also run nightly. Treat
`make check` as a close local proxy, not a guarantee of parity.

Run the CLI itself with `cargo run --bin uze -- <args>` or `./target/debug/uze <args>` after a build; `uze workspace` launches the terminal UI; `uze` with no args prints help.

`.cargo/config.toml` keeps `jobs = 2`, and `CARGO_BUILD_JOBS=2` /
`RUST_TEST_THREADS=4` are a courtesy to a 4-vCPU WSL VM. A WSL session
that dies mid-test is a kill by signal, not memory: see the
`ending-processes` rule.

## Code style

Keep the code clean and self-documenting. Prefer expressive names and small,
focused functions over explanatory comments. Add a comment only when it
explains a non-obvious *why* — the rationale behind a decision, an invariant,
a workaround, or a subtle constraint the code alone cannot convey. Never
restate what the code already says; write intent, not implementation.

Anything a person sees — a TUI surface, a CLI report or help text, the
words in either — follows the `design-system` skill
(`.agents/skills/design-system/`): read it before the change, run its
checklist before handing back, and when the operator corrects something
visual, UX, DX or wording, write the rule into it in the same change as
the fix.

Cross-platform behaviour is a concept in `uze-platform` with one
implementation per platform; a `cfg!` at a call site anywhere else is a
missing concept there. A new dependency is chosen by provenance; the
`dependencies` rule says how, and it arrives with `Cargo.toml`.

## Documentation hygiene

Do not create permanent Markdown files for implementation notes,
investigation logs, temporary plans, or task reports unless explicitly
requested — prefer ephemeral working notes. Research becomes documentation
only when the information is likely to remain useful, is not better
represented by code/tests/ADR/OpenSpec, and has a clear canonical owner.
Before adding a document, update the existing canonical one when
appropriate; new docs must state their durable purpose and owner, and
prefer updating an existing document over creating a new file.

A convention that holds only under one path is a rule in `.agents/rules/`
(the `uze:rules` Skill says how), never a new section here. A procedure
with steps is a Skill under `.agents/skills/`.

## Workspace layout

Cargo workspace, edition 2024, MSRV 1.97. Single version source:
`[workspace.package].version` in the root `Cargo.toml` — every crate
inherits it; bump it before any binary is distributed (dev rebuilds don't
need to). Each crate's own `lib.rs` doc says what belongs in it; the rule
for a crate arrives when you touch it.

- `.` (binary crate `uze`) — the CLI (`src/main.rs` parses and
  dispatches; `src/cli/` holds the grammar, the help and one module per
  command family), the terminal UI (`src/ui.rs`, `src/ui/`, with its
  chrome vocabulary in `src/ui/widget/`), the runtime PATH shim
  (`src/shim.rs`), and `src/command_performance.rs`.
- `crates/uze-core` — the shared foundation and the package manager's
  harness-agnostic domain, in five concerns: `package/`, `capability/`,
  `delivery/`, `project/`, `machine/`.
- `crates/uze-workspace` — the workspace's domain: tasks, checkouts,
  conversations, landing, the `workspace:` declaration and its approval.
- `crates/uze-application` — the product-facing facade
  (`UzeApplication`) orchestrating Core + Integrations into lifecycle
  operations and the workspace's services.
- `crates/uze-integrations` — one module per harness (`claude`, `codex`,
  `opencode`, `antigravity`) implementing `IntegrationPort`, plus
  `shared/`; `registry::IntegrationRegistry` is the single composition
  root naming the concrete types.
- `crates/uze-git` — the one transport for speaking to the Git binary;
  reports Git's exit code rather than classifying it.
- `crates/uze-document` — the one rule for reading a record another build
  wrote: shape, ladder, floor.
- `crates/uze-platform` — what UZE asks the operating system, answered
  once per platform.
- `crates/uze-terminal` — the local terminal runtime: a server owning the
  pseudoterminals, so a pane survives a client leaving.
- `crates/uze-theme`, `crates/uze-keys` — the design and input
  vocabularies (`Token`, `Symbol`; `Action`, `Chord`, `Keymap`).
- `crates/uze-extensions` — built-in TUI extensions (`code`, `architect`,
  `spec`), which describe and never draw.
- `crates/uze-testkit` — shared test infrastructure; `tests/` holds the
  domain suites (taxonomy in `tests/README.md`).
- `conformance/` — the Harness Conformance Lab (Python, Docker): real
  harness binaries against a synthetic provider.
- `journeys/` — the product journeys, the third test tier: a user's flow
  through the real CLI and TUI, checked against the machine it left.
- `playground/` — disposable worlds to try this checkout by hand
  (`make playground-windows`, `make playground-linux`).
- `docs/adr/` — numbered architecture decision records; read the recent
  ones before a structural change.
- `docs/architecture/*.mmd` — the architecture as Mermaid, the one diagram
  source; `uze agent artifacts check` (part of `make check`) fails on one
  that does not draw. Writing them is the `uze:architect` Skill's subject.
- `docs/architecture/invariants.md` — the properties the architecture
  holds today, each tied to the test that proves it: the canonical list of
  "do not break this".
- `openspec/` — change proposals; `openspec validate --all --strict` gates
  CI and warnings fail it too, so a requirement body over 500 characters
  or a placeholder Purpose is fixed in the change that wrote it.

## Architecture

Dependency direction is one-way and enforced by tests, not just convention:

```
CLI/TUI (src/)  ──uses──▶  uze-extensions   (presentation: an extension
      ↓                                      describes, src/ui renders)
uze-application  (orchestration: add/install/remove/update/context)
      ↓                 ↓
      ↓          uze-workspace (the workspace's domain: tasks, checkouts,
      ↓                 ↓       landing, conversations)
uze-core         (shared foundation + the package manager: Package, Store,
      ↑           Engine, Router, IntegrationPort, capability/exposure model)
uze-integrations (Claude, Codex, Antigravity, OpenCode — implement IntegrationPort)

uze-git          (transport, no domain — used by core, workspace, integrations, application)
uze-terminal     (local terminal runtime — reaches only uze-platform and uze-document)
leaves           uze-platform, uze-document, uze-theme, uze-keys (name no domain)
```

`uze-core` never depends on `uze-workspace`: that is how the package
manager works without the workspace, and the compiler is what says so.

The rules below hold wherever you are in the tree. Each fails the build
when broken; `tests/architecture/layering.rs` and
`tests/integrations/identity.rs` are where.

- **`src/` must not name `uze_core::` or `uze_integrations`.**
  Presentation consumes read models from `uze-application`; reaching into
  the domain makes every domain change ripple into the frontend. The debt
  is **zero**: every remaining reach is `sanctioned` and named. Need
  something the domain has? Add a read model, a service method or a
  re-export to `uze-application`. **Never raise a budget.** (The compiler
  cannot enforce this, because `src/shim.rs` and
  `src/bin/uze-harness-matrix.rs` share the binary crate and legitimately
  name the domain.)
- **Only `uze-integrations` names a harness.** `uze-core`,
  `uze-application` and `src/` never name Claude, Codex, OpenCode or
  Antigravity (`core_never_names_a_vendor_harness`,
  `application_never_names_a_vendor_harness`,
  `cli_and_tui_never_name_a_vendor_harness`). A new harness is a new
  integration vertical, one registry entry, conformance and docs, and no
  change anywhere else.
- **Speak to Git through `uze-git`.** Never spawn `git` directly: reads go
  through `read`, writes through `write`. The exceptions are sanctioned by
  name: `acquisition::git`, which clones *untrusted* remote repositories
  and strips the environment, and the testkit's loopback forge. Inside
  `uze-workspace`, Git is reached only through `crate::git`.
- **Every path UZE owns is named in `UzeHome`**, and what it persists
  belongs to the tier its deletion costs; the `persistence-tiers` rule
  says which.
- **`AGENTS.md` is the operator's.** It is the portable baseline every
  harness reads natively, and no production code writes it except
  `uze-application`'s `retired_regions.rs`, which takes out the regions
  earlier releases wrote, in a main working tree only
  (`a_retired_region_cleanup_is_the_only_writer_of_agents_md`). This
  repository's own `AGENTS.md` is maintained directly, since this is uze's
  own source.
- **Nothing the workspace client draws waits on a repository**: every Git
  read and unbounded operation from `src/ui/` runs on a thread and answers
  through a channel.
- **An extension never draws, and reaches nothing it was not handed**: it
  returns a view, and every capability arrives through
  `uze_extensions::Host` (ADR-041).
