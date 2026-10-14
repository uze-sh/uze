## Why

The 2026-10-13 harness survey (Claude Code 2.1.296, Codex 0.162.1, OpenCode
2.0.28, Antigravity CLI 1.3.4) found one break and several beliefs in
`uze-integrations` the vendors have since overtaken: OpenCode lets the
caller name a session, Claude can block a call whose hook fails to run, Antigravity's
plugin census changed shape and its docs now say plugins carry hooks, and
two harnesses now hand the model tools that make worktrees UZE does not
account for. Every gate's recorded evidence is also older than the facts
the integrations declare, so the Lab has not proved the versions UZE
claims to support.

Everything the survey found about project instruction files (Claude
reading `AGENTS.md`, PR #154, `CLAUDE.md`/`GEMINI.md`) is the
`resolve-instruction-files` change; this one touches none of it. Each item is measured against the
real harness before it is implemented; an item the measurement refutes is
dropped from the change, not shipped on the vendor's word.

## What Changes

- **Antigravity census (break):** the installed-plugin census reads the
  1.3.4 `agy plugin list --json` shape and sees plugins installed from the
  vendor's marketplace, so a foreign same-name plugin of either origin is
  refused instead of overwritten.
- **OpenCode continuity is assigned:** UZE names the OpenCode session at
  launch (`--session ses_…`) instead of reading it back afterwards, and a
  recorded conversation is looked up by its id rather than found in the
  first page of a listing.
- **Hooks fail closed on Claude even when the handler cannot run:**
  deny/ask/transform groups carry the vendor's native block-on-failure
  marker, so a missing wrapper or a native timeout blocks the call.
- **Harness worktree tools are accounted for:** a harness may declare
  where it keeps worktrees outside the checkout (Codex's home), and
  worktrees an agent creates through Codex's or OpenCode's own tools are
  classified as that harness's isolation, never as the operator's.
- **Measured, then decided:** whether Antigravity loads a plugin's root
  `hooks.json` and `agents.json` (if so, hooks and agents move into the
  generated plugin); whether Codex's Windows PreToolUse now fires for shell
  (if so, the declared `Unfired` goes). The Antigravity hook-delivery Lab
  check that the docs cite but that does not exist is created either way.
- **Small corrections:** Antigravity MCP passes `url` through as the
  primary key; OpenCode counts `disable-model-invocation` as a hand-written
  invocation control; stale comments and README limitations are fixed; the
  `harness-watch` skill reads OpenCode V2 from the `dev` branch.
- **Harness version floor:** each integration declares the oldest
  version its deliveries are written for, and `doctor`/`status` report a
  harness below it by name; no older path is kept alive below a floor.
- **Evidence:** all four gates are re-recorded on the current releases.

Out of scope, each its own decision: project instruction files
(`resolve-instruction-files`), Claude Mods, a portable plugin `bin/`
capability, and OpenCode's native `execute.before` refusal and
`permission.assert` (not released yet).

## Capabilities

### Modified Capabilities
- `agent-session-continuity`: a harness that accepts a caller-named
  conversation is named at launch, and a recorded conversation is looked
  up by its own identifier.
- `harness-provisioning`: a harness below the version UZE needs is
  reported, never worked around.
- `portable-hooks`: the portable ABI's failure semantics hold when the
  handler cannot start or times out, where the target offers it; the stale
  "no target preserves input transformation" clause goes.
- `checkout-accounting`: harness isolation may live outside the checkout.
- `antigravity-primary-google-harness`: the foreign-plugin refusal covers
  marketplace installs, detach refuses a same-name twin, and the generated
  MCP config keeps `url`.

## Impact

- `crates/uze-integrations`: `antigravity/plugin.rs`, `antigravity/generate.rs`,
  `antigravity/hooks.rs`, `hooks/entries.rs`, `hooks/event_entry.rs`,
  `opencode/session.rs`, `opencode.rs`, `codex.rs`, `codex/hooks.rs`, FACTS tables.
- `crates/uze-core`: the integration port's worktree-place declaration,
  minimum harness version and session-id minting; checkout accounting.
- `crates/uze-application`, `src/`: `status` and `doctor` for the version
  floor.
- `conformance/`: new checks (Antigravity hook delivery, OpenCode assigned
  session, Claude block-on-failure), re-recorded
  evidence, `expected.json`.
- Docs: OpenCode/Antigravity READMEs, `antigravity-compatibility.md`,
  `portable-hooks.md`, `invariants.md`, harness matrix; ADR-027, ADR-040
  and ADR-047 at archive.
